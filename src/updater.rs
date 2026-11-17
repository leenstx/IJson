use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) const PLATFORM_ASSET_NAME: &str = "IJson-arm64-macos.dmg";

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) const PLATFORM_ASSET_NAME: &str = "IJson-x64-macos.dmg";

#[cfg(target_os = "windows")]
pub(crate) const PLATFORM_ASSET_NAME: &str = "IJson-x64-windows.zip";

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const PLATFORM_ASSET_NAME: &str = "";

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

    pub(crate) fn download(&self, ctx: egui::Context, release: ReleaseInfo) {
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
                    inner.status = UpdateStatus::Downloaded {
                        version: release.version.clone(),
                        file_path: path.clone(),
                    };
                    // Automatically open installer on macOS
                    #[cfg(target_os = "macos")]
                    {
                        let _ = std::process::Command::new("open").arg(&path).spawn();
                    }
                }
                Err(err) => {
                    inner.status = UpdateStatus::Error(err);
                }
            }
            ctx.request_repaint();
        });
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
                    for a in assets {
                        if let Some(aname) = a.get("name").and_then(|s| s.as_str()) {
                            if aname == PLATFORM_ASSET_NAME {
                                asset_name = Some(aname.to_string());
                                download_url = a.get("browser_download_url").and_then(|s| s.as_str()).map(str::to_string);
                                asset_size = a.get("size").and_then(|n| n.as_u64());
                                break;
                            }
                        }
                    }
                }

                if download_url.is_none() && !PLATFORM_ASSET_NAME.is_empty() {
                    download_url = Some(format!(
                        "https://github.com/leenstx/IJson/releases/download/{tag}/{PLATFORM_ASSET_NAME}"
                    ));
                    asset_name = Some(PLATFORM_ASSET_NAME.to_string());
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
    let download_url = if !PLATFORM_ASSET_NAME.is_empty() {
        Some(format!(
            "https://github.com/leenstx/IJson/releases/download/{tag}/{PLATFORM_ASSET_NAME}"
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
        asset_name: if PLATFORM_ASSET_NAME.is_empty() { None } else { Some(PLATFORM_ASSET_NAME.to_string()) },
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
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            let dl = PathBuf::from(home).join("Downloads");
            if dl.is_dir() {
                return dl;
            }
        }
    } else if cfg!(target_os = "windows") {
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let dl = PathBuf::from(userprofile).join("Downloads");
            if dl.is_dir() {
                return dl;
            }
        }
    }
    std::env::temp_dir()
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
        // Test how ScrollStyle is documented or structured
        let s = egui::style::ScrollStyle::default();
        println!("{:#?}", s);
        assert!(is_newer("v0.0.14", "0.0.13"));
    }
}
