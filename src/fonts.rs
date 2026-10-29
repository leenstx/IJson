use std::collections::HashSet;

use ab_glyph::Font;
use eframe::egui;

pub(crate) struct FontChoice {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) search_name: String,
    face: Option<fontdb::ID>,
}

pub(crate) struct AppFonts {
    pub(crate) selected: usize,
    pub(crate) available: Vec<FontChoice>,
    pub(crate) search: String,
    pub(crate) error: Option<String>,
    definitions: egui::FontDefinitions,
    database: fontdb::Database,
}

impl AppFonts {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let mut fonts = egui::FontDefinitions::default();

        if let Ok(data) = std::fs::read("/System/Library/Fonts/Menlo.ttc") {
            fonts
                .font_data
                .insert("menlo".to_owned(), egui::FontData::from_owned(data).into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "menlo".to_owned());
        }

        if let Ok(data) = std::fs::read("/System/Library/Fonts/Apple Symbols.ttf") {
            fonts.font_data.insert(
                "apple_symbols".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("apple_symbols".to_owned());
            }
        }

        // Keep Chinese and symbol fallbacks for every selectable primary font.
        for path in [
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/System/Library/Fonts/Supplemental/Songti.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("cjk".to_owned(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "cjk".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk".to_owned());
                break;
            }
        }

        // Scan once, including user fonts and macOS downloadable font assets.
        // The database keeps metadata; selected font bytes are loaded on demand.
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let mut seen = HashSet::new();
        let mut available: Vec<FontChoice> = database
            .faces()
            .filter(|face| {
                !face.post_script_name.starts_with('.')
                    && seen.insert(face.post_script_name.clone())
            })
            .map(|face| {
                let aliases = face
                    .families
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                FontChoice {
                    key: face.post_script_name.clone(),
                    label: face.post_script_name.replace('-', " · "),
                    search_name: format!("{} {aliases}", face.post_script_name).to_lowercase(),
                    face: Some(face.id),
                }
            })
            .collect();
        available.sort_unstable_by(|a, b| a.search_name.cmp(&b.search_name));
        available.insert(
            0,
            FontChoice {
                key: "default".to_owned(),
                label: "Default font".to_owned(),
                search_name: "default font 默认字体".to_owned(),
                face: None,
            },
        );
        ctx.set_fonts(fonts.clone());
        Self {
            selected: 0,
            available,
            search: String::new(),
            error: None,
            definitions: fonts,
            database,
        }
    }

    pub(crate) fn restore(&mut self, key: &str) {
        // Migrate the previous fixed presets to stable system PostScript names.
        let key = match key {
            "menlo" => "Menlo-Regular",
            "monaco" => "Monaco",
            "sf_mono" => ".SFNSMono-Light",
            "helvetica_neue" => "HelveticaNeue",
            "songti" => "STSongti-SC-Regular",
            other => other,
        };
        self.selected = self
            .available
            .iter()
            .position(|choice| choice.key == key)
            .unwrap_or(0);
    }

    pub(crate) fn apply(&mut self, ctx: &egui::Context, index: usize) -> bool {
        let Some(choice) = self.available.get(index) else {
            self.error = Some("Font is no longer available".to_owned());
            return false;
        };
        let mut fonts = self.definitions.clone();
        if let Some(face) = choice.face {
            let loaded = self.database.with_face_data(face, |bytes, index| {
                // Match egui's parser before installing a system font, so an
                // unsupported or damaged face produces an error, not a panic.
                let font = ab_glyph::FontRef::try_from_slice_and_index(bytes, index)
                    .map_err(|_| "Unsupported or damaged font")?;
                if !font.height_unscaled().is_finite() || font.height_unscaled() <= 0.0 {
                    return Err("Invalid font metrics");
                }
                let mut data = egui::FontData::from_owned(bytes.to_vec());
                data.index = index;
                Ok(data)
            });
            let data = match loaded {
                Some(Ok(data)) => data,
                Some(Err(error)) => {
                    self.error = Some(format!("{}: {error}", choice.label));
                    return false;
                }
                None => {
                    self.error = Some(format!("{}: Font file is unavailable", choice.label));
                    return false;
                }
            };
            // Only retain the current face, not every font the user has tried.
            let key = format!("system:{}", choice.key);
            fonts.font_data.insert(key.clone(), data.into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, key.clone());
            }
        }
        ctx.set_fonts(fonts);
        self.selected = index;
        self.error = None;
        true
    }
}
