use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;

use crate::i18n::tr;
use crate::theme::UiColors;

enum CacheEntry {
    Loading,
    Ready {
        texture: egui::TextureHandle,
        dimensions: [usize; 2],
        bytes: Arc<Vec<u8>>,
    },
    Downloaded {
        bytes: Arc<Vec<u8>>,
    },
    Failed(String),
}

static IMAGE_CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Check if a string represents an image link.
pub(crate) fn is_image_url(s: &str) -> bool {
    let s = s.trim();
    if s.starts_with("data:image/") {
        return true;
    }
    if !s.starts_with("http://") && !s.starts_with("https://") {
        return false;
    }
    let path = s.split(['?', '#']).next().unwrap_or(s);
    let path_lower = path.to_ascii_lowercase();
    const IMG_EXTS: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico", ".svg", ".avif", ".tiff", ".tif",
    ];
    if IMG_EXTS.iter().any(|ext| path_lower.ends_with(ext)) {
        return true;
    }
    let s_lower = s.to_ascii_lowercase();
    s_lower.contains("format=png")
        || s_lower.contains("format=jpg")
        || s_lower.contains("format=jpeg")
        || s_lower.contains("format=webp")
        || s_lower.contains("f=png")
        || s_lower.contains("f=jpg")
        || s_lower.contains("f=webp")
}

/// Suggest an appropriate file name when downloading an image.
pub(crate) fn suggest_filename(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.starts_with("data:image/") {
        let after_prefix = &trimmed["data:image/".len()..];
        let subtype = after_prefix.split(';').next().unwrap_or("png");
        let ext = match subtype {
            "jpeg" => "jpg",
            "svg+xml" => "svg",
            other => other,
        };
        return format!("image.{}", ext);
    }

    let path = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    if let Some(last_seg) = path.rsplit('/').next() {
        let clean = last_seg.trim();
        if !clean.is_empty() && clean.contains('.') {
            return clean.to_string();
        }
    }
    "image.png".to_string()
}

fn fetch_bytes_sync(url: &str) -> Result<Vec<u8>, String> {
    let trimmed = url.trim();
    if trimmed.starts_with("data:image/") {
        let comma_pos = trimmed.find(',').ok_or("Invalid data URI: missing comma")?;
        let header = &trimmed[..comma_pos];
        let data = &trimmed[comma_pos + 1..];
        if header.contains(";base64") {
            use base64::Engine as _;
            base64::prelude::BASE64_STANDARD
                .decode(data.trim())
                .map_err(|e| format!("Base64 decode error: {}", e))
        } else {
            Ok(data.as_bytes().to_vec())
        }
    } else {
        let resp = ureq::get(trimmed)
            .timeout(std::time::Duration::from_secs(15))
            .call()
            .map_err(|e| format!("Network error: {}", e))?;
        let mut bytes = Vec::new();
        resp.into_reader()
            .take(25 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Read error: {}", e))?;
        Ok(bytes)
    }
}

pub(crate) fn get_cached_bytes(url: &str) -> Option<Vec<u8>> {
    let lock = cache().lock().ok()?;
    match lock.get(url)? {
        CacheEntry::Ready { bytes, .. } | CacheEntry::Downloaded { bytes } => {
            Some((**bytes).clone())
        }
        _ => None,
    }
}

/// Prompt save file dialog and download/save the image to disk.
pub(crate) fn save_image_dialog(url: &str) {
    let filename = suggest_filename(url);
    let mut dialog = rfd::FileDialog::new().set_file_name(&filename);
    if let Some(ext) = filename.rsplit('.').next() {
        dialog = dialog.add_filter("Image", &[ext]);
    }
    let Some(save_path) = dialog.save_file() else {
        return;
    };

    if let Some(bytes) = get_cached_bytes(url) {
        let _ = std::fs::write(&save_path, bytes);
        return;
    }

    let url_owned = url.to_string();
    std::thread::spawn(move || {
        if let Ok(bytes) = fetch_bytes_sync(&url_owned) {
            let _ = std::fs::write(&save_path, bytes);
        }
    });
}

/// Render the image preview popup on hover.
pub(crate) fn show_image_preview(ui: &mut egui::Ui, url: &str) {
    let url_str = url.to_string();

    // 1. Check or update cache
    {
        let mut lock = cache().lock().unwrap();
        if let Some(entry) = lock.get_mut(url) {
            if let CacheEntry::Downloaded { bytes } = entry {
                // Decode on UI thread and create TextureHandle
                let bytes_clone = bytes.clone();
                match image::load_from_memory(&bytes_clone) {
                    Ok(dyn_img) => {
                        let rgba = dyn_img.to_rgba8();
                        let dims = [rgba.width() as usize, rgba.height() as usize];
                        let color_img =
                            egui::ColorImage::from_rgba_unmultiplied(dims, &rgba.into_raw());
                        let texture =
                            ui.ctx()
                                .load_texture(url, color_img, egui::TextureOptions::LINEAR);
                        *entry = CacheEntry::Ready {
                            texture,
                            dimensions: dims,
                            bytes: bytes_clone,
                        };
                    }
                    Err(err) => {
                        *entry = CacheEntry::Failed(format!("Decode error: {}", err));
                    }
                }
            }
        } else {
            // First time seeing this URL: trigger fetch
            lock.insert(url_str.clone(), CacheEntry::Loading);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let res = fetch_bytes_sync(&url_str);
                let mut lock = cache().lock().unwrap();
                match res {
                    Ok(bytes) => {
                        lock.insert(
                            url_str,
                            CacheEntry::Downloaded {
                                bytes: Arc::new(bytes),
                            },
                        );
                    }
                    Err(err) => {
                        lock.insert(url_str, CacheEntry::Failed(err));
                    }
                }
                ctx.request_repaint();
            });
        }
    }

    // 2. Render content based on current cache state
    let p = UiColors::current(ui);
    ui.set_max_width(380.0);
    egui::Frame::default()
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            let lock = cache().lock().unwrap();
            match lock.get(url) {
                Some(CacheEntry::Ready {
                    texture,
                    dimensions,
                    bytes,
                }) => {
                    let orig_w = dimensions[0] as f32;
                    let orig_h = dimensions[1] as f32;
                    let max_w = 340.0f32;
                    let max_h = 240.0f32;
                    let scale = (max_w / orig_w).min(max_h / orig_h).min(1.0);
                    let disp_w = (orig_w * scale).max(1.0);
                    let disp_h = (orig_h * scale).max(1.0);

                    egui::Frame::default()
                        .fill(p.subtle)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(2))
                        .show(ui, |ui| {
                            ui.image(egui::load::SizedTexture::new(
                                texture.id(),
                                egui::vec2(disp_w, disp_h),
                            ));
                        });

                    ui.add_space(6.0);
                    let size_kb = bytes.len() as f64 / 1024.0;
                    let size_str = if size_kb >= 1024.0 {
                        format!("{:.1} MB", size_kb / 1024.0)
                    } else {
                        format!("{:.1} KB", size_kb)
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} px  •  {}",
                            dimensions[0], dimensions[1], size_str
                        ))
                        .size(11.0)
                        .color(p.muted),
                    );
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Right-click to download"))
                            .size(10.5)
                            .color(p.faint),
                    );
                }
                Some(CacheEntry::Loading) | Some(CacheEntry::Downloaded { .. }) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Loading image..."))
                                .size(12.0)
                                .color(p.muted),
                        );
                    });
                }
                Some(CacheEntry::Failed(err)) => {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Failed to load image"))
                                .size(12.0)
                                .color(p.danger),
                        );
                    });
                    ui.label(
                        egui::RichText::new(err)
                            .size(10.5)
                            .color(p.muted),
                    );
                }
                None => {}
            }
        });
}
