#![windows_subsystem = "windows"]
mod update;
mod native;
mod profile;
mod health;
mod diagnostics;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, process::Command, sync::mpsc::{self, Receiver}};

const LIANLI_VID: u16 = 0x1A86;
const LIANLI_PID: u16 = 0x2107;

#[derive(Clone, Serialize, Deserialize)]
struct Group { name: String, count: usize, speed_percent: u8, brightness: u8, rgb: [u8; 3] }
#[derive(Clone, Serialize, Deserialize)]
struct Config { groups: Vec<Group>, strimer_brightness: [u8; 2] }
impl Default for Config {
    fn default() -> Self {
        Self {
            groups: vec![
                Group { name: "TL Wireless LCD · group A".into(), count: 3, speed_percent: 65, brightness: 70, rgb: [100, 150, 255] },
                Group { name: "TL Wireless LCD · group B".into(), count: 2, speed_percent: 65, brightness: 70, rgb: [100, 150, 255] },
                Group { name: "TL Wireless · non-LCD".into(), count: 3, speed_percent: 65, brightness: 70, rgb: [100, 150, 255] },
            ],
            strimer_brightness: [70, 70],
        }
    }
}
fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("LianLite").join("config.json")
}
fn save(config: &Config) -> Result<(), String> {
    let path = config_path();
    fs::create_dir_all(path.parent().ok_or("Missing config parent")?).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}
#[derive(Serialize)]
struct HidEntry {
    vid: String, pid: String, manufacturer: String, product: String,
    interface: i32, usage_page: u16, usage: u16,
}
fn scan_hid() -> Result<Vec<HidEntry>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    Ok(api.device_list().filter(|d| d.vendor_id() == LIANLI_VID && d.product_id() == LIANLI_PID)
        .map(|d| HidEntry {
            vid: format!("{:04X}", d.vendor_id()), pid: format!("{:04X}", d.product_id()),
            manufacturer: d.manufacturer_string().unwrap_or("").into(),
            product: d.product_string().unwrap_or("").into(),
            interface: d.interface_number(), usage_page: d.usage_page(), usage: d.usage(),
        }).collect())
}
fn read_lianli_processes() -> String {
    // Read-only diagnostics. Never stop services or processes automatically.
    match Command::new("tasklist").args(["/FO", "CSV", "/NH"]).output() {
        Ok(out) if out.status.success() => {
            let listing = String::from_utf8_lossy(&out.stdout);
            let found: Vec<_> = listing.lines().filter(|l| {
                let s = l.to_ascii_lowercase();
                s.contains("l-connect") || s.contains("lconnect") || s.contains("lianli")
            }).collect();
            if found.is_empty() { "No matching process names found. Services may still be active.".into() }
            else { found.join("\n") }
        }
        Ok(out) => format!("tasklist failed: {}", out.status),
        Err(e) => format!("Cannot run tasklist: {e}"),
    }
}
enum UpdateMessage { Checked(Option<update::Available>), Staged(PathBuf) }
enum NativeMessage { Scanned(Vec<native::NativeDevice>), SystemScanned(String), AuraInspected(String) }
struct App {
    config: Config, hid: Vec<HidEntry>, process_report: String, status: String,
    update_rx: Option<Receiver<Result<UpdateMessage, String>>>,
    available: Option<update::Available>, staged: Option<PathBuf>,
    native_rx: Option<Receiver<Result<NativeMessage, String>>>,
    native_devices: Vec<native::NativeDevice>,
    native_windows_json: String,
    aura_pnp_json: String,
    global_color: [u8;3],
    global_brightness: u8,
    last_summary: String,
}
impl Default for App {
    fn default() -> Self {
        let mut config: Config = fs::read(config_path()).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        profile::validate(&mut config);
        Self { config, hid: Vec::new(), process_report: "Not checked yet".into(),
            status: "Read-only hardware mode. No USB commands will be sent.".into(),
            update_rx: None, available: None, staged: None,
            native_rx: None, native_devices: vec![], native_windows_json: String::new(), aura_pnp_json: String::new(),
            global_color: [100,150,255], global_brightness: 70, last_summary: String::new() }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Receive network completion without blocking UI thread.
        if let Some(rx) = &self.update_rx {
            match rx.try_recv() {
                Ok(Ok(UpdateMessage::Checked(Some(info)))) => {
                    self.status = format!("Update v{} available.", info.version);
                    self.available = Some(info); self.update_rx = None;
                }
                Ok(Ok(UpdateMessage::Checked(None))) => {
                    self.status = "Already up to date.".into(); self.available = None; self.update_rx = None;
                }
                Ok(Ok(UpdateMessage::Staged(path))) => {
                    self.status = "Update downloaded and verified; ready to install.".into();
                    self.staged = Some(path); self.update_rx = None;
                }
                Ok(Err(e)) => { self.status = format!("Update error: {e}"); self.update_rx = None; }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "Update worker stopped unexpectedly.".into(); self.update_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(150));
                }
            }
        }
        if let Some(rx) = &self.native_rx {
            match rx.try_recv() {
                Ok(Ok(NativeMessage::Scanned(devices))) => {
                    self.status = format!("Native: {} supported HID entries found (read-only)", devices.len());
                    self.native_devices = devices; self.native_rx = None;
                }
                Ok(Ok(NativeMessage::SystemScanned(report))) => {
                    self.native_windows_json = report;
                    self.status = "Windows GPU/RAM inventory retrieved (read-only)".into();
                    self.native_rx = None;
                }
                Ok(Ok(NativeMessage::AuraInspected(report))) => {
                    self.aura_pnp_json = report;
                    self.status = "ASUS Aura PnP inspection completed (read-only)".into();
                    self.native_rx = None;
                }
                Ok(Err(e)) => { self.status = format!("Native scan error: {e}"); self.native_rx = None; }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Native scan worker disconnected".into(); self.native_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LianLite 0.8.0  ·  Ultra Minimal");
            ui.label("Windows 11 | Native RGB controller research · no OpenRGB required");
            ui.colored_label(egui::Color32::YELLOW, "Lian Li fan/LCD control remains disabled. Keep L-Connect services running.");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Software updates · GitHub Releases");
                let updater_status_path = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
                    .join("LianLite").join("updater-last-status.txt");
                if let Ok(last_status) = fs::read_to_string(&updater_status_path) {
                    ui.collapsing("Last update diagnostics", |ui| {
                        ui.monospace(last_status);
                        ui.small(format!("Log: {}", updater_status_path.display()));
                    });
                }
                ui.label("Release repository: quanghuy2203/LianLite");
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.update_rx.is_none(), egui::Button::new("Check for updates")).clicked() {
                        let (tx, rx) = mpsc::channel(); self.update_rx = Some(rx);
                        std::thread::spawn(move || { let _ = tx.send(update::check().map(UpdateMessage::Checked)); });
                    }
                    if let Some(a) = self.available.clone() {
                        if ui.add_enabled(self.update_rx.is_none(), egui::Button::new(format!("Download v{}", a.version))).clicked() {
                            let (tx, rx) = mpsc::channel(); self.update_rx = Some(rx);
                            std::thread::spawn(move || { let _ = tx.send(update::stage(&a).map(UpdateMessage::Staged)); });
                        }
                    }
                    if let Some(path) = self.staged.clone() {
                        if ui.button("Install & restart").clicked() {
                            match update::launch_helper(&path) {
                                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                                Err(e) => self.status = format!("Cannot install: {e}"),
                            }
                        }
                    }
                });
                if self.update_rx.is_some() { ui.spinner(); ui.label("Working in background..."); }
                ui.separator();
                ui.heading("Profiles · Local preview only");
                ui.horizontal(|ui| {
                    for preset in [profile::Preset::Silent, profile::Preset::Balanced, profile::Preset::Performance] {
                        if ui.button(preset.name()).clicked() {
                            profile::apply(&mut self.config, preset);
                            self.status = format!("{} local preview applied. Hardware unchanged.", preset.name());
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Global RGB preview:");
                    let mut global = egui::Color32::from_rgb(self.global_color[0], self.global_color[1], self.global_color[2]);
                    if ui.color_edit_button_srgba(&mut global).changed() {
                        self.global_color = [global.r(), global.g(), global.b()];
                    }
                    if ui.button("Sync preview color").clicked() {
                        profile::all_same_color(&mut self.config, self.global_color);
                        self.status = "Preview colors matched across all fan groups. No USB writes.".into();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.global_brightness, 0..=100).text("Global brightness"));
                    if ui.button("Apply brightness preview").clicked() {
                        profile::all_brightness(&mut self.config, self.global_brightness);
                        self.status = "Preview brightness set on fans and Strimers. Hardware unchanged.".into();
                    }
                });
                ui.separator();
                ui.heading("Fan groups");
                for group in &mut self.config.groups {
                    ui.group(|ui| {
                        ui.label(format!("{} ({} fans)", group.name, group.count));
                        ui.add(egui::Slider::new(&mut group.speed_percent, 20..=100).text("Speed % · preview"));
                        ui.add(egui::Slider::new(&mut group.brightness, 0..=100).text("RGB brightness · preview"));
                        let mut color = egui::Color32::from_rgb(group.rgb[0], group.rgb[1], group.rgb[2]);
                        if ui.color_edit_button_srgba(&mut color).changed() { group.rgb = [color.r(), color.g(), color.b()]; }
                    });
                }
                ui.heading("Strimer Wireless");
                for (i, brightness) in self.config.strimer_brightness.iter_mut().enumerate() {
                    ui.add(egui::Slider::new(brightness, 0..=100).text(format!("Strimer {} · brightness preview", i + 1)));
                }
                if ui.button("Save local profile").clicked() {
                    self.status = match save(&self.config) { Ok(()) => format!("Saved to {}", config_path().display()), Err(e) => e };
                }
                ui.separator();
                ui.heading("Native hardware detection");
                ui.label("ASUS ROG Strix B650E-F · HID candidate 0B05:19AF");
                ui.label("ZOTAC RTX 4070 Ti Trinity OC · SPECTRA protocol not yet confirmed");
                ui.label("Lexar ARES V2 DDR5 · RGB chip and SMBus access not yet verified");
                ui.colored_label(egui::Color32::YELLOW, "All native drivers are read-only. No RGB, SMBus, GPU or fan writes.");
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Detect ASUS + Lian Li HID")).clicked() {
                        let (tx,rx)=mpsc::channel(); self.native_rx=Some(rx);
                        std::thread::spawn(move || { let _=tx.send(native::scan_hid().map(NativeMessage::Scanned)); });
                    }
                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Detect GPU + RAM")).clicked() {
                        let (tx,rx)=mpsc::channel(); self.native_rx=Some(rx);
                        std::thread::spawn(move || { let _=tx.send(native::scan_gpu_and_memory().map(NativeMessage::SystemScanned)); });
                    }
                    if ui.button("Export native report").clicked() {
                        self.status = if self.native_windows_json.is_empty() {
                            "Use Detect GPU + RAM before exporting native report".into()
                        } else {
                            match native::export_report(&self.native_devices, &self.native_windows_json) {
                                Ok(path) => format!("Saved {}. Review IDs before sharing.",path.display()),
                                Err(e) => format!("Export failed: {e}"),
                            }
                        };
                    }
                });
                if self.native_rx.is_some() { ui.spinner(); }
                for d in &self.native_devices {
                    ui.label(format!("{}: {} ({}) IF={} UsagePage={:04X}",d.kind,d.name,d.id,d.interface,d.usage_page));
                    ui.small(d.driver);
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Inspect ASUS Aura (read-only)")).clicked() {
                        let (tx,rx)=mpsc::channel(); self.native_rx=Some(rx);
                        std::thread::spawn(move || { let _=tx.send(native::inspect_aura_pnp().map(NativeMessage::AuraInspected)); });
                    }
                    if ui.add_enabled(!self.aura_pnp_json.is_empty(), egui::Button::new("Export ASUS report")).clicked() {
                        self.status = match native::export_aura_report(&self.aura_pnp_json) {
                            Ok(path) => format!("ASUS PnP report saved: {}. Review InstanceId before sharing.", path.display()),
                            Err(e) => format!("ASUS export failed: {e}"),
                        };
                    }
                });
                ui.collapsing("ASUS Aura Windows PnP properties", |ui| {
                    ui.monospace(&self.aura_pnp_json);
                });
                ui.collapsing("GPU/RAM inventory (read-only)", |ui| {
                    ui.monospace(&self.native_windows_json);
                });
                ui.separator();
                ui.heading("Update and installation health");
                if ui.button("Check install / updater").clicked() {
                    match health::inspect() {
                        Ok(h) => self.status = format!(
                            "Updater: {} | Backup: {} | Older backup: {} | Pending: {}",
                            if h.updater_present {"present"} else {"missing"},
                            h.backup_present, h.old_backup_present, h.pending_present
                        ),
                        Err(e) => self.status = format!("Health check failed: {e}"),
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Export privacy-safe diagnostic summary").clicked() {
                        match health::inspect() {
                            Ok(h) => {
                                let summary = diagnostics::summary(
                                    &self.native_devices, &self.native_windows_json, &self.aura_pnp_json, &h
                                );
                                self.last_summary = serde_json::to_string_pretty(&summary).unwrap_or_default();
                                self.status = match diagnostics::export(&summary) {
                                    Ok(path) => format!("Saved {}. No PnP instance IDs included.", path.display()),
                                    Err(e) => format!("Summary export error: {e}"),
                                };
                            }
                            Err(e) => self.status = format!("Summary error: {e}"),
                        }
                    }
                });
                ui.collapsing("Shareable summary preview", |ui| { ui.monospace(&self.last_summary); });
                ui.separator();
                ui.heading("Hardware diagnostics");
                ui.horizontal(|ui| {
                    if ui.button("Scan Lian Li HID").clicked() {
                        match scan_hid() {
                            Ok(items) => { self.status = format!("Found {} matching HID interface(s)", items.len()); self.hid = items; }
                            Err(e) => self.status = format!("Scan error: {e}"),
                        }
                    }
                    if ui.button("Export HID JSON").clicked() {
                        let path = config_path().with_file_name("hid-inventory.json");
                        let result = fs::create_dir_all(path.parent().unwrap())
                            .and_then(|_| fs::write(&path, serde_json::to_vec_pretty(&self.hid).unwrap_or_default()));
                        self.status = match result { Ok(()) => format!("Exported {}", path.display()), Err(e) => e.to_string() };
                    }
                    if ui.button("Check Lian Li processes").clicked() { self.process_report = read_lianli_processes(); }

                });
                for entry in &self.hid {
                    ui.label(format!("{} {} · VID:{} PID:{} · interface {} · usage page {:04X}", entry.manufacturer, entry.product, entry.vid, entry.pid, entry.interface, entry.usage_page));
                }
                ui.label("Native report is read-only. Review device identifiers before sharing.");
                ui.label("Matching process names (read-only):");
                ui.monospace(&self.process_report);
                ui.separator(); ui.label(&self.status);
            });
        });
    }
}
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([730.0, 720.0]), ..Default::default() };
    eframe::run_native("LianLite", options, Box::new(|_| Ok(Box::new(App::default()))))
}
