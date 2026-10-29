use eframe::egui;
use serde_json::Value;

pub(crate) fn json_editor_id(tab_id: u64) -> egui::Id {
    egui::Id::new("json_main_code_editor").with(tab_id)
}

pub(crate) fn search_input_id(tab_id: u64) -> egui::Id {
    egui::Id::new("search_query_input").with(tab_id)
}

pub(crate) fn session_cache_path() -> Option<std::path::PathBuf> {
    let dir = if cfg!(target_os = "macos") {
        let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
        home.join("Library/Application Support/IJson")
    } else if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            std::path::PathBuf::from(appdata).join("IJson")
        } else if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            std::path::PathBuf::from(userprofile).join("AppData/Roaming/IJson")
        } else {
            return None;
        }
    } else {
        let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
        home.join(".config/IJson")
    };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("session.json"))
}

pub(crate) struct WindowGeom {
    pub(crate) inner_x: f32,
    pub(crate) inner_y: f32,
    pub(crate) outer_x: f32,
    pub(crate) outer_y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) maximized: bool,
    pub(crate) fullscreen: bool,
}

impl WindowGeom {
    pub(crate) fn from_rects(
        inner: egui::Rect,
        outer: egui::Rect,
        maximized: bool,
        fullscreen: bool,
    ) -> Self {
        Self {
            inner_x: inner.min.x,
            inner_y: inner.min.y,
            outer_x: outer.min.x,
            outer_y: outer.min.y,
            width: inner.width(),
            height: inner.height(),
            maximized,
            fullscreen,
        }
    }

    pub(crate) fn approx_eq(&self, other: &Self) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() < 1.0;
        close(self.inner_x, other.inner_x)
            && close(self.inner_y, other.inner_y)
            && close(self.outer_x, other.outer_x)
            && close(self.outer_y, other.outer_y)
            && close(self.width, other.width)
            && close(self.height, other.height)
            && self.maximized == other.maximized
            && self.fullscreen == other.fullscreen
    }

    pub(crate) fn to_json(&self) -> Value {
        serde_json::json!({
            "inner_x": self.inner_x,
            "inner_y": self.inner_y,
            "outer_x": self.outer_x,
            "outer_y": self.outer_y,
            "width": self.width,
            "height": self.height,
            "maximized": self.maximized,
            "fullscreen": self.fullscreen,
        })
    }

    pub(crate) fn from_value(v: &Value) -> Option<Self> {
        let num = |key| v.get(key).and_then(|x| x.as_f64()).map(|n| n as f32);
        let geom = Self {
            inner_x: num("inner_x")?,
            inner_y: num("inner_y")?,
            outer_x: num("outer_x")?,
            outer_y: num("outer_y")?,
            width: num("width")?,
            height: num("height")?,
            maximized: v
                .get("maximized")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            fullscreen: v
                .get("fullscreen")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
        };
        if !geom.width.is_finite()
            || !geom.height.is_finite()
            || geom.width < 200.0
            || geom.height < 150.0
        {
            return None;
        }
        if ![geom.inner_x, geom.inner_y, geom.outer_x, geom.outer_y]
            .into_iter()
            .all(|n| n.is_finite())
        {
            return None;
        }
        Some(geom)
    }

    pub(crate) fn apply(&self, viewport: egui::ViewportBuilder) -> egui::ViewportBuilder {
        // macOS `with_position` is the inner origin; other platforms use the outer frame.
        let pos = if cfg!(target_os = "macos") {
            [self.inner_x, self.inner_y]
        } else {
            [self.outer_x, self.outer_y]
        };
        viewport
            .with_inner_size([self.width, self.height])
            .with_position(pos)
            .with_maximized(self.maximized)
            .with_fullscreen(self.fullscreen)
    }
}

pub(crate) fn read_window_geom() -> Option<WindowGeom> {
    let text = std::fs::read_to_string(session_cache_path()?).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    WindowGeom::from_value(v.get("window")?)
}
