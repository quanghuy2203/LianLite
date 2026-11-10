use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, time::Duration};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO: &str = "quanghuy2203/LianLite";
const ZIP_ASSET: &str = "lianlite-windows-x64.zip";
const HASH_ASSET: &str = "lianlite-windows-x64.zip.sha256";
const MAX_ZIP_BYTES: u64 = 150 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Available { pub version: String, pub zip_url: String, pub hash_url: String }
#[derive(Deserialize)]
struct Release { tag_name: String, draft: bool, prerelease: bool, assets: Vec<Asset> }
#[derive(Deserialize)]
struct Asset { name: String, browser_download_url: String, size: u64 }

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(30))
        .user_agent("LianLite-Updater/0.4").build().map_err(|e| e.to_string())
}
pub fn check() -> Result<Option<Available>, String> {
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = client()?.get(api).send().map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None); // No published releases yet.
    }
    let release: Release = response.error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;
    if release.draft || release.prerelease { return Ok(None); }
    let remote = semver::Version::parse(release.tag_name.trim_start_matches('v')).map_err(|e| e.to_string())?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    if remote <= current { return Ok(None); }
    let zip = release.assets.iter().find(|a| a.name == ZIP_ASSET && a.size <= MAX_ZIP_BYTES && a.size > 0)
        .ok_or("Release has no valid Windows ZIP asset")?;
    let hash = release.assets.iter().find(|a| a.name == HASH_ASSET && a.size > 0 && a.size < 4096)
        .ok_or("Release has no SHA256 checksum asset")?;
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    for url in [&zip.browser_download_url, &hash.browser_download_url] {
        if !url.starts_with(&prefix) { return Err("Unexpected release asset URL".into()); }
    }
    Ok(Some(Available { version: remote.to_string(), zip_url: zip.browser_download_url.clone(), hash_url: hash.browser_download_url.clone() }))
}
fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = client()?.get(url).send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n > limit) { return Err("Download exceeds size limit".into()); }
    let mut data = Vec::new();
    response.by_ref().take(limit + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > limit { return Err("Download exceeds size limit".into()); }
    Ok(data)
}
pub fn stage(a: &Available) -> Result<PathBuf, String> {
    let checksum = String::from_utf8(download(&a.hash_url, 4096)?).map_err(|e| e.to_string())?;
    let expected = checksum.split_whitespace().next().ok_or("Empty checksum")?.to_ascii_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) { return Err("Invalid checksum format".into()); }
    let bytes = download(&a.zip_url, MAX_ZIP_BYTES)?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected { return Err("SHA256 mismatch; update rejected".into()); }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if archive.len() != 1 { return Err("ZIP must contain exactly lianlite.exe".into()); }
    let mut entry = archive.by_index(0).map_err(|e| e.to_string())?;
    if entry.name() != "lianlite.exe" || entry.size() > MAX_ZIP_BYTES { return Err("Unexpected ZIP entry".into()); }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if exe.file_name().and_then(|x| x.to_str()).map(|s| !s.eq_ignore_ascii_case("lianlite.exe")).unwrap_or(true) {
        return Err("Auto-update requires installed executable named lianlite.exe".into());
    }
    let staged = exe.with_extension("exe.pending");
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&staged).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&staged); }
    result.map(|_| staged)
}
pub fn launch_helper(staged: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let helper = exe.with_file_name("lianlite-updater.exe");
    if !helper.is_file() { return Err("lianlite-updater.exe is missing next to the app".into()); }
    std::process::Command::new(helper).arg("--apply").arg(&exe).arg(staged)
        .spawn().map_err(|e| e.to_string())?;
    Ok(())
}
