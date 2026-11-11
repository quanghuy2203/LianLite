#![windows_subsystem = "windows"]
mod update;
mod native;
mod profile;
mod health;
mod diagnostics;
mod aura_descriptor;
mod discovery;
mod wireless_research;
mod snapshot;
mod backups;
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
            groups: vec![],
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
enum NativeMessage { Scanned(Vec<native::NativeDevice>), SystemScanned(String), AuraInspected(String), DescriptorInspected(Vec<aura_descriptor::DescriptorInfo>), WirelessDiscovered(discovery::DiscoveryReport), SettingsInspected(wireless_research::ResearchReport), SurveyDone(discovery::DiscoveryReport, wireless_research::ResearchReport),
    PcScanned { hid: Result<Vec<native::NativeDevice>, String>,
        system: Result<String, String>, aura: Result<String, String> } }
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
    wireless_report: Option<discovery::DiscoveryReport>,
    wireless_settings: Option<wireless_research::ResearchReport>,
    confirm_backup_cleanup: bool,
    confirm_preview_reset: bool,
}
impl Default for App {
    fn default() -> Self {
        let mut config: Config = fs::read(config_path()).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        profile::validate(&mut config);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(Ok(NativeMessage::SurveyDone(discovery::scan(), wireless_research::inspect()))); });
        Self { config, hid: Vec::new(), process_report: "Not checked yet".into(),
            status: "Read-only hardware mode. No USB commands will be sent.".into(),
            update_rx: None, available: None, staged: None,
            native_rx: Some(rx), native_devices: vec![], native_windows_json: String::new(), aura_pnp_json: String::new(),
            global_color: [100,150,255], global_brightness: 70, last_summary: String::new(),
            page: Page::Overview, aura_descriptor: Vec::new(), wireless_report: None, wireless_settings: None, confirm_backup_cleanup: false, confirm_preview_reset: false }
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
                Ok(Ok(NativeMessage::PcScanned { hid, system, aura })) => {
                    let mut outcomes = Vec::new();
                    match hid {
                        Ok(found) => { outcomes.push(format!("HID: {} entries", found.len())); self.native_devices = found; }
                        Err(e) => outcomes.push(format!("HID error: {e}")),
                    }
                    match system {
                        Ok(data) => { outcomes.push("GPU/RAM: read".into()); self.native_windows_json = data; }
                        Err(e) => outcomes.push(format!("GPU/RAM error: {e}")),
                    }
                    match aura {
                        Ok(data) => { outcomes.push("ASUS PnP: read".into()); self.aura_pnp_json = data; }
                        Err(e) => outcomes.push(format!("ASUS PnP error: {e}")),
                    }
                    self.status = outcomes.join(" | ");
                    self.native_rx = None;
                }
                Ok(Ok(NativeMessage::SurveyDone(discovered, schema))) => {
                    self.status = format!(
                        "Offline survey: {} adapter types, {} saved records, {} structural fields; live wireless topology unverified.",
                        discovered.dongles.len(), schema.typed_records, schema.fields.len()
                    );
                    self.wireless_report = Some(discovered);
                    self.wireless_settings = Some(schema);
                    self.native_rx = None;
                }
                Ok(Ok(NativeMessage::SettingsInspected(report))) => {
                    self.status = format!("L-Connect saved settings: {} records, {} distinct type/shape entries. No live RF query.",
                        report.typed_records, report.record_types.len());
                    self.wireless_settings = Some(report);
                    self.native_rx = None;
                }
                Ok(Ok(NativeMessage::WirelessDiscovered(report))) => {
                    self.status = format!("Wireless discovery: {} adapter type(s), {} saved fan settings. Live RF groups are not yet queried.",
                        report.dongles.len(), report.groups.len());
                    self.wireless_report = Some(report);
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
        let cyan = egui::Color32::from_rgb(90, 213, 230);
        let muted = egui::Color32::from_rgb(147, 165, 186);
        let card = egui::Color32::from_rgb(24, 31, 46);
        let border = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(47, 59, 76));

        egui::SidePanel::left("navigation")
            .resizable(false).exact_width(188.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(15, 22, 36)).inner_margin(18.0))
            .show(ctx, |ui| {
                ui.add_space(14.0);
                ui.label(egui::RichText::new("LIANLITE").size(23.0).strong().color(cyan));
                ui.label(egui::RichText::new("RUST NATIVE  /  v0.12").size(10.0).color(muted));
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
                        ui.label(egui::RichText::new("v0.12.0").color(cyan).strong());
                    });
                });
                ui.label(egui::RichText::new(
                    "Windows 11  /  Minimal resources  /  Hardware changes locked until verified"
                ).color(muted).size(12.0));
                ui.add_space(18.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.page {
                        Page::Overview => {
                            let dongles = self.wireless_report.as_ref().map_or(0, |r| r.dongles.len());
                            let lcd_receivers = self.wireless_report.as_ref()
                                .and_then(|r| r.lcd_usb_receivers)
                                .map_or("?".into(), |n| n.to_string());
                            let saved_categories = self.wireless_settings.as_ref()
                                .map_or(0, |r| r.record_types.len());
                            ui.columns(4, |columns| {
                                for (col, title, value, info) in [
                                    (0, "WIRELESS ADAPTER TYPES", dongles.to_string(), "Windows PnP"),
                                    (1, "LCD USB RECEIVERS", lcd_receivers, "Windows USB 1CBE:0006"),
                                    (2, "SAVED CONFIG TYPES", saved_categories.to_string(), "offline L-Connect"),
                                    (3, "LIVE RF GROUPS", "?".to_string(), "not yet queried"),
                                ] {
                                    egui::Frame::none().fill(card)
                                        .rounding(egui::Rounding::same(12.0_f32))
                                        .stroke(border).inner_margin(14.0)
                                        .show(&mut columns[col], |ui| {
                                            ui.label(egui::RichText::new(title).size(10.0).color(muted));
                                            ui.label(egui::RichText::new(value).size(29.0).strong().color(cyan));
                                            ui.label(egui::RichText::new(info).size(11.0).color(muted));
                                        });
                                }
                            });
                            ui.add_space(12.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                .stroke(border).inner_margin(14.0).show(ui, |ui| {
                                    ui.heading("Discovery from your PC");
                                    ui.label(egui::RichText::new("Adapter IDs are detected from Windows. FanQuantity values may come from old L-Connect settings, NOT live wireless groups.").color(muted));
                                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Refresh offline discovery")).clicked() {
                                        let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                        std::thread::spawn(move || { let _ = tx.send(Ok(NativeMessage::SurveyDone(discovery::scan(), wireless_research::inspect()))); });
                                    }
                                    if self.native_rx.is_some() { ui.spinner(); }
                                    if let Some(report) = &self.wireless_report {
                                        for adapter in &report.dongles { ui.label(format!("Adapter detected: {}",adapter.model)); }
                                        ui.label(match report.lcd_usb_receivers {
                                            Some(n) => format!("Actual LCD USB receiver roots detected: {n} (not RF groups)"),
                                            None => "LCD USB receiver inventory unavailable".into(),
                                        });
                                        for group in &report.groups {
                                            ui.label(format!("Saved setting: {}  |  {} fan(s)  |  not live",
                                                group.name, group.fan_count.unwrap_or(0)));
                                        }
                                        if report.dongles.is_empty() && report.groups.is_empty() {
                                            ui.colored_label(egui::Color32::from_rgb(246, 194, 107),
                                                "No adapter or saved fan-group data found. This does NOT mean devices are disconnected.");
                                        }
                                    }
                                });
                            if let Some(schema) = &self.wireless_settings {
                                let candidates: Vec<_> = schema.record_types.iter().filter(|item|
                                    ["Fan", "GroupLCD", "SingleLCD", "Strimer"].contains(&item.setting_type.as_str())
                                ).collect();
                                ui.add_space(8.0);
                                if !candidates.is_empty() {
                                    egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                        .stroke(border).inner_margin(14.0).show(ui, |ui| {
                                            ui.heading("Saved L-Connect categories (not live groups)");
                                            ui.horizontal_wrapped(|ui| {
                                                for item in candidates {
                                                    ui.label(format!("{}: {} saved record(s)", item.setting_type, item.records));
                                                }
                                            });
                                            ui.label(egui::RichText::new("Live 2/3/3 wireless fan topology still requires verified controller discovery.").color(muted));
                                        });
                                }
                            }
                            ui.add_space(18.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                .stroke(border).inner_margin(17.0).show(ui, |ui| {
                                    ui.heading("Local preview only — not connected hardware");
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
                            ui.heading("Saved preview zones (manually configured)");
                            ui.label(egui::RichText::new("These are local user presets and are NOT a list of devices detected by L-Wireless.").color(muted));
                            if ui.button("Add manual preview zone").clicked() {
                                self.config.groups.push(Group { name: format!("Preview zone {}", self.config.groups.len()+1), count: 1, speed_percent: 65, brightness: 70, rgb: self.global_color });
                            }
                            if !self.config.groups.is_empty() {
                                if !self.confirm_preview_reset {
                                    if ui.button("Clear manual preview zones...").clicked() {
                                        self.confirm_preview_reset = true;
                                    }
                                } else {
                                    ui.colored_label(egui::Color32::from_rgb(246, 194, 107),
                                        "This clears only saved manual preview zones. It does not touch L-Connect or any hardware.");
                                    ui.horizontal(|ui| {
                                        if ui.button("Confirm clear preview zones").clicked() {
                                            self.config.groups.clear();
                                            self.confirm_preview_reset = false;
                                            self.status = match save(&self.config) {
                                                Ok(()) => "Cleared saved manual preview zones. Hardware unchanged.".into(),
                                                Err(e) => format!("Zones cleared locally, but saving failed: {e}"),
                                            };
                                        }
                                        if ui.button("Cancel").clicked() {
                                            self.confirm_preview_reset = false;
                                        }
                                    });
                                }
                            }
                            for group in &mut self.config.groups {
                                egui::Frame::none().fill(card).rounding(egui::Rounding::same(11.0_f32))
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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(11.0_f32))
                                .stroke(border).inner_margin(15.0).show(ui, |ui| {
                                    ui.heading("Strimer preview slots (not detected)");
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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                .stroke(border).inner_margin(17.0).show(ui, |ui| {
                                    ui.heading("Native device discovery");
                                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Scan PC hardware in one click")).clicked() {
                                        let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                        std::thread::spawn(move || {
                                            let hid = native::scan_hid();
                                            let system = native::scan_gpu_and_memory();
                                            let aura = native::inspect_aura_pnp();
                                            let _ = tx.send(Ok(NativeMessage::PcScanned { hid, system, aura }));
                                        });
                                    }
                                    ui.label(egui::RichText::new("Live wireless group discovery is unavailable while L-Connect owns the WinUSB TX/RX devices. No services are stopped.").color(muted));
                                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Refresh offline discovery")).clicked() {
                                        let (tx,rx)=mpsc::channel(); self.native_rx=Some(rx);
                                        std::thread::spawn(move || { let _=tx.send(Ok(NativeMessage::SurveyDone(discovery::scan(), wireless_research::inspect()))); });
                                    }
                                    if let Some(report) = &self.wireless_report {
                                        for adapter in &report.dongles { ui.label(format!("Detected PnP adapter: {}",adapter.model)); }
                                        for group in &report.groups {
                                            ui.label(format!("Saved fan quantity: {} => {} (NOT live wireless)",
                                                group.name, group.fan_count.unwrap_or(0)));
                                        }
                                        for warning in &report.warnings { ui.small(warning); }
                                    }

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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("Export diagnostic reports");
                                    if ui.button("Export combined support snapshot (recommended)").clicked() {
                                        self.status = match health::inspect() {
                                            Ok(h) => {
                                                let report = snapshot::compose(
                                                    &self.native_devices, &self.native_windows_json, &self.aura_pnp_json,
                                                    &h, self.wireless_report.as_ref(), self.wireless_settings.as_ref()
                                                );
                                                self.last_summary = serde_json::to_string_pretty(&report).unwrap_or_default();
                                                match snapshot::export(&report) {
                                                    Ok(path) => format!("Combined support snapshot saved: {}", path.display()),
                                                    Err(e) => format!("Snapshot export error: {e}"),
                                                }
                                            }
                                            Err(e) => format!("Unable to inspect installation: {e}"),
                                        };
                                    }
                                    ui.label(egui::RichText::new(
                                        "One support file with hardware flags and saved wireless structure. Scan PC hardware first to fill GPU/RAM/ASUS fields."
                                    ).color(muted));
                                    ui.add_space(7.0);
                                    ui.heading("L-Connect saved-settings structure (offline)");
                                    ui.label(egui::RichText::new("Includes nested JSON key shapes and saved array lengths. Never represents live RF fan counts.").color(muted));
                                    if ui.add_enabled(self.native_rx.is_none(), egui::Button::new("Analyze saved setting types")).clicked() {
                                        let (tx,rx) = mpsc::channel(); self.native_rx = Some(rx);
                                        std::thread::spawn(move || { let _ = tx.send(Ok(NativeMessage::SettingsInspected(wireless_research::inspect()))); });
                                    }
                                    if let Some(report) = &self.wireless_settings {
                                        ui.label(format!("{} saved records, {} categories, {} sanitized structural fields",
                                            report.typed_records, report.record_types.len(), report.fields.len()));
                                        for record in &report.record_types {
                                            ui.label(format!("{}  ·  {}  ·  {} record(s){}",
                                                record.setting_type, record.value_shape, record.records,
                                                if record.wireless_candidate { "  ·  wireless candidate (unverified)" } else { "" }));
                                        }
                                        ui.collapsing("Inspect saved field structure (keys only)", |ui| {
                                            for row in &report.fields {
                                                let cardinality = match (row.stored_array_min, row.stored_array_max) {
                                                    (Some(min), Some(max)) if min == max => format!(" | saved array length {min}"),
                                                    (Some(min), Some(max)) => format!(" | saved array lengths {min}–{max}"),
                                                    _ => String::new(),
                                                };
                                                ui.small(format!("{} / {} : {} × {}{}",
                                                    row.setting_type, row.field_path, row.value_shape,
                                                    row.occurrences, cardinality));
                                            }
                                            if report.field_rows_truncated {
                                                ui.label("Field list capped for privacy and performance.");
                                            }
                                        });
                                        if ui.button("Export v0.12 nested schema (no IDs)").clicked() {
                                            self.status = match wireless_research::export(report) {
                                                Ok(path) => format!("Saved offline schema to {}",path.display()),
                                                Err(e) => format!("Schema export error: {e}"),
                                            };
                                        }
                                    }
                                    if let (Some(wireless), Some(schema)) = (&self.wireless_report, &self.wireless_settings) {
                                        if ui.button("Export both offline discovery reports").clicked() {
                                            self.status = match (discovery::export(wireless), wireless_research::export(schema)) {
                                                (Ok(a), Ok(b)) => format!("Saved wireless reports: {} and {}", a.display(), b.display()),
                                                (Err(e), _) | (_, Err(e)) => format!("Some report exports failed: {e}"),
                                            };
                                        }
                                    }
                                    ui.separator();
                                    if let Some(report) = &self.wireless_report {
                                        if ui.button("Export read-only wireless inventory").clicked() {
                                            self.status = match discovery::export(report) {
                                                Ok(path) => format!("Inventory saved to {}",path.display()),
                                                Err(e) => format!("Export failed: {e}"),
                                            };
                                        }
                                    }
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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
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
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
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
                                    ui.label(egui::RichText::new("Keeps the newest application rollback and updater rollback. Older numbered backups are cleaned after a successful update.").color(muted));
                                });
                            ui.add_space(14.0);
                            egui::Frame::none().fill(card).rounding(egui::Rounding::same(12.0_f32))
                                .stroke(border).inner_margin(18.0).show(ui, |ui| {
                                    ui.heading("Installation health");
                                    if ui.button("Check updater and backups").clicked() {
                                        self.status = match health::inspect() {
                                            Ok(h) => format!("Updater: {} | Backup: {} | Previous: {} | Pending: {}",
                                                h.updater_present, h.backup_present, h.old_backup_present, h.pending_present),
                                            Err(e) => e,
                                        };
                                    }
                                    ui.add_space(8.0);
                                    if !self.confirm_backup_cleanup {
                                        if ui.button("Clean old update backups...").clicked() {
                                            self.confirm_backup_cleanup = true;
                                        }
                                    } else {
                                        ui.colored_label(egui::Color32::from_rgb(246, 194, 107),
                                            "Keeps lianlite.exe.backup and the newest updater backup. Deletes only older numbered archives.");
                                        ui.horizontal(|ui| {
                                            if ui.button("Confirm cleanup").clicked() {
                                                self.confirm_backup_cleanup = false;
                                                self.status = match std::env::current_exe()
                                                    .map_err(|e| e.to_string())
                                                    .and_then(|exe| {
                                                        let dir = exe.parent().ok_or("Missing install directory")?;
                                                        backups::cleanup(dir)
                                                    }) {
                                                        Ok(result) => format!(
                                                            "Backup cleanup: {} old files deleted, {} rollbacks retained. {}",
                                                            result.deleted, result.retained,
                                                            if result.failed.is_empty() { "No errors.".into() }
                                                            else { result.failed.join(" | ") }
                                                        ),
                                                        Err(e) => format!("Backup cleanup error: {e}"),
                                                    };
                                            }
                                            if ui.button("Cancel").clicked() { self.confirm_backup_cleanup = false; }
                                        });
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
    eframe::run_native("LianLite", options, Box::new(|cc| {
        // Set the palette once. Re-applying Visuals every frame causes avoidable
        // style mutations and repaints on a lightweight monitoring UI.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(13, 17, 28);
        visuals.window_fill = egui::Color32::from_rgb(19, 24, 36);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(32, 40, 55);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(41, 59, 78);
        visuals.selection.bg_fill = egui::Color32::from_rgb(49, 123, 169);
        cc.egui_ctx.set_visuals(visuals);
        Ok(Box::new(App::default()))
    }))
}
