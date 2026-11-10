mod update;
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
struct App {
    config: Config, hid: Vec<HidEntry>, process_report: String, status: String,
    update_rx: Option<Receiver<Result<UpdateMessage, String>>>,
    available: Option<update::Available>, staged: Option<PathBuf>,
}
impl Default for App {
    fn default() -> Self {
        let config = fs::read(config_path()).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Self { config, hid: Vec::new(), process_report: "Not checked yet".into(),
            status: "Read-only hardware mode. No USB commands will be sent.".into(),
            update_rx: None, available: None, staged: None }
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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LianLite 0.4  ·  Ultra Minimal");
            ui.label("Windows 11  |  Read-only diagnostics + local profile editor");
            ui.colored_label(egui::Color32::YELLOW, "Hardware control is NOT enabled. Keep L-Connect services running.");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Software updates · GitHub Releases");
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
