//! Offline research on L-Connect's saved JSON/gzip setting schema.
//! Reads bounded local files without opening the TX/RX USB devices.
//! Does not export values, file paths, serials, or DeviceID.
use flate2::read::GzDecoder;
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::Read, path::{Path,PathBuf}};

const MAX_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_DECODED: u64 = 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct RecordType {
    pub setting_type: String,
    pub records: usize,
    pub value_shape: &'static str,
    pub wireless_candidate: bool,
}
#[derive(Clone, Serialize)]
pub struct ResearchReport {
    pub examined_files: usize,
    pub decoded_json: usize,
    pub typed_records: usize,
    pub record_types: Vec<RecordType>,
    pub binding_groups_verified: bool,
    pub safety_note: &'static str,
    pub hints: Vec<String>,
}
fn safe_setting_type(name: &str) -> String {
    // Type strings are usually fixed enum names, but refuse identifiers,
    // paths and any unusual content that could identify the user.
    if name.is_empty() || name.len() > 40
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    { return "[redacted]".into(); }
    name.to_string()
}
fn shape(data: &Value) -> &'static str {
    match data {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
fn wireless_candidate(typ: &str) -> bool {
    let name = typ.to_ascii_lowercase();
    // Heuristic only: these words do not establish a live wireless binding.
    ["wireless", "strimer", "tlw", "rf", "bond", "bind", "channel"]
        .iter().any(|keyword| name.contains(keyword))
}
fn walk(root: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 || files.len() >= MAX_FILES {return;}
    let Ok(entries) = fs::read_dir(root) else {return;};
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {break;}
        let Ok(ty) = entry.file_type() else {continue;};
        if ty.is_symlink() {continue;}
        if ty.is_dir() {walk(&entry.path(),files,depth+1);}
        else if ty.is_file() {files.push(entry.path());}
    }
}
fn json_from_setting(file: &Path) -> Option<Value> {
    let meta = fs::metadata(file).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES || meta.len() < 2 {return None;}
    let raw = fs::read(file).ok()?;
    let bytes = if raw.starts_with(&[0x1f,0x8b]) {
        let mut data = Vec::new();
        GzDecoder::new(raw.as_slice()).take(MAX_DECODED+1).read_to_end(&mut data).ok()?;
        if data.len() as u64 > MAX_DECODED {return None;}
        data
    } else if raw.starts_with(b"{") {raw}
    else {return None;};
    serde_json::from_slice(&bytes).ok()
}
pub fn inspect() -> ResearchReport {
    let root = std::env::var_os("PROGRAMDATA").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Lian-Li").join("L-Connect 3").join("device");
    let mut files=Vec::new();
    walk(&root,&mut files,0);
    let total=files.len();
    let mut json_count=0;
    let mut typed=0;
    let mut types:BTreeMap<(String,&'static str),usize>=BTreeMap::new();
    for file in files {
        let Some(value)=json_from_setting(&file) else {continue;};
        json_count+=1;
        let Some(typ)=value.get("Type").and_then(Value::as_str) else {continue;};
        let Some(data)=value.get("Data") else {continue;};
        typed+=1;
        let name=safe_setting_type(typ);
        *types.entry((name,shape(data))).or_default()+=1;
    }
    let entries=types.into_iter().map(|((setting_type,value_shape),records)| {
        let wireless_candidate=wireless_candidate(&setting_type);
        RecordType {setting_type,records,value_shape,wireless_candidate}
    }).collect();
    ResearchReport {
        examined_files: total,
        decoded_json: json_count,
        typed_records: typed,
        record_types: entries,
        binding_groups_verified:false,
        safety_note:"Offline schema research only; no live RF query, no raw settings or device IDs exported.",
        hints:vec![
            "Wireless-related setting types are hints, not discovered channels.".into(),
            "TX/RX WinUSB requires exclusive access; LianLite does not interrupt L-Connect.".into(),
            "The correct live group count remains unknown until an independently verified read-only interface exists.".into(),
        ],
    }
}
pub fn export(report:&ResearchReport)->Result<PathBuf,String>{
    let root=std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(||PathBuf::from("."));
    let dir=root.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let path=dir.join("wireless-settings-schema-v011.json");
    let bytes=serde_json::to_vec_pretty(report).map_err(|e|e.to_string())?;
    fs::write(&path,bytes).map_err(|e|e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn types_redact_identifiers() {
        assert_eq!(safe_setting_type("FanQuantity"),"FanQuantity");
        assert_eq!(safe_setting_type("ab:cd:ef:00"),"[redacted]");
        assert_eq!(safe_setting_type("C:\\Users\\Alice"),"[redacted]");
        assert_eq!(safe_setting_type("StrimerWireless"),"StrimerWireless");
    }
    #[test] fn shape_does_not_expose_values() {
        assert_eq!(shape(&serde_json::json!({"id":"private"})),"object");
        assert_eq!(shape(&serde_json::json!(["private"])),"array");
        assert!(!wireless_candidate("FanQuantity"));
        assert!(wireless_candidate("WirelessSettings"));
    }
}
