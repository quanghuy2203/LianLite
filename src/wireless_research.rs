//! Bounded offline schema inventory of L-Connect saved configuration.
//! This does not discover live RF groups. Only explicitly allowlisted key NAMES
//! and value shapes are exported, never DeviceID, paths, values or serials.
use flate2::read::GzDecoder;
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::Read, path::{Path, PathBuf}};

const MAX_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_DECODED_BYTES: u64 = 1024 * 1024;
const MAX_DEPTH: usize = 4;
const MAX_OBJECT_KEYS: usize = 48;
const MAX_ARRAY_SAMPLE: usize = 12;
const MAX_FIELD_ROWS: usize = 240;

#[derive(Clone, Serialize)]
pub struct RecordType {
    pub setting_type: String,
    pub records: usize,
    pub value_shape: &'static str,
    pub wireless_candidate: bool,
}

#[derive(Clone, Serialize)]
pub struct FieldShape {
    pub setting_type: String,
    pub field_path: String,
    pub value_shape: &'static str,
    pub occurrences: usize,
    // Array cardinality is OFFLINE STORED DATA only: not a live fan or channel count.
    pub stored_array_min: Option<usize>,
    pub stored_array_max: Option<usize>,
}

#[derive(Clone, Serialize)]
pub struct ResearchReport {
    pub examined_files: usize,
    pub decoded_json: usize,
    pub typed_records: usize,
    pub record_types: Vec<RecordType>,
    pub fields: Vec<FieldShape>,
    pub field_rows_truncated: bool,
    pub binding_groups_verified: bool,
    pub safety_note: &'static str,
    pub hints: Vec<String>,
}

fn safe_setting_type(name: &str) -> String {
    if name.is_empty() || name.len() > 40 ||
        !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        "[redacted]".into()
    } else { name.into() }
}
fn shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
fn wireless_candidate(typ: &str) -> bool {
    let lower = typ.to_ascii_lowercase();
    ["wireless", "strimer", "tlw", "rf", "bind", "bond", "channel"]
        .iter().any(|key| lower.contains(key))
}

// Only literal, expected field names can appear in exported reports.
// Any unknown/custom field name becomes an anonymous [other] bucket.
fn safe_field_name(field: &str) -> &'static str {
    match field.to_ascii_lowercase().as_str() {
        "groups" => "groups", "group" => "group",
        "fans" => "fans", "fan" => "fan",
        "fancount" => "fanCount", "fanquantity" => "fanQuantity",
        "fannum" => "fanNum", "quantity" => "quantity",
        "lcd" => "lcd", "lcds" => "lcds", "lcdcount" => "lcdCount",
        "screens" => "screens", "screen" => "screen",
        "strimer" => "strimer", "strimers" => "strimers",
        "port" => "port", "ports" => "ports",
        "channel" => "channel", "channels" => "channels",
        "device" => "device", "devices" => "devices",
        "devicetype" => "deviceType", "type" => "type",
        "data" => "data", "items" => "items",
        "count" => "count", "length" => "length",
        "leds" => "leds", "segments" => "segments",
        "mode" => "mode", "effect" => "effect", "effects" => "effects",
        "lighting" => "lighting", "profile" => "profile",
        "speed" => "speed", "brightness" => "brightness",
        "color" => "color", "colors" => "colors", "rgb" => "rgb",
        "merge" => "merge", "direction" => "direction",
        "index" => "index", "order" => "order",
        "islcd" => "isLCD", "size" => "size",
        "width" => "width", "height" => "height",
        "sync" => "sync", "name" => "name", "id" => "id",
        "rotation" => "rotation", "layout" => "layout",
        "enable" => "enable", "enabled" => "enabled",
        "value" => "value", "values" => "values",
        "setting" => "setting", "settings" => "settings",
        "status" => "status", "connected" => "connected",
        "version" => "version", "model" => "model",
        _ => "[other]",
    }
}

#[derive(Default)]
struct FieldCount {
    occurrences: usize,
    min: Option<usize>,
    max: Option<usize>,
}
type FieldMap = BTreeMap<(String, String, &'static str), FieldCount>;

fn describe_data(
    setting_type: &str,
    path: &str,
    value: &Value,
    depth: usize,
    fields: &mut FieldMap,
) {
    if depth > MAX_DEPTH { return; }
    let key = (setting_type.to_string(), path.to_string(), shape(value));
    let stats = fields.entry(key).or_default();
    stats.occurrences += 1;
    if let Value::Array(items) = value {
        stats.min = Some(stats.min.map_or(items.len(), |old| old.min(items.len())));
        stats.max = Some(stats.max.map_or(items.len(), |old| old.max(items.len())));
    }
    match value {
        Value::Object(object) => {
            // Sorted object keys yield deterministic output independent of JSON field order.
            for (name, child) in object.iter().take(MAX_OBJECT_KEYS) {
                let sanitized = safe_field_name(name);
                let next = format!("{path}.{sanitized}");
                describe_data(setting_type, &next, child, depth+1, fields);
            }
        }
        Value::Array(elements) => {
            // Sample only 12 entries; never serialize any individual element values.
            for child in elements.iter().take(MAX_ARRAY_SAMPLE) {
                describe_data(setting_type, &format!("{path}[]"), child, depth+1, fields);
            }
        }
        _ => {}
    }
}
fn walk(root: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 || files.len() >= MAX_FILES { return; }
    let Ok(dir) = fs::read_dir(root) else { return; };
    for item in dir.flatten() {
        if files.len() >= MAX_FILES { break; }
        let Ok(kind) = item.file_type() else { continue; };
        if kind.is_symlink() { continue; }
        if kind.is_dir() { walk(&item.path(), files, depth+1); }
        else if kind.is_file() { files.push(item.path()); }
    }
}
fn read_setting(path: &Path) -> Option<Value> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() < 2 || meta.len() > MAX_FILE_BYTES { return None; }
    let raw = fs::read(path).ok()?;
    let decoded = if raw.starts_with(&[0x1F, 0x8B]) {
        let mut bytes = Vec::new();
        GzDecoder::new(raw.as_slice()).take(MAX_DECODED_BYTES + 1)
            .read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_DECODED_BYTES { return None; }
        bytes
    } else if raw.first() == Some(&b'{') { raw }
    else { return None; };
    serde_json::from_slice(&decoded).ok()
}
fn inspect_root(root: &Path) -> ResearchReport {
    let mut files = Vec::new();
    walk(root, &mut files, 0);
    let examined_files = files.len();
    let mut decoded_json = 0usize;
    let mut typed_records = 0usize;
    let mut types: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
    let mut fields = FieldMap::new();
    for path in files {
        let Some(setting) = read_setting(&path) else { continue; };
        decoded_json += 1;
        let Some(setting_type) = setting.get("Type").and_then(Value::as_str) else { continue; };
        let Some(data) = setting.get("Data") else { continue; };
        typed_records += 1;
        let kind = safe_setting_type(setting_type);
        *types.entry((kind.clone(), shape(data))).or_default() += 1;
        describe_data(&kind, "Data", data, 0, &mut fields);
    }
    let record_types = types.into_iter().map(|((setting_type, value_shape), records)| RecordType {
        wireless_candidate: wireless_candidate(&setting_type),
        setting_type, value_shape, records,
    }).collect();
    let field_rows_truncated = fields.len() > MAX_FIELD_ROWS;
    let fields = fields.into_iter().take(MAX_FIELD_ROWS)
        .map(|((setting_type, field_path, value_shape), stats)| FieldShape {
            setting_type, field_path, value_shape, occurrences: stats.occurrences,
            stored_array_min: stats.min, stored_array_max: stats.max,
        }).collect();
    ResearchReport {
        examined_files, decoded_json, typed_records, record_types, fields,
        field_rows_truncated, binding_groups_verified: false,
        safety_note: "Offline structural inventory only. No live RF query, DeviceID, filenames, paths or raw setting values exported.",
        hints: vec![
            "Arrays describe saved configuration shapes; their lengths are NOT live fan counts.".into(),
            "Wireless-named setting types are only candidates; binding topology remains unverified.".into(),
            "No WinUSB/HID operations, no service interruption, no fan/RGB/LCD changes.".into(),
        ],
    }
}
pub fn inspect() -> ResearchReport {
    let base = std::env::var_os("PROGRAMDATA").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    inspect_root(&base.join("Lian-Li").join("L-Connect 3").join("device"))
}
pub fn export(report: &ResearchReport) -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("wireless-settings-schema-v012.json");
    fs::write(&path, serde_json::to_vec_pretty(report).map_err(|e|e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn allowlist_never_exposes_untrusted_field_names() {
        assert_eq!(safe_setting_type("ab:cd:ef:12"), "[redacted]");
        assert_eq!(safe_field_name("MySecretSerial"), "[other]");
        assert_eq!(safe_field_name("Fans"), "fans");
    }
    #[test] fn nested_settings_shapes_not_values() {
        let setting = serde_json::json!({
            "groups": [{"fanCount": 3, "Serial_ABC": "personal-device-123", "model": "TL"}],
            "MySecretSerial": "top-secret-device-id"
        });
        let mut fields = FieldMap::new();
        describe_data("Fan", "Data", &setting, 0, &mut fields);
        let report: Vec<_> = fields.iter().map(|((typ,path,kind),v)| {
            format!("{typ} {path} {kind} {}", v.occurrences)
        }).collect();
        let text = report.join(" ");
        assert!(text.contains("Data.groups"));
        assert!(text.contains("fanCount"));
        assert!(!text.contains("personal-device"));
        assert!(!text.contains("MySecretSerial"));
        assert!(!text.contains("top-secret"));
    }
    #[test] fn array_count_is_clearly_saved_data() {
        let mut fields = FieldMap::new();
        describe_data("GroupLCD", "Data", &serde_json::json!({"groups":[1,2,3]}), 0, &mut fields);
        let arr = &fields[&("GroupLCD".into(), "Data.groups".into(), "array")];
        assert_eq!(arr.min, Some(3));
        assert_eq!(arr.max, Some(3));
    }
    #[test] fn empty_directory_is_safe() {
        let r = inspect_root(Path::new("this-directory-does-not-exist-lianlite"));
        assert_eq!(r.typed_records, 0);
        assert!(!r.binding_groups_verified);
    }
}
