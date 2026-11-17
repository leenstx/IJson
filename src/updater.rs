use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) const PLATFORM_ASSET_NAMES: &[&str] = &["IJson-arm64-macos.app.zip", "IJson-arm64-macos.dmg"];

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) const PLATFORM_ASSET_NAMES: &[&str] = &["IJson-x64-macos.app.zip", "IJson-x64-macos.dmg"];

#[cfg(target_os = "windows")]
pub(crate) const PLATFORM_ASSET_NAMES: &[&str] = &["IJson-x64-windows.zip"];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const PLATFORM_ASSET_NAMES: &[&str] = &[];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReleaseInfo {
    pub(crate) tag: String,
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) body: String,
    pub(crate) html_url: String,
    pub(crate) download_url: Option<String>,
    pub(crate) asset_name: Option<String>,
    pub(crate) asset_size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(ReleaseInfo),
    Downloading {
        version: String,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        progress: f32,
    },
    Installing {
        version: String,
    },
    Downloaded {
        version: String,
        file_path: PathBuf,
    },
    Error(String),
}

#[derive(Clone)]
pub(crate) struct Updater {
    inner: Arc<Mutex<UpdaterInner>>,
}

struct UpdaterInner {
    status: UpdateStatus,
    is_checking: bool,
    is_downloading: bool,
}

impl Updater {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(UpdaterInner {
                status: UpdateStatus::Idle,
                is_checking: false,
                is_downloading: false,
            })),
        }
    }

    pub(crate) fn status(&self) -> UpdateStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub(crate) fn has_update(&self) -> bool {
        matches!(
            self.status(),
            UpdateStatus::Available(_)
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::Installing { .. }
                | UpdateStatus::Downloaded { .. }
        )
    }

    pub(crate) fn check(&self, ctx: egui::Context, force: bool) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.is_checking || inner.is_downloading {
                return;
            }
            if !force && matches!(inner.status, UpdateStatus::Available(_) | UpdateStatus::Downloaded { .. }) {
                return;
            }
            inner.is_checking = true;
            inner.status = UpdateStatus::Checking;
        }
        ctx.request_repaint();

        let inner_clone = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let res = fetch_latest_release();
            let mut inner = inner_clone.lock().unwrap();
            inner.is_checking = false;
            match res {
                Ok(info) => {
                    if is_newer(&info.tag, CURRENT_VERSION) {
                        inner.status = UpdateStatus::Available(info);
                    } else {
                        inner.status = UpdateStatus::UpToDate;
                    }
                }
                Err(err) => {
                    inner.status = UpdateStatus::Error(err);
                }
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn download(&self, ctx: egui::Context, release: ReleaseInfo, auto_install: bool) {
        let download_url = match &release.download_url {
            Some(u) => u.clone(),
            None => {
                let mut inner = self.inner.lock().unwrap();
                inner.status = UpdateStatus::Error("No download URL found for this release".to_string());
                return;
            }
        };

        {
            let mut inner = self.inner.lock().unwrap();
            if inner.is_downloading {
                return;
            }
            inner.is_downloading = true;
            inner.status = UpdateStatus::Downloading {
                version: release.version.clone(),
                downloaded_bytes: 0,
                total_bytes: release.asset_size,
                progress: 0.0,
            };
        }
        ctx.request_repaint();

        let inner_clone = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let res = download_file(&download_url, &release, &ctx, &inner_clone);
            let mut inner = inner_clone.lock().unwrap();
            inner.is_downloading = false;
            match res {
                Ok(path) => {
                    if auto_install {
                        inner.status = UpdateStatus::Installing {
                            version: release.version.clone(),
                        };
                        drop(inner);
                        ctx.request_repaint();

                        if let Err(err) = install_and_restart(&path) {
                            let mut inner = inner_clone.lock().unwrap();
                            inner.status = UpdateStatus::Downloaded {
                                version: release.version.clone(),
                                file_path: path,
                            };
                            eprintln!("Auto-install failed: {err}");
                        }
                    } else {
                        inner.status = UpdateStatus::Downloaded {
                            version: release.version.clone(),
                            file_path: path,
                        };
                    }
                }
                Err(err) => {
                    inner.status = UpdateStatus::Error(err);
                }
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn apply_and_restart(&self, path: &Path) {
        let _ = install_and_restart(path);
    }

    pub(crate) fn open_installer(&self, path: &Path) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(path).spawn();
        }
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(path).spawn();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
    }
}

pub(crate) fn parse_version(s: &str) -> Vec<u64> {
    let s = s.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let core = s.split('-').next().unwrap_or(s);
    core.split('.')
        .map(|seg| seg.parse::<u64>().unwrap_or(0))
        .collect()
}

pub(crate) fn is_newer(remote: &str, current: &str) -> bool {
    let r_parts = parse_version(remote);
    let c_parts = parse_version(current);
    let max_len = r_parts.len().max(c_parts.len());
    for i in 0..max_len {
        let r = r_parts.get(i).copied().unwrap_or(0);
        let c = c_parts.get(i).copied().unwrap_or(0);
        if r > c {
            return true;
        } else if r < c {
            return false;
        }
    }
    false
}

fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    // 1. First attempt: GitHub Releases API
    let api_url = "https://api.github.com/repos/leenstx/IJson/releases/latest";
    let api_resp = ureq::get(api_url)
        .set("User-Agent", "IJson-Desktop")
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(Duration::from_secs(8))
        .call();

    if let Ok(resp) = api_resp {
        if let Ok(text) = resp.into_string() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(tag) = v.get("tag_name").and_then(|s| s.as_str()) {
                let tag = tag.to_string();
                let version = tag.trim_start_matches(|c| c == 'v' || c == 'V').to_string();
                let name = v.get("name").and_then(|s| s.as_str()).unwrap_or(&tag).to_string();
                let body = v.get("body").and_then(|s| s.as_str()).unwrap_or("").to_string();
                let html_url = v
                    .get("html_url")
                    .and_then(|s| s.as_str())
                    .unwrap_or(&format!("https://github.com/leenstx/IJson/releases/tag/{tag}"))
                    .to_string();

                let mut download_url = None;
                let mut asset_size = None;
                let mut asset_name = None;

                if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
                    'find_asset: for target_name in PLATFORM_ASSET_NAMES {
                        for a in assets {
                            if let Some(aname) = a.get("name").and_then(|s| s.as_str()) {
                                if aname == *target_name {
                                    asset_name = Some(aname.to_string());
                                    download_url = a.get("browser_download_url").and_then(|s| s.as_str()).map(str::to_string);
                                    asset_size = a.get("size").and_then(|n| n.as_u64());
                                    break 'find_asset;
                                }
                            }
                        }
                    }
                }

                let fallback_name = PLATFORM_ASSET_NAMES.first().copied().unwrap_or("");
                if download_url.is_none() && !fallback_name.is_empty() {
                    download_url = Some(format!(
                        "https://github.com/leenstx/IJson/releases/download/{tag}/{fallback_name}"
                    ));
                    asset_name = Some(fallback_name.to_string());
                }

                return Ok(ReleaseInfo {
                    tag,
                    version,
                    name,
                    body,
                    html_url,
                    download_url,
                    asset_name,
                    asset_size,
                });
                }
            }
        }
    }

    // 2. Fallback: GitHub latest release web redirect (never rate-limited by GitHub API)
    let web_url = "https://github.com/leenstx/IJson/releases/latest";
    let tag = match ureq::builder()
        .redirects(0)
        .timeout(Duration::from_secs(8))
        .build()
        .get(web_url)
        .set("User-Agent", "IJson-Desktop")
        .call()
    {
        Ok(r) => r
            .header("location")
            .and_then(extract_tag_from_url)
            .or_else(|| extract_tag_from_url(r.get_url())),
        Err(ureq::Error::Status(code, resp)) if (300..=399).contains(&code) => {
            resp.header("location").and_then(extract_tag_from_url)
        }
        _ => None,
    };

    // 3. Fallback: Follow redirects in case environment forces redirects
    let tag = match tag {
        Some(t) => Some(t),
        None => {
            if let Ok(resp) = ureq::get(web_url)
                .set("User-Agent", "IJson-Desktop")
                .timeout(Duration::from_secs(8))
                .call()
            {
                extract_tag_from_url(resp.get_url())
            } else {
                None
            }
        }
    };

    // 4. Fallback: Parse GitHub tags page
    let tag = match tag {
        Some(t) => Some(t),
        None => {
            let tags_url = "https://github.com/leenstx/IJson/tags";
            if let Ok(resp) = ureq::get(tags_url)
                .set("User-Agent", "IJson-Desktop")
                .timeout(Duration::from_secs(8))
                .call()
            {
                if let Ok(html) = resp.into_string() {
                    extract_tag_from_url(&html)
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let tag = tag.ok_or_else(|| "Could not determine latest version tag from GitHub".to_string())?;
    let version = tag.trim_start_matches(|c| c == 'v' || c == 'V').to_string();
    let fallback_name = PLATFORM_ASSET_NAMES.first().copied().unwrap_or("");
    let download_url = if !fallback_name.is_empty() {
        Some(format!(
            "https://github.com/leenstx/IJson/releases/download/{tag}/{fallback_name}"
        ))
    } else {
        None
    };

    Ok(ReleaseInfo {
        tag: tag.clone(),
        version,
        name: format!("IJson {tag}"),
        body: String::new(),
        html_url: format!("https://github.com/leenstx/IJson/releases/tag/{tag}"),
        download_url,
        asset_name: if fallback_name.is_empty() { None } else { Some(fallback_name.to_string()) },
        asset_size: None,
    })
}

fn extract_tag_from_url(url: &str) -> Option<String> {
    if let Some(idx) = url.find("/releases/tag/") {
        let tag = &url[idx + "/releases/tag/".len()..];
        let tag = tag.split(['/', '?', '#']).next().unwrap_or(tag);
        if !tag.is_empty() {
            return Some(tag.to_string());
        }
    }
    None
}

fn download_dest_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("IJson-updates");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn download_file(
    url: &str,
    release: &ReleaseInfo,
    ctx: &egui::Context,
    inner_arc: &Arc<Mutex<UpdaterInner>>,
) -> Result<PathBuf, String> {
    let filename = release
        .asset_name
        .clone()
        .unwrap_or_else(|| format!("IJson-{}-update.bin", release.version));

    let dest_dir = download_dest_dir();
    let dest_path = dest_dir.join(&filename);

    let resp = ureq::get(url)
        .set("User-Agent", "IJson-Desktop")
        .timeout(Duration::from_secs(60))
        .call()
        .map_err(|e| format!("Download error: {e}"))?;

    let total_bytes = resp
        .header("content-length")
        .and_then(|s| s.parse::<u64>().ok())
        .or(release.asset_size);

    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&dest_path).map_err(|e| format!("Cannot create file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    let mut last_repaint = std::time::Instant::now();

    loop {
        use std::io::Read;
        let n = reader.read(&mut buf).map_err(|e| format!("Read error: {e}"))?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buf[..n]).map_err(|e| format!("Write error: {e}"))?;
        downloaded += n as u64;

        let progress = total_bytes
            .map(|tot| if tot > 0 { (downloaded as f32 / tot as f32).clamp(0.0, 1.0) } else { 0.0 })
            .unwrap_or(0.0);

        if last_repaint.elapsed() > Duration::from_millis(80) {
            last_repaint = std::time::Instant::now();
            let mut inner = inner_arc.lock().unwrap();
            inner.status = UpdateStatus::Downloading {
                version: release.version.clone(),
                downloaded_bytes: downloaded,
                total_bytes,
                progress,
            };
            ctx.request_repaint();
        }
    }

    Ok(dest_path)
}

#[cfg(target_os = "macos")]
fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let canonical = exe.canonicalize().unwrap_or(exe);
    let mut curr = canonical.as_path();
    while let Some(parent) = curr.parent() {
        if parent.extension().and_then(|ext| ext.to_str()) == Some("app") {
            return Some(parent.to_path_buf());
        }
        curr = parent;
    }
    let default_app = PathBuf::from("/Applications/IJson.app");
    if default_app.exists() {
        return Some(default_app);
    }
    None
}

#[cfg(target_os = "macos")]
fn extract_app_bundle(archive_path: &Path, staging_dir: &Path) -> Result<PathBuf, String> {
    let filename = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if filename.ends_with(".zip") {
        let status = std::process::Command::new("ditto")
            .arg("-x")
            .arg("-k")
            .arg(archive_path)
            .arg(staging_dir)
            .status()
            .map_err(|e| format!("Failed to run ditto: {e}"))?;
        if !status.success() {
            return Err("Failed to extract update zip".to_string());
        }
    } else if filename.ends_with(".dmg") {
        let mount_point = staging_dir.join("dmg_mount");
        let _ = std::fs::create_dir_all(&mount_point);

        let attach_status = std::process::Command::new("hdiutil")
            .arg("attach")
            .arg(archive_path)
            .arg("-nobrowse")
            .arg("-readonly")
            .arg("-mountpoint")
            .arg(&mount_point)
            .status()
            .map_err(|e| format!("Failed to attach DMG: {e}"))?;

        if !attach_status.success() {
            return Err("Failed to mount DMG disk image".to_string());
        }

        let mut app_src = None;
        if let Ok(entries) = std::fs::read_dir(&mount_point) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("app") {
                    app_src = Some(p);
                    break;
                }
            }
        }

        let app_src = match app_src {
            Some(s) => s,
            None => {
                let _ = std::process::Command::new("hdiutil")
                    .arg("detach")
                    .arg(&mount_point)
                    .arg("-force")
                    .status();
                return Err("No .app bundle found inside DMG".to_string());
            }
        };

        let target_in_staging = staging_dir.join("IJson.app");
        let copy_status = std::process::Command::new("ditto")
            .arg(&app_src)
            .arg(&target_in_staging)
            .status()
            .map_err(|e| format!("Failed to copy app from DMG: {e}"))?;

        let _ = std::process::Command::new("hdiutil")
            .arg("detach")
            .arg(&mount_point)
            .arg("-force")
            .status();

        if !copy_status.success() {
            return Err("Failed to copy .app bundle from DMG".to_string());
        }
    } else {
        return Err(format!("Unsupported archive format: {filename}"));
    }

    if let Ok(entries) = std::fs::read_dir(staging_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("app") {
                return Ok(p);
            }
        }
    }

    Err("Could not find extracted .app bundle".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn install_and_restart(downloaded_archive: &Path) -> Result<(), String> {
    let target_app = current_app_bundle().unwrap_or_else(|| PathBuf::from("/Applications/IJson.app"));

    let staging_parent = std::env::temp_dir().join(format!("ijson_update_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging_parent);
    std::fs::create_dir_all(&staging_parent)
        .map_err(|e| format!("Cannot create staging dir: {e}"))?;

    let staged_app = extract_app_bundle(downloaded_archive, &staging_parent)?;

    let staged_exe = staged_app.join("Contents/MacOS/IJson");
    if staged_exe.exists() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&staged_exe) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&staged_exe, perms);
        }
    }

    let pid = std::process::id();
    let script_path = staging_parent.join("install_and_restart.sh");

    let script_content = format!(
        r#"#!/bin/sh
PID="{pid}"
STAGED_APP="{staged_app}"
TARGET_APP="{target_app}"
STAGE_PARENT="{staging_parent}"

while kill -0 "$PID" 2>/dev/null; do
    sleep 0.1
done

rm -rf "$TARGET_APP"
ditto "$STAGED_APP" "$TARGET_APP"
xattr -r -d com.apple.quarantine "$TARGET_APP" 2>/dev/null || true
touch "$TARGET_APP"
rm -rf "$STAGE_PARENT"

open -a "$TARGET_APP" || open "$TARGET_APP"
rm -f "$0"
"#,
        pid = pid,
        staged_app = staged_app.display(),
        target_app = target_app.display(),
        staging_parent = staging_parent.display(),
    );

    std::fs::write(&script_path, script_content)
        .map_err(|e| format!("Cannot write update script: {e}"))?;

    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));

    std::process::Command::new("/bin/sh")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot spawn update script: {e}"))?;

    std::process::exit(0);
}

#[cfg(target_os = "windows")]
pub(crate) fn install_and_restart(downloaded_archive: &Path) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| format!("Cannot get current exe: {e}"))?;
    let staging_parent = std::env::temp_dir().join(format!("ijson_update_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging_parent);
    std::fs::create_dir_all(&staging_parent)
        .map_err(|e| format!("Cannot create staging dir: {e}"))?;

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                downloaded_archive.display(),
                staging_parent.display()
            ),
        ])
        .status()
        .map_err(|e| format!("Failed to expand archive: {e}"))?;

    if !status.success() {
        return Err("Failed to extract update zip".to_string());
    }

    let mut new_exe = None;
    if let Ok(entries) = std::fs::read_dir(&staging_parent) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.file_name().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) == Some("ijson.exe".to_string()) {
                new_exe = Some(p);
                break;
            }
        }
    }

    let new_exe = new_exe.ok_or_else(|| "Could not find IJson.exe in extracted update".to_string())?;
    let pid = std::process::id();
    let bat_path = staging_parent.join("update_restart.cmd");

    let bat_content = format!(
        r#"@echo off
set PID={pid}
set NEW_EXE="{new_exe}"
set TARGET_EXE="{target_exe}"
set STAGE_DIR="{staging_parent}"

:WAIT_LOOP
timeout /t 1 /nobreak >nul
tasklist /fi "PID eq %PID%" 2>nul | find "%PID%" >nul
if not errorlevel 1 goto WAIT_LOOP

copy /y %NEW_EXE% %TARGET_EXE% >nul
start "" %TARGET_EXE%
rmdir /s /q %STAGE_DIR% >nul 2>&1
exit
"#,
        pid = pid,
        new_exe = new_exe.display(),
        target_exe = current_exe.display(),
        staging_parent = staging_parent.display(),
    );

    std::fs::write(&bat_path, bat_content).map_err(|e| format!("Cannot write batch script: {e}"))?;

    std::process::Command::new("cmd")
        .args(["/C", "start", "/min", "", &bat_path.display().to_string()])
        .spawn()
        .map_err(|e| format!("Cannot spawn batch updater: {e}"))?;

    std::process::exit(0);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn install_and_restart(downloaded_archive: &Path) -> Result<(), String> {
    let _ = std::process::Command::new("xdg-open").arg(downloaded_archive).spawn();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tag_from_various_urls() {
        assert_eq!(
            extract_tag_from_url("https://github.com/leenstx/IJson/releases/tag/v0.0.13"),
            Some("v0.0.13".to_string())
        );
        assert_eq!(
            extract_tag_from_url("/leenstx/IJson/releases/tag/v0.0.14"),
            Some("v0.0.14".to_string())
        );
        assert_eq!(
            extract_tag_from_url("https://github.com/leenstx/IJson/releases/tag/v0.1.0?query=1"),
            Some("v0.1.0".to_string())
        );
        assert_eq!(extract_tag_from_url("https://github.com/leenstx/IJson"), None);
    }

    #[test]
    fn test_version_comparison() {
        assert!(is_newer("v0.0.14", "0.0.13"));
        assert!(!is_newer("v0.0.13", "0.0.14"));
        assert!(!is_newer("v0.0.14", "0.0.14"));
    }

    #[test]
    fn test_platform_assets() {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        assert!(!PLATFORM_ASSET_NAMES.is_empty());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_current_app_bundle_resolution() {
        // Function should run without panic
        let _ = current_app_bundle();
    }
}
