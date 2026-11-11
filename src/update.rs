use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, time::Duration};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO: &str = "quanghuy2203/LianLite";
const PORTABLE: &str = "LianLite-Portable-Windows-x64.zip";
const CHECKSUM: &str = "LianLite-Portable-Windows-x64.zip.sha256";
const MAX_BYTES: u64 = 150 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Available { pub version: String, zip_url: String, hash_url: String }
#[derive(Debug)]
pub struct Staged { pub app: PathBuf, pub helper: PathBuf }
#[derive(Deserialize)]
struct Release { tag_name: String, draft: bool, prerelease: bool, assets: Vec<Asset> }
#[derive(Deserialize)]
struct Asset { name: String, browser_download_url: String, size: u64 }

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .user_agent(concat!("LianLite-Updater/", env!("CARGO_PKG_VERSION")))
        .build().map_err(|e| e.to_string())
}
pub fn check() -> Result<Option<Available>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = client()?.get(url).send().map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND { return Ok(None); }
    let release: Release = response.error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;
    if release.draft || release.prerelease { return Ok(None); }
    let remote = semver::Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|e| e.to_string())?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| e.to_string())?;
    if remote <= current { return Ok(None); }
    let zip = release.assets.iter().find(|a| a.name == PORTABLE && a.size > 0 && a.size <= MAX_BYTES)
        .ok_or("Release lacks portable ZIP with app and updater")?;
    let hash = release.assets.iter().find(|a| a.name == CHECKSUM && a.size > 0 && a.size <= 4096)
        .ok_or("Release lacks portable ZIP checksum")?;
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    if !zip.browser_download_url.starts_with(&prefix) || !hash.browser_download_url.starts_with(&prefix) {
        return Err("Unexpected asset download origin".into());
    }
    Ok(Some(Available { version: remote.to_string(),
        zip_url: zip.browser_download_url.clone(), hash_url: hash.browser_download_url.clone() }))
}
fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = client()?.get(url).send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|size| size > limit) {
        return Err("Release download is too large".into());
    }
    let mut buffer = Vec::new();
    response.by_ref().take(limit + 1).read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    if buffer.len() as u64 > limit { return Err("Release exceeds size limit".into()); }
    Ok(buffer)
}
fn read_executable(archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> Result<Vec<u8>, String> {
    let mut file = archive.by_name(name).map_err(|e| format!("Missing {name}: {e}"))?;
    if file.size() == 0 || file.size() > MAX_BYTES {
        return Err(format!("Invalid size for {name}"));
    }
    let mut contents = Vec::new();
    file.by_ref().take(MAX_BYTES + 1).read_to_end(&mut contents).map_err(|e| e.to_string())?;
    if contents.len() as u64 != file.size() || contents.len() < 2 || &contents[..2] != b"MZ" {
        return Err(format!("{name} is incomplete or not a Windows executable"));
    }
    Ok(contents)
}
fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("download");
    let result = (|| -> Result<(), String> {
        let mut f = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        f.write_all(contents).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}
pub fn stage(a: &Available) -> Result<Staged, String> {
    let text = String::from_utf8(download(&a.hash_url, 4096)?).map_err(|e| e.to_string())?;
    let expected = text.split_whitespace().next().ok_or("Missing SHA256")?.to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid SHA256 format".into());
    }
    let data = download(&a.zip_url, MAX_BYTES)?;
    if format!("{:x}", Sha256::digest(&data)) != expected {
        return Err("SHA256 verification failed".into());
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    if archive.len() != 2 { return Err("Portable ZIP must contain exactly two executables".into()); }
    let app = read_executable(&mut archive, "lianlite.exe")?;
    let helper = read_executable(&mut archive, "lianlite-updater.exe")?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !exe.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("lianlite.exe")) {
        return Err("Application executable must be named lianlite.exe".into());
    }
    let staged_app = exe.with_extension("exe.pending");
    let staging_root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir).join("LianLite").join("update-staging");
    let helper_dir = staging_root.join(format!("v{}", a.version));
    fs::create_dir_all(&helper_dir).map_err(|e| e.to_string())?;
    let staged_helper = helper_dir.join("lianlite-updater.exe");

    // The helper runs OUTSIDE the installed directory so it can replace its installed copy.
    write_atomic(&staged_helper, &helper)?;
    write_atomic(&staged_app, &app)?;
    Ok(Staged { app: staged_app, helper: staged_helper })
}
pub fn launch_helper(staged: &Staged) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !staged.helper.is_file() || !staged.app.is_file() {
        return Err("Staged application/updater is missing".into());
    }
    std::process::Command::new(&staged.helper)
        .arg("--apply").arg(&exe).arg(&staged.app)
        .spawn().map_err(|e| format!("Could not start staged updater: {e}"))?;
    Ok(())
}
