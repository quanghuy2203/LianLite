#![windows_subsystem = "windows"]
mod update;
mod native;
mod profile;
mod health;
mod diagnostics;
mod aura_descriptor;
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
enum UpdateMessage { Checked(Option<update::Available>), Staged(update::Staged) }
enum NativeMessage { Scanned(Vec<native::NativeDevice>), SystemScanned(String), AuraInspected(String), DescriptorInspected(Vec<aura_descriptor::DescriptorInfo>) }
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page { Overview, Devices, Diagnostics, Updates }
struct App {
    config: Config, hid: Vec<HidEntry>, process_report: String, status: String,
    update_rx: Option<Receiver<Result<UpdateMessage, String>>>,
    available: Option<update::Available>, staged: Option<update::Staged>,
    native_rx: Option<Receiver<Result<NativeMessage, String>>>,
    native_devices: Vec<native::NativeDevice>,
    native_windows_json: String,
    aura_pnp_json: String,
    global_color: [u8;3],
    global_brightness: u8,
    last_summary: String,
    page: Page,
    aura_descriptor: Vec<aura_descriptor::DescriptorInfo>,
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
            global_color: [100,150,255], global_brightness: 70, last_summary: String::new(),
            page: Page::Overview, aura_descriptor: Vec::new() }
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
                Ok(Ok(NativeMessage::DescriptorInspected(info))) => {
                    self.status = format!("ASUS HID descriptor read: {} interface(s). No hardware writes.", info.len());
                    self.aura_descriptor = info; self.native_rx = None;
                }
                Ok(Err(e)) => { self.status = format!("Native scan error: {e}"); self.native_rx = None; }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Native scan worker disconnected".into(); self.native_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(13, 17, 28);
        visuals.window_fill = egui::Color32::from_rgb(19, 24, 36);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(32, 40, 55);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(41, 59, 78);
        visuals.selection.bg_fill = egui::Color32::from_rgb(49, 123, 169);
        ctx.set_visuals(visuals);
        let cyan = egui::Color32::from_rgb(90, 213, 230);
        let muted = egui::Color32::from_rgb(147, 165, 186);
        let card = egui::Color32::from_rgb(24, 31, 46);
        let border = egui::Stroke::new(1.0, egui::Color32::from_rgb(47, 59, 76));

        egui::SidePanel::left("navigation")
            .resizable(false).exact_width(188.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(15, 22, 36)).inner_margin(18.0))
            .show(ctx, |ui| {
                ui.add_space(14.0);
                ui.label(egui::RichText::new("LIANLITE").size(23.0).strong().color(cyan));
                ui.label(egui::RichText::new("RUST NATIVE  /  v0.9").size(10.0).color(muted));
                ui.add_space(35.0);
                for (page, label) in [
                    (Page::Overview, "  Overview"),
                    (Page::Devices, "  Devices"),
                    (Page::Diagnostics, "  Diagnostics"),
                    (Page::Updates, "  Updates"),
                ] {
                    if ui.add_sized([156.0, 38.0],
                        egui::SelectableLabel::new(self.page == page, label)).clicked() {
                        self.page = page;
                    }
                    ui.add_space(9.0);
                }
                ui.add_space(22.0);
                ui.separator();
                ui.add_space(12.0);
                ui.label(egui::RichText::new("SAFE MODE").color(cyan).strong().size(11.0));
                ui.label(egui::RichText::new("Read-only hardware access").color(muted).size(11.0));
                ui.label(egui::RichText::new("No OpenRGB required").color(muted).size(11.0));
            });

        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(16, 23, 36)).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("STATUS").strong().color(cyan));
                    ui.label(egui::RichText::new(&self.status).color(muted));
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(22.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(match self.page {
                        Page::Overview => "Your lighting dashboard",
                        Page::Devices => "Connected hardware",
                        Page::Diagnostics => "Hardware diagnostics",
                        Page::Updates => "Application updates",
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("v0.9.0").color(cyan).strong());
                    });
                });
                ui.label(egui::RichText::new(
                    "Windows 11  /  Minimal resources  /  Hardware changes locked until verified"
                ).color(muted).size(12.0));
                ui.add_space(18.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.page {
                        Page::Overview => {
                            ui.columns(4, |columns| {
                                for (col, title, value, info) in [
                                    (0, "LCD FANS", "05", "3 + 2 groups"),
                                    (1, "TL FANS", "03", "one group"),
                                    (2, "STRIMERS", "02", "Wireless"),
                                    (3, "PROFILES", "03", "local previews"),
                                ] {
                                    egui::Frame::none().fill(card)
                                        .rounding(egui::Rounding::same(12))
                                        .stroke(border).inner_margin(14.0)
                                        .show(&mut columns[col], |ui| {
                                            ui.label(egui::RichText::new(title).size(10.0).color(muted));
                                            ui.label(egui::RichText::new(value).size(29.0).strong().color(cyan));
                                            ui.label(egui::RichText::new(info).size(11.0).color(muted));
                                        });
                                }
                            });
                            ui.add_space(18.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(17.0).show(ui, |ui| {
                                    ui.heading("Quick profiles");
                                    ui.label(egui::RichText::new("Preview only: these controls do not change fan speed or RGB hardware.").color(muted));
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        for preset in [profile::Preset::Silent, profile::Preset::Balanced, profile::Preset::Performance] {
                                            if ui.button(preset.name()).clicked() {
                                                profile::apply(&mut self.config, preset);
                                                self.status = format!("{} profile preview selected. No hardware writes.", preset.name());
                                            }
                                        }
                                    });
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Global preview color");
                                        let mut color = egui::Color32::from_rgb(
                                            self.global_color[0], self.global_color[1], self.global_color[2]);
                                        if ui.color_edit_button_srgba(&mut color).changed() {
                                            self.global_color = [color.r(), color.g(), color.b()];
                                        }
                                        if ui.button("Sync preview color").clicked() {
                                            profile::all_same_color(&mut self.config, self.global_color);
                                            self.status = "Local fan group RGB preview synchronized.".into();
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Slider::new(&mut self.global_brightness, 0..=100).text("Brightness"));
                                        if ui.button("Apply preview").clicked() {
                                            profile::all_brightness(&mut self.config, self.global_brightness);
                                            self.status = "Local brightness preview synchronized.".into();
                                        }
                                    });
                                });
                            ui.add_space(16.0);
                            ui.heading("Fan zones");
                            for group in &mut self.config.groups {
                                egui::Frame::none().fill(card).rounding(egui::Rounding::same(11))
                                    .stroke(border).inner_margin(15.0).show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&group.name).strong().size(15.0));
                                            ui.label(egui::RichText::new(format!("{} fans", group.count)).color(muted));
                                        });
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Slider::new(&mut group.speed_percent, 20..=100).text("Speed preview %"));
                                            ui.add(egui::Slider::new(&mut group.brightness, 0..=100).text("RGB brightness"));
                                            let mut color = egui::Color32::from_rgb(group.rgb[0], group.rgb[1], group.rgb[2]);
                                            if ui.color_edit_button_srgba(&mut color).changed() {
                                                group.rgb = [color.r(), color.g(), color.b()];
                                            }
                                        });
                                    });
                                ui.add_space(8.0);
                            }
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(11))
                                .stroke(border).inner_margin(15.0).show(ui, |ui| {
                                    ui.heading("Strimer Wireless");
                                    for (index, brightness) in self.config.strimer_brightness.iter_mut().enumerate() {
                                        ui.add(egui::Slider::new(brightness, 0..=100)
                                            .text(format!("Strimer {} brightness preview", index + 1)));
                                    }
                                    if ui.button("Save local profile").clicked() {
                                        self.status = match save(&self.config) {
                                            Ok(()) => "Local profile saved".into(), Err(e) => e,
                                        };
                                    }
                                });
                        }
                        Page::Devices => {
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(17.0).show(ui, |ui| {
                                    ui.heading("Native device discovery");
                                    ui.label(egui::RichText::new("These buttons only inspect device IDs. No RGB control commands are sent.").color(muted));
                                    ui.add_space(9.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Detect ASUS + Lian Li HID")).clicked() {
                                            let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                            std::thread::spawn(move || { let _ = tx.send(native::scan_hid().map(NativeMessage::Scanned)); });
                                        }
                                        if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Detect GPU + RAM")).clicked() {
                                            let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                            std::thread::spawn(move || { let _ = tx.send(native::scan_gpu_and_memory().map(NativeMessage::SystemScanned)); });
                                        }
                                    });
                                    if self.native_rx.is_some() { ui.spinner(); }
                                    for d in &self.native_devices {
                                        ui.label(format!("{}  |  {}  |  {}  |  IF {}", d.kind, d.name, d.id, d.interface));
                                    }
                                    if self.native_devices.is_empty() { ui.label(egui::RichText::new("Scan to list USB HID controllers").color(muted)); }
                                    ui.collapsing("GPU and RAM raw inventory", |ui| { ui.monospace(&self.native_windows_json); });
                                });
                            ui.add_space(14.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(17.0).show(ui, |ui| {
                                    ui.heading("ASUS ROG Strix B650E-F");
                                    ui.label("AURA 0B05:19AF  |  MI_00 WinUSB candidate  |  MI_02 HID FF72:00A1");
                                    ui.colored_label(egui::Color32::from_rgb(246, 194, 107),
                                        "Protocol NOT verified. All writes disabled.");
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Inspect ASUS PnP")).clicked() {
                                            let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                            std::thread::spawn(move || { let _ = tx.send(native::inspect_aura_pnp().map(NativeMessage::AuraInspected)); });
                                        }
                                        if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Read HID descriptor")).clicked() {
                                            let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                            std::thread::spawn(move || { let _ = tx.send(aura_descriptor::inspect().map(NativeMessage::DescriptorInspected)); });
                                        }
                                    });
                                    for desc in &self.aura_descriptor {
                                        ui.label(format!("HID interface {}: {} descriptor bytes", desc.interface, desc.length));
                                    }
                                    ui.collapsing("ASUS PnP details", |ui| { ui.monospace(&self.aura_pnp_json); });
                                });
                        }
                        Page::Diagnostics => {
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("Export diagnostic reports");
                                    ui.label(egui::RichText::new("For the summary, scan Devices first. Raw PnP reports can include device identifiers.").color(muted));
                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Export safe summary").clicked() {
                                            match health::inspect() {
                                                Ok(h) => {
                                                    let value = diagnostics::summary(&self.native_devices, &self.native_windows_json,
                                                        &self.aura_pnp_json, &h);
                                                    self.last_summary = serde_json::to_string_pretty(&value).unwrap_or_default();
                                                    self.status = match diagnostics::export(&value) {
                                                        Ok(path) => format!("Summary saved: {}",path.display()),
                                                        Err(e) => format!("Export error: {e}"),
                                                    };
                                                }
                                                Err(e) => self.status = e,
                                            }
                                        }
                                        if ui.add_enabled(!self.aura_descriptor.is_empty(), egui::Button::new("Export HID descriptor")).clicked() {
                                            self.status = match aura_descriptor::export(&self.aura_descriptor) {
                                                Ok(path) => format!("ASUS descriptor saved: {}",path.display()),
                                                Err(e) => e,
                                            };
                                        }
                                        if ui.add_enabled(!self.aura_pnp_json.is_empty(), egui::Button::new("Export ASUS PnP")).clicked() {
                                            self.status = match native::export_aura_report(&self.aura_pnp_json) {
                                                Ok(path) => format!("ASUS PnP report saved: {}",path.display()),
                                                Err(e) => e,
                                            };
                                        }
                                        if ui.button("Open reports folder").clicked() {
                                            let root = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                                            let dir = root.join("LianLite");
                                            self.status = match fs::create_dir_all(&dir)
                                                .and_then(|_| Command::new("explorer.exe").arg(&dir).spawn().map(|_| ())) {
                                                Ok(()) => "Reports folder opened".into(),
                                                Err(e) => format!("Cannot open reports: {e}"),
                                            };
                                        }
                                    });
                                    ui.collapsing("Shareable summary preview", |ui| { ui.monospace(&self.last_summary); });
                                });
                            ui.add_space(14.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("Lian Li process health");
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Check Lian Li services").clicked() {
                                            self.process_report = read_lianli_processes();
                                        }
                                        if ui.button("Scan Lian Li HID").clicked() {
                                            match scan_hid() {
                                                Ok(devices) => { self.status = format!("{} Lian Li HID entries", devices.len()); self.hid = devices; }
                                                Err(e) => self.status = e,
                                            }
                                        }
                                    });
                                    ui.collapsing("Lian Li process details", |ui| { ui.monospace(&self.process_report); });
                                });
                        }
                        Page::Updates => {
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("GitHub Releases updater");
                                    ui.label(egui::RichText::new("quanghuy2203/LianLite").color(cyan));
                                    ui.add_space(9.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.add_enabled(self.update_rx.is_none(), egui::Button::new("Check for updates")).clicked() {
                                            let (tx,rx) = mpsc::channel(); self.update_rx = Some(rx);
                                            std::thread::spawn(move || { let _ = tx.send(update::check().map(UpdateMessage::Checked)); });
                                        }
                                        if let Some(version) = self.available.clone() {
                                            if ui.add_enabled(self.update_rx.is_none(), egui::Button::new(format!("Download v{}",version.version))).clicked() {
                                                let (tx,rx) = mpsc::channel(); self.update_rx = Some(rx);
                                                std::thread::spawn(move || { let _ = tx.send(update::stage(&version).map(UpdateMessage::Staged)); });
                                            }
                                        }
                                        if let Some(ref staged) = self.staged {
                                            if ui.button("Install & restart").clicked() {
                                                match update::launch_helper(staged) {
                                                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                                                    Err(e) => self.status = format!("Cannot install: {e}"),
                                                }
                                            }
                                        }
                                    });
                                    if self.update_rx.is_some() { ui.spinner(); }
                                    ui.label(egui::RichText::new("The update replaces both application and updater. Old backups are kept for recovery.").color(muted));
                                });
                            ui.add_space(14.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("Installation health");
                                    if ui.button("Check updater and backups").clicked() {
                                        self.status = match health::inspect() {
                                            Ok(h) => format!("Updater: {} | Backup: {} | Previous: {} | Pending: {}",
                                                h.updater_present, h.backup_present, h.old_backup_present, h.pending_present),
                                            Err(e) => e,
                                        };
                                    }
                                    let path = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
                                        .unwrap_or_else(std::env::temp_dir).join("LianLite");
                                    if let Ok(text) = fs::read_to_string(path.join("updater-last-status.txt")) {
                                        ui.collapsing("Last update log", |ui| { ui.monospace(text); });
                                    }
                                    if let Ok(text) = fs::read_to_string(path.join("updater-last-error.txt")) {
                                        ui.collapsing("Last update error", |ui| { ui.monospace(text); });
                                    }
                                });
                        }
                    }
                });
            });
    }
}
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1060.0, 740.0]), ..Default::default() };
    eframe::run_native("LianLite", options, Box::new(|_| Ok(Box::new(App::default()))))
}
