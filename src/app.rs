use eframe::egui;
use serde_json::Value;
use std::ops::{Deref, DerefMut};

use crate::cache::{
    json_editor_id, read_window_geom, search_input_id, session_cache_path, WindowGeom,
};
use crate::compare::Comparison;
use crate::fold::{draw_fold_chevron, folded_selection_source};
use crate::fonts::AppFonts;
use crate::highlight::highlight_json;
use crate::icons::{draw_lucide_icon, LucideIcon};
use crate::model::{
    collect_all_expandable_paths, is_request_candidate, try_request_json, try_unescape_json,
    SortMode, TreeAction, ValueTypeTag,
};
use crate::tab::{
    local_stamp_now, stamp_from_system_time, tab_from_value, unique_tab_name, JsonTab,
    TAB_RAIL_WIDTH,
};
use crate::i18n::{tr, Language};
use crate::theme::{ThemeColors, ThemeMode, UiColors};
use crate::tree::render_tree_node;
use crate::widgets::{
    btn_clear, btn_collapse_all, btn_compare, btn_compress, btn_copy, btn_expand_all, btn_format,
    btn_new_tab, btn_redo, btn_simplify, btn_sort, btn_undo, field_caption, info_pill, menu_item,
    menu_separator, modal_button, type_chip,
};

pub(crate) struct JsonViewerApp {
    tabs: Vec<JsonTab>,
    active: usize,
    next_id: u64,
    renaming: Option<TabRename>,
    reveal_active: bool,
    // Cmd+W also asks the window to close. Cancel that for a couple of frames.
    suppress_close_frames: u8,

    tab_rail_width: f32,
    left_panel_width: f32,
    // False until the user drags the splitter. Then the saved width is restored.
    split_customized: bool,
    language: Language,
    theme_mode: ThemeMode,
    fonts: AppFonts,

    cache_dirty: bool,
    last_cache_save: Option<std::time::Instant>,
    window_geom: Option<WindowGeom>,
    window_pos_restored: bool,
    window_settle_frames: u8,
}

struct TabRename {
    id: u64,
    buffer: String,
    focus_requested: bool,
}

#[derive(Default)]
struct RailAction {
    add: bool,
    activate: Option<usize>,
    close: Option<usize>,
    pin: Option<usize>,
    start_rename: Option<u64>,
    commit_rename: bool,
    cancel_rename: bool,
}

impl JsonViewerApp {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let mut app = Self {
            tabs: vec![JsonTab::empty(1, local_stamp_now())],
            active: 0,
            next_id: 2,
            renaming: None,
            reveal_active: false,
            suppress_close_frames: 0,
            tab_rail_width: TAB_RAIL_WIDTH,
            left_panel_width: 380.0,
            split_customized: false,
            language: Language::English,
            theme_mode: ThemeMode::Light,
            fonts: AppFonts::new(ctx),
            cache_dirty: false,
            last_cache_save: None,
            window_geom: read_window_geom(),
            window_pos_restored: false,
            window_settle_frames: 0,
        };
        app.load_cache();
        app.language.apply(ctx);
        app.theme_mode.apply(ctx);
        let font_choice = app.fonts.selected;
        if font_choice != 0 && !app.fonts.apply(ctx, font_choice) {
            app.fonts.selected = 0;
        }
        app
    }

    fn harvest_tab_dirty(&mut self) {
        for tab in &mut self.tabs {
            self.cache_dirty |= tab.dirty;
            tab.dirty = false;
            if let Some(comparison) = &mut tab.comparison {
                self.cache_dirty |= comparison.right.dirty;
                comparison.right.dirty = false;
            }
        }
    }

    fn load_cache(&mut self) {
        let Some(path) = session_cache_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        if let Some(code) = v.get("language").and_then(Value::as_str) {
            self.language = match code {
                "zh" => Language::Chinese,
                _ => Language::English,
            };
        }
        if let Some(code) = v.get("theme_mode").and_then(Value::as_str) {
            self.theme_mode = match code {
                "dark" => ThemeMode::Dark,
                "system" => ThemeMode::System,
                _ => ThemeMode::Light,
            };
        }
        if let Some(code) = v.get("font_choice").and_then(Value::as_str) {
            self.fonts.restore(code);
        }
        if let Some(width) = v.get("tab_rail_width").and_then(Value::as_f64) {
            if width.is_finite() {
                self.tab_rail_width = width.clamp(120.0, 400.0) as f32;
            }
        }
        self.split_customized = v
            .get("split_customized")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        if self.split_customized {
            if let Some(width) = v.get("left_panel_width").and_then(|x| x.as_f64()) {
                self.left_panel_width = width as f32;
            }
        }

        if let Some(loaded) = self.tabs_from_session(&v) {
            self.install_loaded_tabs(loaded, v.get("active_id").and_then(|x| x.as_u64()));
            if let Some(next_id) = v.get("next_id").and_then(|x| x.as_u64()) {
                self.next_id = self.next_id.max(next_id);
            }
            self.cache_dirty = false;
            return;
        }

        // v1 stored a single document at the top level.
        if v.get("input").and_then(|x| x.as_str()).is_some() {
            let stamp = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(stamp_from_system_time)
                .unwrap_or_else(local_stamp_now);
            let mut legacy = v;
            legacy["name"] = Value::String(stamp.clone());
            legacy["auto_name"] = Value::String(stamp);
            legacy["id"] = Value::from(1_u64);
            if let Some(loaded) = tab_from_value(&legacy, 1) {
                self.install_loaded_tabs(vec![loaded], Some(1));
                self.cache_dirty = false;
            }
        }
    }

    fn tabs_from_session(&self, v: &Value) -> Option<Vec<crate::tab::LoadedTab>> {
        let tabs = v.get("tabs").and_then(|x| x.as_array())?;
        if tabs.is_empty() {
            return None;
        }
        let mut loaded = Vec::new();
        let mut next_fallback = 1_u64;
        for tab in tabs {
            let Some(item) = tab_from_value(tab, next_fallback) else {
                continue;
            };
            next_fallback = next_fallback.max(item.tab.id.saturating_add(1));
            loaded.push(item);
        }
        if loaded.is_empty() {
            None
        } else {
            Some(loaded)
        }
    }

    fn install_loaded_tabs(&mut self, loaded: Vec<crate::tab::LoadedTab>, active_id: Option<u64>) {
        let mut max_id = 0_u64;
        self.tabs = loaded
            .into_iter()
            .map(|mut item| {
                max_id = max_id.max(item.tab.id);
                if let Some(comparison) = &item.tab.comparison {
                    max_id = max_id.max(comparison.right.id);
                }
                item.tab.finish_load(item.collapsed, item.search_index);
                item.tab
            })
            .collect();
        if self.tabs.is_empty() {
            self.tabs.push(JsonTab::empty(1, local_stamp_now()));
            max_id = max_id.max(1);
        }
        self.next_id = max_id.saturating_add(1).max(self.next_id);
        self.active = active_id
            .and_then(|id| self.tabs.iter().position(|tab| tab.id == id))
            .unwrap_or(0);
        if self.active >= self.tabs.len() {
            self.active = 0;
        }
    }

    fn flush_cache(&mut self, force: bool) {
        if !force {
            if !self.cache_dirty {
                return;
            }
            if let Some(saved_at) = self.last_cache_save {
                if saved_at.elapsed() < std::time::Duration::from_millis(800) {
                    return;
                }
            }
        }
        self.write_cache();
    }

    fn write_cache(&mut self) {
        let Some(path) = session_cache_path() else {
            return;
        };
        let active_id = self.tabs.get(self.active).map(|tab| tab.id);
        let tabs: Vec<Value> = self.tabs.iter().map(|tab| tab.to_json()).collect();
        let mut value = serde_json::json!({
            "v": 2,
            "active_id": active_id,
            "next_id": self.next_id,
            "tabs": tabs,
            "tab_rail_width": self.tab_rail_width,
            "left_panel_width": self.left_panel_width,
            "split_customized": self.split_customized,
            "language": self.language.code(),
            "theme_mode": self.theme_mode.code(),
            "font_choice": self.fonts.available[self.fonts.selected].key,
        });
        if let Some(win) = &self.window_geom {
            value["window"] = win.to_json();
        }
        let Ok(text) = serde_json::to_string(&value) else {
            return;
        };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            self.cache_dirty = false;
            self.last_cache_save = Some(std::time::Instant::now());
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    fn surrender_focus(&mut self, ctx: &egui::Context, tab_id: u64) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return;
        };
        tab.editor_editing = false;
        let right_id = tab.comparison.as_mut().map(|comparison| {
            comparison.right.editor_editing = false;
            comparison.right.id
        });
        ctx.memory_mut(|mem| {
            mem.surrender_focus(json_editor_id(tab_id));
            mem.surrender_focus(search_input_id(tab_id));
            mem.surrender_focus(egui::Id::new("tab_rename").with(tab_id));
            if let Some(right_id) = right_id {
                mem.surrender_focus(json_editor_id(right_id));
            }
        });
    }

    fn add_tab(&mut self, ctx: &egui::Context) {
        self.commit_rename();
        let previous = self.tabs[self.active].id;
        self.surrender_focus(ctx, previous);
        let id = self.alloc_id();
        let name = unique_tab_name(&self.tabs, &local_stamp_now());
        let tab = JsonTab::empty(id, name);
        // New tabs are inserted at the top so they are visible immediately.
        self.tabs.insert(0, tab);
        self.active = 0;
        self.reveal_active = true;
        self.cache_dirty = true;
    }

    fn add_comparison_tab(&mut self, ctx: &egui::Context) {
        self.commit_rename();
        let source = &self.tabs[self.active];
        let input = source.input.clone();
        let name = unique_tab_name(&self.tabs, &format!("{} · {}", tr(ctx, "Compare"), source.name));
        let previous = source.id;
        self.surrender_focus(ctx, previous);
        let id = self.alloc_id();
        let right_id = self.alloc_id();
        let mut tab = JsonTab::empty(id, name);
        // Keep an exact snapshot; later edits never change the source tab.
        tab.input = input;
        tab.reparse();
        tab.comparison = Some(Box::new(Comparison::new(JsonTab::empty(
            right_id,
            String::new(),
        ))));
        self.tabs.insert(0, tab);
        self.active = 0;
        self.reveal_active = true;
        self.cache_dirty = true;
        ctx.request_repaint();
    }

    fn close_tab(&mut self, ctx: &egui::Context, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        let closing_id = self.tabs[index].id;
        let active_id = self.tabs[self.active].id;
        if self
            .renaming
            .as_ref()
            .is_some_and(|rename| rename.id == closing_id)
        {
            self.renaming = None;
        }
        self.surrender_focus(ctx, closing_id);
        self.tabs.remove(index);
        if closing_id == active_id {
            self.active = index.min(self.tabs.len() - 1);
            self.editor_editing = false;
        } else {
            self.active = self.index_of(active_id).unwrap_or(0);
        }
        self.cache_dirty = true;
    }

    fn toggle_pin(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let active_id = self.tabs[self.active].id;
        let mut tab = self.tabs.remove(index);
        if tab.pinned {
            tab.pinned = false;
            let at = self
                .tabs
                .iter()
                .position(|item| !item.pinned)
                .unwrap_or(self.tabs.len());
            self.tabs.insert(at, tab);
        } else {
            tab.pinned = true;
            self.tabs.insert(0, tab);
        }
        self.active = self.index_of(active_id).unwrap_or(0);
        self.cache_dirty = true;
    }

    fn activate_tab(&mut self, ctx: &egui::Context, index: usize) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }
        self.commit_rename();
        let previous = self.tabs[self.active].id;
        self.surrender_focus(ctx, previous);
        self.active = index;
        self.editor_editing = false;
        self.cache_dirty = true;
    }

    fn start_rename(&mut self, id: u64) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        self.renaming = Some(TabRename {
            id,
            buffer: tab.name.clone(),
            focus_requested: false,
        });
    }

    fn cancel_rename(&mut self) {
        self.renaming = None;
    }

    fn commit_rename(&mut self) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == rename.id) else {
            return;
        };
        let next = rename.buffer.trim();
        if next.is_empty() {
            tab.name = tab.auto_name.clone();
        } else if next != tab.name {
            tab.name = next.to_string();
        } else {
            return;
        }
        self.cache_dirty = true;
    }
}

// Keyboard commands target the selected editor; tab metadata uses `tabs[active]`.
impl Deref for JsonViewerApp {
    type Target = JsonTab;

    fn deref(&self) -> &Self::Target {
        let tab = &self.tabs[self.active];
        if let Some(comparison) = &tab.comparison {
            if comparison.right_active {
                return &comparison.right;
            }
        }
        tab
    }
}

impl DerefMut for JsonViewerApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let tab = &mut self.tabs[self.active];
        if tab
            .comparison
            .as_ref()
            .is_some_and(|comparison| comparison.right_active)
        {
            return &mut tab.comparison.as_mut().unwrap().right;
        }
        tab
    }
}

impl eframe::App for JsonViewerApp {
    /// Eliminate any background black bar by matching the active theme canvas
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let closing = ctx.input(|i| i.viewport().close_requested());
        self.note_window_geom(ctx);
        self.restore_window_outer_position(ctx);
        self.render_ui(ctx);
        self.harvest_tab_dirty();
        self.flush_cache(closing);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_cache(true);
    }
}

impl JsonViewerApp {
    fn note_window_geom(&mut self, ctx: &egui::Context) {
        let (inner, outer, maximized, fullscreen, minimized) = ctx.input(|i| {
            let v = i.viewport();
            (
                v.inner_rect,
                v.outer_rect,
                v.maximized.unwrap_or(false),
                v.fullscreen.unwrap_or(false),
                v.minimized.unwrap_or(false),
            )
        });
        if minimized {
            return;
        }
        let (Some(inner), Some(outer)) = (inner, outer) else {
            return;
        };
        if inner.width() < 200.0 || inner.height() < 150.0 {
            return;
        }
        // Ignore the first frames so a not-yet-restored position is not written back.
        if self.window_settle_frames < 2 {
            self.window_settle_frames += 1;
            return;
        }
        let next = WindowGeom::from_rects(inner, outer, maximized, fullscreen);
        if self
            .window_geom
            .as_ref()
            .is_some_and(|g| g.approx_eq(&next))
        {
            return;
        }
        self.window_geom = Some(next);
        self.cache_dirty = true;
    }

    fn restore_window_outer_position(&mut self, ctx: &egui::Context) {
        if self.window_pos_restored {
            return;
        }
        let Some(saved) = self.window_geom.as_ref() else {
            self.window_pos_restored = true;
            return;
        };
        if saved.maximized || saved.fullscreen {
            self.window_pos_restored = true;
            return;
        }
        let Some(outer) = ctx.input(|i| i.viewport().outer_rect) else {
            return;
        };
        self.window_pos_restored = true;
        let saved_pos = egui::pos2(saved.outer_x, saved.outer_y);
        if (outer.min - saved_pos).length() > 2.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(saved_pos));
        }
    }

    fn select_comparison_editor(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let Some(comparison) = &mut tab.comparison else {
            return;
        };
        let mut right_active = comparison.right_active;
        if ctx.input(|i| i.pointer.any_pressed()) {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                let right_rect = comparison.right.editor_card_rect;
                let left_rect = tab.editor_card_rect;
                // Include the toolbars, so subsequent shortcuts use the last operated side.
                if right_rect.expand2(egui::vec2(0.0, 34.0)).contains(pos) {
                    right_active = true;
                } else if left_rect.expand2(egui::vec2(0.0, 34.0)).contains(pos) {
                    right_active = false;
                }
            }
        } else if ctx.memory(|mem| mem.has_focus(json_editor_id(comparison.right.id))) {
            right_active = true;
        } else if ctx.memory(|mem| mem.has_focus(json_editor_id(tab.id))) {
            right_active = false;
        }
        if comparison.right_active != right_active {
            let previous = if right_active {
                tab.editor_editing = false;
                tab.id
            } else {
                comparison.right.editor_editing = false;
                comparison.right.id
            };
            ctx.memory_mut(|mem| mem.surrender_focus(json_editor_id(previous)));
            comparison.right_active = right_active;
        }
    }

    fn render_split_handle(
        &mut self,
        ui: &mut egui::Ui,
        panel_rect: egui::Rect,
        pane_rect: egui::Rect,
    ) {
        // Overlay the divider hit area so resizing needs no visible gap.
        let handle_rect = egui::Rect::from_min_size(
            egui::pos2(panel_rect.left() + self.left_panel_width - 4.0, panel_rect.top()),
            egui::vec2(8.0, panel_rect.height()),
        );
        let response = ui.interact(
            handle_rect,
            ui.id().with("split_handle"),
            egui::Sense::drag(),
        );
        if response.dragged() {
            self.split_customized = true;
            self.left_panel_width += response.drag_delta().x;
            self.cache_dirty = true;
        }
        let active = response.hovered() || response.dragged();
        if active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        // Use the same vertical bounds as both scrollable panes.
        let bar = egui::Rect::from_center_size(
            egui::pos2(handle_rect.center().x, pane_rect.center().y),
            egui::vec2(if active { 2.0 } else { 1.0 }, pane_rect.height()),
        );
        let p = UiColors::current(ui);
        ui.painter().rect_filled(
            bar,
            egui::CornerRadius::same(1),
            if active { p.blue } else { p.border },
        );
    }

    fn render_comparison(
        &mut self,
        ui: &mut egui::Ui,
        panel_rect: egui::Rect,
        colors: ThemeColors,
    ) {
        let total_w = panel_rect.width();
        if !self.split_customized {
            self.left_panel_width = total_w * 0.5;
        }
        // Both toolbars must fit their eight buttons at the minimum window size.
        self.left_panel_width = self
            .left_panel_width
            .clamp(232.0, (total_w - 232.0).max(232.0));
        let left_w = self.left_panel_width;
        let right_w = (total_w - left_w).max(100.0);
        let card_top = panel_rect.top() + 34.0;
        let card_h = (panel_rect.bottom() - card_top).max(50.0);
        let right_x = panel_rect.left() + left_w;
        let background_rect = egui::Rect::from_min_size(
            egui::pos2(panel_rect.left(), card_top),
            egui::vec2(total_w, card_h),
        );
        ui.painter().rect_filled(
            background_rect,
            egui::CornerRadius::ZERO,
            colors.ui.surface,
        );
        let pane_rect = background_rect.shrink2(egui::vec2(0.0, 8.0));

        // Temporarily detach the right document to reuse the same editor for both sides.
        let tab = &mut self.tabs[self.active];
        let mut comparison = tab.comparison.take().unwrap();
        let p = colors.ui;

        for (document, x, width) in [
            (&mut *tab, panel_rect.left(), left_w),
            (&mut comparison.right, right_x, right_w),
        ] {
            let rect =
                egui::Rect::from_min_size(egui::pos2(x, panel_rect.top()), egui::vec2(width, 28.0));
            let mut toolbar = ui.new_child(
                egui::UiBuilder::default()
                    .id_salt(("editor_toolbar", document.id))
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            toolbar.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
            document.render_editor_toolbar(&mut toolbar);
            document.poll_request_loading(ui.ctx());
        }

        comparison.refresh(&tab.input, tab.revision);

        // egui turns Cmd+scroll into zoom, so read those wheel events directly
        // and scroll both editors by the same amount.
        let line_scroll_speed = ui.ctx().options(|o| o.line_scroll_speed);
        let (immediate, wheel, dt) = ui.input(|i| {
            let hovered = i.pointer.hover_pos().is_some_and(|pos| pane_rect.contains(pos));
            let (mut immediate, mut wheel) = (0.0, 0.0);
            for event in &i.events {
                let egui::Event::MouseWheel { unit, delta, modifiers } = event else {
                    continue;
                };
                if !hovered || !modifiers.command || modifiers.shift {
                    continue;
                }
                // Same conversion and smoothing split as egui's own scrolling.
                match unit {
                    egui::MouseWheelUnit::Point if delta.y.abs() < 8.0 => immediate += delta.y,
                    egui::MouseWheelUnit::Point => wheel += delta.y,
                    egui::MouseWheelUnit::Line => wheel += delta.y * line_scroll_speed,
                    egui::MouseWheelUnit::Page => wheel += delta.y * i.screen_rect.height(),
                }
            }
            (immediate, wheel, i.stable_dt.min(0.1))
        });
        comparison.pending_sync_scroll += wheel;
        let pending = comparison.pending_sync_scroll;
        let eased = if pending.abs() < 1.0 {
            pending
        } else {
            pending * egui::emath::exponential_smooth_factor(0.90, 0.1, dt)
        };
        comparison.pending_sync_scroll -= eased;
        let requested = immediate + eased;
        if requested != 0.0 {
            // Stop at the first edge either editor reaches.
            let docs = [&*tab, &comparison.right];
            let room_up = docs.iter().map(|d| d.editor_scroll_y).fold(f32::INFINITY, f32::min);
            let room_down = docs
                .iter()
                .map(|d| d.editor_scroll_max_y - d.editor_scroll_y)
                .fold(f32::INFINITY, f32::min);
            // A positive delta scrolls toward the top.
            let sync_scroll = requested.clamp(-room_down.max(0.0), room_up.max(0.0));
            if sync_scroll != requested {
                comparison.pending_sync_scroll = 0.0;
            }
            if sync_scroll != 0.0 {
                tab.editor_sync_scroll = sync_scroll;
                comparison.right.editor_sync_scroll = sync_scroll;
            }
        }
        if comparison.pending_sync_scroll != 0.0 {
            ui.ctx().request_repaint();
        }

        for (document, x, width, lines, color) in [
            (
                &mut *tab,
                panel_rect.left(),
                left_w,
                comparison.removed.as_slice(),
                p.danger_bg,
            ),
            (
                &mut comparison.right,
                right_x,
                right_w,
                comparison.added.as_slice(),
                p.added,
            ),
        ] {
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, pane_rect.top()),
                egui::vec2(width, pane_rect.height()),
            );
            let mut editor_ui = ui.new_child(
                egui::UiBuilder::default()
                    .id_salt(("compare_editor", document.id))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            document.render_editor_card(
                &mut editor_ui,
                rect,
                colors,
                Some((lines, color)),
            );
        }
        // An edit made by TextEdit is reflected in the diff on the following frame.
        if comparison.right.dirty || tab.dirty {
            ui.ctx().request_repaint();
        }
        tab.comparison = Some(comparison);
        // Hint that Cmd+scroll moves both editors. Set after the editors so it
        // overrides their text cursor.
        let sync_hint = ui.input(|i| {
            i.modifiers.command
                && i.pointer.hover_pos().is_some_and(|pos| pane_rect.contains(pos))
        });
        if sync_hint {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        self.render_split_handle(ui, panel_rect, pane_rect);
    }

    fn render_top_bar(&mut self, ctx: &egui::Context, colors: ThemeColors) {
        let p = colors.ui;
        egui::TopBottomPanel::top("app_window_titlebar")
            .exact_height(30.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(p.titlebar)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 12,
                        top: 3,
                        bottom: 3,
                    }),
            )
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // Window drag interaction on empty titlebar space
                let drag_resp = ui.interact(rect, ui.id().with("titlebar_drag"), egui::Sense::click_and_drag());
                if drag_resp.dragged() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_resp.double_clicked() {
                    let is_max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }

                // Window Title (placed next to macOS traffic lights)
                #[cfg(target_os = "macos")]
                let traffic_light_padding = 74.0;
                #[cfg(not(target_os = "macos"))]
                let traffic_light_padding = 4.0;

                let title_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + traffic_light_padding, rect.top()),
                    egui::vec2(120.0, rect.height()),
                );
                let mut title_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(title_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                title_ui.add(
                    egui::Label::new(
                        egui::RichText::new("IJson")
                            .size(12.5)
                            .strong()
                            .color(p.muted),
                    )
                    .selectable(false),
                );

                // Right side: Font picker, Language toggle and Theme toggle.
                let controls_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 124.0, rect.top()),
                    egui::vec2(124.0, rect.height()),
                );
                let mut controls_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(controls_rect)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                controls_ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);

                // 1. Theme toggle button (rightmost)
                let theme_icon = self.theme_mode.icon();
                let theme_tip = self.theme_mode.hint();
                let (btn_rect, resp) = controls_ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
                if resp.hovered() {
                    controls_ui.painter().rect_filled(btn_rect, egui::CornerRadius::same(4), p.hover);
                }
                let color = if resp.hovered() { p.strong } else { p.muted };
                let icon_rect = egui::Rect::from_center_size(btn_rect.center(), egui::vec2(15.0, 15.0));
                draw_lucide_icon(controls_ui.painter(), icon_rect, theme_icon, color);
                if resp.clicked() {
                    self.theme_mode = self.theme_mode.next();
                    self.theme_mode.apply(controls_ui.ctx());
                    self.cache_dirty = true;
                }
                resp.on_hover_text(tr(controls_ui.ctx(), theme_tip))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);

                // 2. Language toggle button (left of theme)
                let lang_label = self.language.label();
                let lang_tip = match self.language {
                    Language::English => "Language: English · click for 中文",
                    Language::Chinese => "语言：中文 · 点击切换为 English",
                };
                let (btn_rect, resp) = controls_ui.allocate_exact_size(egui::vec2(46.0, 22.0), egui::Sense::click());
                if resp.hovered() {
                    controls_ui.painter().rect_filled(btn_rect, egui::CornerRadius::same(4), p.hover);
                }
                let color = if resp.hovered() { p.strong } else { p.muted };
                controls_ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    lang_label,
                    egui::FontId::proportional(12.5),
                    color,
                );
                if resp.clicked() {
                    self.language = self.language.toggle();
                    self.language.apply(controls_ui.ctx());
                    self.cache_dirty = true;
                }
                resp.on_hover_text(lang_tip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);

                // 3. Global font picker (left of language), transparent when idle.
                let popup_id = controls_ui.id().with("font_picker");
                let was_open = controls_ui.memory(|mem| mem.is_popup_open(popup_id));
                let (btn_rect, resp) = controls_ui.allocate_exact_size(
                    egui::vec2(24.0, 22.0),
                    egui::Sense::click(),
                );
                if resp.clicked() {
                    controls_ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                    if !was_open {
                        self.fonts.search.clear();
                    }
                }
                let open = controls_ui.memory(|mem| mem.is_popup_open(popup_id));
                if resp.hovered() || open {
                    controls_ui.painter().rect_filled(
                        btn_rect,
                        egui::CornerRadius::same(4),
                        p.hover,
                    );
                }
                controls_ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Aa",
                    egui::FontId::proportional(12.5),
                    if resp.hovered() || open { p.strong } else { p.muted },
                );
                let mut picked = None;
                egui::popup_below_widget(
                    &controls_ui,
                    popup_id,
                    &resp,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_width(300.0);
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Global font"))
                                .small()
                                .color(p.muted),
                        );
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.fonts.search)
                                .hint_text(tr(ui.ctx(), "Search fonts…"))
                                .desired_width(f32::INFINITY),
                        );
                        if resp.clicked() && !was_open {
                            search.request_focus();
                        }
                        let query = self.fonts.search.trim().to_lowercase();
                        let mut filtered: Vec<usize> = self
                            .fonts
                            .available
                            .iter()
                            .enumerate()
                            .filter(|(_, choice)| choice.search_name.contains(&query))
                            .map(|(index, _)| index)
                            .collect();
                        if let Some(position) = filtered
                            .iter()
                            .position(|&index| index == self.fonts.selected)
                        {
                            filtered[..=position].rotate_right(1);
                        }
                        let row_h = ui.spacing().interact_size.y.max(
                            ui.text_style_height(&egui::TextStyle::Button)
                                + 2.0 * ui.spacing().button_padding.y,
                        );
                        let list_h = (ui.ctx().screen_rect().height() - 160.0).clamp(80.0, 320.0);
                        // Virtualize the system font list; only visible rows are laid out.
                        let mut list = egui::ScrollArea::vertical()
                            .id_salt("system_font_list")
                            .max_height(list_h)
                            .auto_shrink([false, true]);
                        if (resp.clicked() && !was_open) || search.changed() {
                            list = list.vertical_scroll_offset(0.0);
                        }
                        list.show_rows(ui, row_h, filtered.len(), |ui, rows| {
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::Min),
                                |ui| {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    for row in rows {
                                        let index = filtered[row];
                                        let choice = &self.fonts.available[index];
                                        if ui
                                            .add(egui::SelectableLabel::new(
                                                self.fonts.selected == index,
                                                tr(ui.ctx(), &choice.label),
                                            ))
                                            .on_hover_text(&choice.label)
                                            .clicked()
                                        {
                                            picked = Some(index);
                                        }
                                    }
                                },
                            );
                        });
                        if filtered.is_empty() {
                            ui.label(tr(ui.ctx(), "No matching fonts"));
                        }
                        ui.separator();
                        ui.label(tr(ui.ctx(), "Preview: Aa 0123 中文"));
                        if let Some(error) = &self.fonts.error {
                            ui.colored_label(p.danger, tr(ui.ctx(), "Unable to load font"));
                            ui.label(egui::RichText::new(error).small().color(p.muted));
                        }
                    },
                );
                if let Some(index) = picked {
                    if index == self.fonts.selected {
                        controls_ui.memory_mut(|mem| mem.close_popup());
                    } else if self.fonts.apply(controls_ui.ctx(), index) {
                        // Re-measure cached content widths without changing documents.
                        for tab in &mut self.tabs {
                            tab.editor_content_width = 0.0;
                            tab.tree_content_width = 0.0;
                            if let Some(comparison) = &mut tab.comparison {
                                comparison.right.editor_content_width = 0.0;
                                comparison.right.tree_content_width = 0.0;
                            }
                        }
                        self.cache_dirty = true;
                        controls_ui.memory_mut(|mem| mem.close_popup());
                    }
                }
                resp.on_hover_text(format!(
                    "{}: {}",
                    tr(controls_ui.ctx(), "Global font"),
                    tr(controls_ui.ctx(), &self.fonts.available[self.fonts.selected].label),
                ))
                .on_hover_cursor(egui::CursorIcon::PointingHand);

                // Subtle bottom border separating titlebar from workspace below
                let clip = ui.clip_rect();
                ui.painter().hline(
                    clip.x_range(),
                    clip.bottom() - 0.5,
                    egui::Stroke::new(1.0, p.border),
                );
            });
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        self.language.apply(ctx);
        let colors = ThemeColors::current(ctx);

        self.handle_tab_shortcuts(ctx);
        self.select_comparison_editor(ctx);
        self.handle_edit_history(ctx);
        // Command+V replaces the whole document only when the editor is not being edited.
        // Insert at the selection, then format valid JSON as part of the same edit.
        self.handle_paste(ctx);
        self.handle_folded_copy_cut(ctx);
        self.open_dropped_files(ctx);

        self.render_top_bar(ctx, colors);

        if self.tabs.len() > 1 {
            self.render_tab_rail(ctx, colors);
        }

        let mut open_comparison = false;
        // =========================================================================
        // Two-Column Layout using explicit rectangles for pixel-perfect alignment.
        // Both panes share the divider's vertical bounds; the outer bands use
        // the same background color without extending the scrollable viewports.
        // =========================================================================
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(colors.ui.canvas)
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 8,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
                let panel_rect = ui.available_rect_before_wrap();
                if self.tabs[self.active].comparison.is_some() {
                    self.render_comparison(ui, panel_rect, colors);
                    ui.allocate_rect(panel_rect, egui::Sense::hover());
                    return;
                }
                let total_w = panel_rect.width();
                let total_h = panel_rect.height();
                let origin = panel_rect.min;

                let min_w = if self.tabs.len() == 1 { 290.0 } else { 260.0 };
                let max_w = (total_w - 260.0).max(300.0);
                if !self.split_customized {
                    self.left_panel_width = total_w * 0.5;
                }
                self.left_panel_width = self.left_panel_width.clamp(min_w, max_w);

                let left_w = self.left_panel_width;
                let right_w = (total_w - left_w).max(100.0);

                let toolbar_h = 28.0;
                let spacing_h = 6.0;
                let card_top_y = origin.y + toolbar_h + spacing_h;
                let card_h = (origin.y + total_h - card_top_y).max(50.0);
                let background_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x, card_top_y),
                    egui::vec2(total_w, card_h),
                );
                ui.painter().rect_filled(
                    background_rect,
                    egui::CornerRadius::ZERO,
                    colors.ui.surface,
                );
                let pane_rect = background_rect.shrink2(egui::vec2(0.0, 8.0));

                // 1. LEFT TOOLBAR
                let left_tb_rect = egui::Rect::from_min_size(origin, egui::vec2(left_w, toolbar_h));
                let mut left_tb_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(left_tb_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                left_tb_ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
                if self.tabs.len() == 1 && btn_new_tab(&mut left_tb_ui).clicked() {
                    let ctx = left_tb_ui.ctx().clone();
                    self.add_tab(&ctx);
                }
                self.tabs[self.active].render_editor_toolbar(&mut left_tb_ui);
                open_comparison = btn_compare(&mut left_tb_ui).clicked();

                // 2. RIGHT TOOLBAR
                let right_tb_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x + left_w, origin.y),
                    egui::vec2(right_w, toolbar_h),
                );
                let mut right_tb_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(right_tb_rect)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                right_tb_ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);

                // Right side buttons (laid out right-to-left):
                // 1. Expand All
                if btn_expand_all(&mut right_tb_ui).clicked() {
                    self.expand_all();
                }

                // 2. Collapse All
                if btn_collapse_all(&mut right_tb_ui).clicked() {
                    self.collapse_all();
                }

                // 3. Level buttons L4, L3, L2, L1 (in reverse so L1 ends up leftmost)
                for level in (1..=4).rev() {
                    let is_active = self.current_depth == level;
                    let label = format!("L{}", level);
                    let (rect, resp) = right_tb_ui
                        .allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
                    if resp.clicked() {
                        self.expand_to_depth(level);
                    }

                    let p = colors.ui;
                    let bg = if is_active {
                        p.blue_bg
                    } else if resp.hovered() {
                        p.hover
                    } else {
                        p.subtle
                    };
                    let stroke = if is_active {
                        p.blue_border
                    } else {
                        p.border
                    };
                    let text_color = if is_active {
                        p.blue
                    } else if resp.hovered() {
                        p.strong
                    } else {
                        p.muted
                    };

                    right_tb_ui
                        .painter()
                        .rect_filled(rect, egui::CornerRadius::same(4), bg);
                    right_tb_ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::same(4),
                        egui::Stroke::new(1.0, stroke),
                        egui::StrokeKind::Inside,
                    );
                    right_tb_ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &label,
                        egui::FontId::proportional(11.5),
                        text_color,
                    );
                    resp.on_hover_text(format!("Level {}", level));
                }

                // Clean 4px spacing between L1 and Search Box
                right_tb_ui.add_space(4.0);

                // 4. Search Box (immediately to the left of L1!)
                let p = colors.ui;
                let search_w = (right_w - 240.0).clamp(70.0, 140.0);
                let search_frame = egui::Frame::default()
                    .fill(p.surface)
                    .stroke(egui::Stroke::new(
                        1.0,
                        p.border,
                    ))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(7, 3));

                search_frame.show(&mut right_tb_ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(5.0, 0.0);

                        let (icon_rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 16.0), egui::Sense::hover());
                        let search_icon_rect = egui::Rect::from_center_size(
                            icon_rect.center(),
                            egui::vec2(13.0, 13.0),
                        );
                        draw_lucide_icon(
                            ui.painter(),
                            search_icon_rect,
                            LucideIcon::Search,
                            p.faint,
                        );

                        let search_id = search_input_id(self.id);
                        // Consume navigation before TextEdit handles Enter or moves its cursor.
                        let (prev, next) = if ui.memory(|mem| mem.has_focus(search_id)) {
                            ui.input_mut(|i| {
                                let prev = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                                    || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter);
                                let next = i
                                    .consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                                (prev, next)
                            })
                        } else {
                            (false, false)
                        };
                        let search_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.search_query)
                                .id(search_id)
                                .hint_text(tr(ui.ctx(), "Search…"))
                                .desired_width(search_w)
                                .frame(false),
                        );

                        if search_resp.changed() {
                            self.update_search();
                        }
                        if prev {
                            self.search_prev();
                        } else if next {
                            self.search_next();
                        }

                        if !self.search_query.is_empty() {
                            if ui.small_button("✕").clicked() {
                                self.search_query.clear();
                                self.update_search();
                            }
                        }
                    });
                });

                // 5. Match Count Badge & Navigation (to the left of Search Box)
                if !self.search_matches.is_empty() {
                    if right_tb_ui
                        .small_button("▼")
                        .on_hover_text(tr(right_tb_ui.ctx(), "Next match"))
                        .clicked()
                    {
                        self.search_next();
                    }
                    if right_tb_ui
                        .small_button("▲")
                        .on_hover_text(tr(right_tb_ui.ctx(), "Previous match"))
                        .clicked()
                    {
                        self.search_prev();
                    }

                    let match_badge =
                        egui::Frame::default().inner_margin(egui::Margin::symmetric(5, 1));

                    match_badge.show(&mut right_tb_ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/{}",
                                self.search_index + 1,
                                self.search_matches.len()
                            ))
                            .color(p.muted)
                            .small(),
                        );
                    });
                }

                // 3. LEFT EDITOR CARD
                let left_card_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x, pane_rect.top()),
                    egui::vec2(left_w, pane_rect.height()),
                );
                let mut left_card_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(left_card_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                self.tabs[self.active].render_editor_card(
                    &mut left_card_ui,
                    left_card_rect,
                    colors,
                    None,
                );

                // 4. RIGHT TREE CARD
                let right_card_rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x + left_w, pane_rect.top()),
                    egui::vec2(right_w, pane_rect.height()),
                );
                let mut right_card_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(right_card_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                self.render_tree_card(&mut right_card_ui, right_card_rect, colors);

                // Register last so the divider takes priority over both cards.
                self.render_split_handle(ui, panel_rect, pane_rect);

                // Consume the full panel rect so egui doesn't complain
                ui.allocate_rect(panel_rect, egui::Sense::hover());
            });

        self.render_edit_modal(ctx);
        let tab = &mut self.tabs[self.active];
        tab.sync_editor_editing(ctx);
        if let Some(comparison) = &mut tab.comparison {
            comparison.right.sync_editor_editing(ctx);
        }
        if open_comparison {
            self.add_comparison_tab(ctx);
        }
    }

    fn handle_edit_history(&mut self, ctx: &egui::Context) {
        if self.edit_modal.is_some() {
            return;
        }
        let tab_id = self.id;
        let blocked = ctx.memory(|mem| {
            mem.has_focus(search_input_id(tab_id))
                || self.renaming.as_ref().is_some_and(|rename| {
                    mem.has_focus(egui::Id::new("tab_rename").with(rename.id))
                })
        });
        if blocked {
            return;
        }
        let (undo, redo) = ctx.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl;
            let z = i.key_pressed(egui::Key::Z);
            (
                cmd && z && !i.modifiers.shift,
                cmd && z && i.modifiers.shift,
            )
        });
        if !undo && !redo {
            return;
        }
        if undo {
            self.undo_edit();
        } else {
            self.redo_edit();
        }
        ctx.input_mut(|i| {
            i.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Z,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command || modifiers.mac_cmd || modifiers.ctrl
                )
            });
        });
    }

    fn handle_tab_shortcuts(&mut self, ctx: &egui::Context) {
        let (new_tab, close_tab, close_requested) = ctx.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl;
            let plain = cmd && !i.modifiers.shift;
            (
                plain && i.key_pressed(egui::Key::T),
                plain && i.key_pressed(egui::Key::W),
                i.viewport().close_requested(),
            )
        });
        if new_tab {
            self.add_tab(ctx);
            ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::T);
                i.consume_key(egui::Modifiers::CTRL, egui::Key::T);
            });
        }
        if close_tab && self.tabs.len() > 1 {
            self.close_tab(ctx, self.active);
            self.suppress_close_frames = 3;
            ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::W);
                i.consume_key(egui::Modifiers::CTRL, egui::Key::W);
            });
        }
        if self.suppress_close_frames > 0 {
            if close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            self.suppress_close_frames -= 1;
        }
    }

    fn open_dropped_files(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.dropped_files.is_empty()) {
            return;
        }
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        for file in files {
            let Some(path) = file.path else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            self.add_tab(ctx);
            self.load_text(content);
        }
    }

    fn render_tab_rail(&mut self, ctx: &egui::Context, colors: ThemeColors) {
        let mut action = RailAction::default();
        // Consume the flag before handling clicks, so a tab created this frame is revealed next frame.
        let reveal = self.reveal_active;
        self.reveal_active = false;
        // Leave enough room for both editor toolbars when the window is narrow.
        let max_width = (ctx.available_rect().width() - 490.0).clamp(TAB_RAIL_WIDTH, 400.0);
        let p = colors.ui;
        let rail_id = egui::Id::new("json_tab_rail");
        let response = egui::SidePanel::left(rail_id)
            .default_width(self.tab_rail_width)
            .width_range(120.0..=max_width)
            .resizable(true)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 6,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ctx, |ui| {
                action = self.tab_rail_contents(ui, reveal);
                let rect = ui.clip_rect();
                ui.painter().vline(
                    rect.right() - 0.5,
                    rect.y_range(),
                    egui::Stroke::new(1.0, p.border),
                );
            });
        // Match the center divider without changing other widgets' foreground colors.
        // egui 0.31 uses this ID for the SidePanel's native resize handle.
        if ctx
            .read_response(rail_id.with("__resize"))
            .is_some_and(|response| response.hovered() || response.dragged())
        {
            let rect = response.response.rect;
            ctx.layer_painter(egui::LayerId::background())
                .with_clip_rect(rect)
                .vline(
                    rect.right() - 1.0,
                    rect.y_range(),
                    egui::Stroke::new(2.0, p.blue),
                );
        }
        let width = response.response.rect.width();
        if (width - self.tab_rail_width).abs() > 0.5 {
            self.tab_rail_width = width;
            self.cache_dirty = true;
        }
        self.apply_rail_action(ctx, action);
    }

    fn tab_rail_contents(&mut self, ui: &mut egui::Ui, reveal: bool) -> RailAction {
        let mut action = RailAction::default();
        if rail_add_button(ui).clicked() {
            action.add = true;
        }
        ui.add_space(4.0);

        let rows: Vec<(u64, String, bool, bool)> = self
            .tabs
            .iter()
            .map(|tab| {
                (
                    tab.id,
                    tab.name.clone(),
                    tab.pinned,
                    tab.comparison.is_some(),
                )
            })
            .collect();
        let active_id = self.tabs[self.active].id;
        let renaming_id = self.renaming.as_ref().map(|rename| rename.id);

        egui::ScrollArea::vertical()
            .id_salt("tab_rail_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (index, (id, name, pinned, comparison)) in rows.into_iter().enumerate() {
                    if renaming_id == Some(id) {
                        self.rename_row(ui, &mut action);
                    } else {
                        tab_row(
                            ui,
                            index,
                            id,
                            &name,
                            pinned,
                            comparison,
                            id == active_id,
                            reveal && id == active_id,
                            &mut action,
                        );
                    }
                }
            });
        action
    }

    fn rename_row(&mut self, ui: &mut egui::Ui, action: &mut RailAction) {
        let Some(rename) = &mut self.renaming else {
            return;
        };
        let p = UiColors::current(ui);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), p.surface);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(6),
            egui::Stroke::new(1.0, p.blue_border),
            egui::StrokeKind::Inside,
        );
        let mut edit_ui = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(rect.shrink(4.0))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let resp = edit_ui.add(
            egui::TextEdit::singleline(&mut rename.buffer)
                .id(egui::Id::new("tab_rename").with(rename.id))
                .desired_width((rect.width() - 8.0).max(24.0))
                .frame(false),
        );
        if resp.has_focus() {
            rename.focus_requested = true;
        }
        let escape = edit_ui.input(|i| i.key_pressed(egui::Key::Escape));
        let enter = edit_ui.input(|i| i.key_pressed(egui::Key::Enter));
        if !rename.focus_requested {
            resp.request_focus();
        } else if escape {
            action.cancel_rename = true;
        } else if enter || resp.lost_focus() {
            action.commit_rename = true;
        }
    }

    fn apply_rail_action(&mut self, ctx: &egui::Context, action: RailAction) {
        if action.cancel_rename {
            self.cancel_rename();
        } else if action.commit_rename {
            self.commit_rename();
        }
        if let Some(index) = action.close {
            self.close_tab(ctx, index);
            return;
        }
        if let Some(index) = action.pin {
            self.toggle_pin(index);
        }
        if let Some(index) = action.activate {
            self.activate_tab(ctx, index);
        }
        if let Some(id) = action.start_rename {
            self.start_rename(id);
        }
        if action.add {
            self.add_tab(ctx);
        }
    }

    fn handle_paste(&mut self, ctx: &egui::Context) {
        if let Some(rename) = &self.renaming {
            let rename_id = egui::Id::new("tab_rename").with(rename.id);
            if ctx.memory(|m| m.has_focus(rename_id)) {
                return;
            }
        }
        let tab_id = self.id;
        let search_has_focus = ctx.memory(|m| m.has_focus(search_input_id(tab_id)));
        if self.edit_modal.is_some() || search_has_focus {
            return;
        }

        let paste_text = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Paste(text) if !text.is_empty() => Some(text.clone()),
                _ => None,
            })
        });
        let cmd_v_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::V)
                && (i.modifiers.mac_cmd || i.modifiers.command || i.modifiers.ctrl)
        });
        if paste_text.is_none() && !cmd_v_pressed {
            return;
        }

        let text = paste_text.or_else(|| {
            arboard::Clipboard::new()
                .ok()
                .and_then(|mut cb| cb.get_text().ok())
                .filter(|text| !text.is_empty())
        });
        let Some(mut text) = text else {
            return;
        };

        // If converting to JSON fails (or wraps an escaped JSON string), try unescaping and parsing again.
        let needs_unescape = match serde_json::from_str::<Value>(&text) {
            Err(_) => true,
            Ok(Value::String(ref s)) => {
                let trimmed = s.trim();
                (trimmed.starts_with('{') && trimmed.ends_with('}'))
                    || (trimmed.starts_with('[') && trimmed.ends_with(']'))
            }
            Ok(_) => false,
        };
        if needs_unescape {
            if let Some((_, unescaped)) = try_unescape_json(&text) {
                text = unescaped;
            } else if is_request_candidate(&text) {
                let editor_has_focus = ctx.memory(|m| m.has_focus(json_editor_id(tab_id)));
                let for_editor_paste = self.editor_editing || editor_has_focus;
                self.start_request_loading(text, for_editor_paste, ctx);
                ctx.input_mut(|i| {
                    i.events.retain(|e| !matches!(e, egui::Event::Paste(_)));
                });
                return;
            } else if let Some((_, requested)) = try_request_json(&text) {
                text = requested;
            }
        }

        let editor_has_focus = ctx.memory(|m| m.has_focus(json_editor_id(tab_id)));
        if self.editor_editing || editor_has_focus {
            // Preserve insertion/selection semantics before formatting the document.
            if !text.is_empty() {
                self.paste_into_editor(ctx, &text);
            }
        } else if !text.trim().is_empty() {
            self.replace_document(text);
            self.editor_editing = false;
        }

        ctx.input_mut(|i| {
            i.events.retain(|e| !matches!(e, egui::Event::Paste(_)));
        });
    }

    fn handle_folded_copy_cut(&mut self, ctx: &egui::Context) {
        if self.edit_modal.is_some() {
            return;
        }
        let editor_id = json_editor_id(self.id);
        let focused = ctx.memory(|m| m.has_focus(editor_id));
        if !focused && !self.editor_editing {
            return;
        }
        let (copy, cut) = ctx.input(|i| {
            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut));
            (copy, cut)
        });
        if !copy && !cut {
            return;
        }
        let Some(mut state) = egui::text_edit::TextEditState::load(ctx, editor_id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        let [start, end] = range.sorted();
        let Some(text) = folded_selection_source(
            &self.input,
            &self.editor_map,
            &self.editor_view,
            start.index,
            end.index,
        ) else {
            return;
        };
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
        ctx.input_mut(|i| {
            i.events
                .retain(|e| !matches!(e, egui::Event::Copy | egui::Event::Cut));
        });
        if cut {
            use egui::TextBuffer as _;
            let char_count = self.editor_view.chars().count();
            let start_idx = start.index.min(char_count);
            let end_idx = end.index.min(char_count);
            self.editor_view.delete_char_range(start_idx..end_idx);
            self.commit_editor_view(false);
            let max = self.editor_view.chars().count();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(start_idx.min(max)),
                )));
            state.store(ctx, editor_id);
        }
    }
}

impl JsonTab {
    fn paste_into_editor(&mut self, ctx: &egui::Context, text: &str) {
        let editor_id = json_editor_id(self.id);
        let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
        let char_count = self.editor_view.chars().count();
        let range = state
            .cursor
            .char_range()
            .unwrap_or_else(|| egui::text::CCursorRange::one(egui::text::CCursor::new(char_count)));
        let [start, end] = range.sorted();
        let start_idx = start.index.min(char_count);
        let end_idx = end.index.min(char_count);

        use egui::TextBuffer as _;
        self.editor_view.delete_char_range(start_idx..end_idx);
        let mut ccursor = egui::text::CCursor::new(start_idx);
        self.editor_view
            .insert_text_at(&mut ccursor, text, usize::MAX);
        let inserted_view = self.editor_view.clone();
        self.commit_editor_view(true);

        let max = self.editor_view.chars().count();
        // Formatting changes offsets; keep the caret within the rebuilt editor.
        if self.editor_view != inserted_view {
            ccursor.index = max;
        }
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(ccursor.index.min(max)),
            )));
        state.store(ctx, editor_id);
        ctx.memory_mut(|mem| mem.request_focus(editor_id));
        self.editor_editing = true;
    }

    fn sync_editor_editing(&mut self, ctx: &egui::Context) {
        let editor_id = json_editor_id(self.id);
        if ctx.input(|i| i.pointer.primary_clicked()) {
            let inside = ctx.input(|i| {
                i.pointer
                    .interact_pos()
                    .is_some_and(|p| self.editor_card_rect.contains(p))
            });
            if inside {
                self.editor_editing = true;
                ctx.memory_mut(|mem| mem.request_focus(editor_id));
            } else if self.edit_modal.is_none() {
                self.editor_editing = false;
                ctx.memory_mut(|mem| {
                    if mem.has_focus(editor_id) {
                        mem.surrender_focus(editor_id);
                    }
                });
            }
        } else if ctx.memory(|m| m.has_focus(editor_id)) {
            self.editor_editing = true;
        }
    }
}

impl JsonViewerApp {
    fn render_edit_modal(&mut self, ctx: &egui::Context) {
        if self.edit_modal.is_none() {
            return;
        }

        let mut close_modal = false;
        let mut save_modal = false;
        let screen = ctx.screen_rect();

        egui::Area::new(egui::Id::new("edit_modal_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(56));
                if resp.clicked() {
                    close_modal = true;
                }
            });

        let p = UiColors::from_ctx(ctx);
        let frame = egui::Frame::default()
            .fill(p.surface)
            .stroke(egui::Stroke::new(
                1.0,
                p.border,
            ))
            .corner_radius(egui::CornerRadius::same(12))
            .shadow(egui::epaint::Shadow {
                offset: [0, 12],
                blur: 28,
                spread: 0,
                color: egui::Color32::from_black_alpha(if ctx.theme() == egui::Theme::Dark { 120 } else { 40 }),
            })
            .inner_margin(egui::Margin::same(18));

        egui::Window::new("edit_value_modal")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -16.0))
            .default_width(440.0)
            .frame(frame)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.set_max_width(460.0);
                ui.spacing_mut().item_spacing.y = 4.0;

                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 36.0), egui::Sense::hover());
                    let edit_icon_rect =
                        egui::Rect::from_center_size(icon_rect.center(), egui::vec2(18.0, 18.0));
                    draw_lucide_icon(
                        ui.painter(),
                        edit_icon_rect,
                        LucideIcon::Pencil,
                        p.primary,
                    );
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Edit Value"))
                                .size(16.0)
                                .strong()
                                .color(p.strong),
                        );
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Update this node and write it back to the editor"))
                                .size(11.5)
                                .color(p.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(6),
                                p.hover,
                            );
                        }
                        let close_color = if resp.hovered() {
                            p.strong
                        } else {
                            p.muted
                        };
                        let close_icon_rect =
                            egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0));
                        draw_lucide_icon(ui.painter(), close_icon_rect, LucideIcon::X, close_color);
                        if resp.clicked() {
                            close_modal = true;
                        }
                    });
                });

                if let Some(modal) = &mut self.edit_modal {
                    ui.add_space(12.0);
                    field_caption(ui, "PATH");
                    info_pill(ui, &modal.path);
                    ui.add_space(6.0);
                    field_caption(ui, "KEY");
                    let key = if modal.key_label.is_empty() {
                        format!("({})", tr(ui.ctx(), "root"))
                    } else {
                        modal.key_label.clone()
                    };
                    info_pill(ui, &key);

                    ui.add_space(8.0);
                    field_caption(ui, "TYPE");
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let choices = [
                            (ValueTypeTag::String, "String"),
                            (ValueTypeTag::Number, "Number"),
                            (ValueTypeTag::Bool, "Bool"),
                            (ValueTypeTag::Null, "Null"),
                            (ValueTypeTag::RawJson, "JSON"),
                        ];
                        for (tag, label) in choices {
                            if type_chip(ui, label, modal.target_type == tag).clicked()
                                && modal.target_type != tag
                            {
                                modal.target_type = tag;
                                modal.error = None;
                                if tag == ValueTypeTag::Bool
                                    && modal.input_text != "true"
                                    && modal.input_text != "false"
                                {
                                    modal.input_text = "true".to_string();
                                }
                            }
                        }
                    });

                    ui.add_space(8.0);
                    field_caption(ui, "VALUE");
                    let has_error = modal.error.is_some();
                    let field_stroke = if has_error {
                        egui::Stroke::new(1.0, p.danger)
                    } else {
                        egui::Stroke::new(1.0, p.border)
                    };
                    let field = egui::Frame::default()
                        .fill(p.subtle)
                        .stroke(field_stroke)
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 8));

                    field.show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match modal.target_type {
                            ValueTypeTag::Bool => {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    if type_chip(ui, "true", modal.input_text == "true").clicked() {
                                        modal.input_text = "true".to_string();
                                        modal.error = None;
                                    }
                                    if type_chip(ui, "false", modal.input_text == "false").clicked()
                                    {
                                        modal.input_text = "false".to_string();
                                        modal.error = None;
                                    }
                                });
                            }
                            ValueTypeTag::Null => {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(tr(ui.ctx(), "This value will be set to null"))
                                            .size(13.0)
                                            .color(p.muted),
                                    )
                                    .selectable(false),
                                );
                            }
                            _ => {
                                let text_edit = if modal.target_type == ValueTypeTag::RawJson {
                                    egui::TextEdit::multiline(&mut modal.input_text)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(7)
                                        .frame(false)
                                } else {
                                    egui::TextEdit::singleline(&mut modal.input_text)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .frame(false)
                                };
                                let resp = ui.add(text_edit);
                                if !resp.has_focus() {
                                    resp.request_focus();
                                }
                                if resp.changed() {
                                    modal.error = None;
                                }
                                if modal.target_type != ValueTypeTag::RawJson
                                    && resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    save_modal = true;
                                }
                            }
                        }
                    });

                    if let Some(err) = &modal.error {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(err)
                                .size(12.0)
                                .color(p.danger),
                        );
                    }

                    ui.add_space(4.0);
                    let hint = if modal.target_type == ValueTypeTag::RawJson {
                        "Cmd+Enter to save   Esc to cancel"
                    } else {
                        "Enter to save   Esc to cancel"
                    };
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), hint))
                            .size(11.0)
                            .color(p.faint),
                    );

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if modal_button(ui, "Save", true).clicked() {
                                save_modal = true;
                            }
                            if modal_button(ui, "Cancel", false).clicked() {
                                close_modal = true;
                            }
                        });
                    });
                }
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close_modal = true;
        }
        if ctx.input(|i| {
            i.key_pressed(egui::Key::Enter)
                && (i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl)
        }) {
            save_modal = true;
        }

        if save_modal {
            self.commit_edit_modal();
        } else if close_modal {
            self.edit_modal = None;
        }
    }
}

impl JsonTab {
    fn render_editor_toolbar(&mut self, ui: &mut egui::Ui) {
        if btn_format(ui).clicked() {
            self.format_json(ui.ctx());
        }
        if btn_sort(ui, self.sort_mode).clicked() {
            self.sort_mode = match self.sort_mode {
                SortMode::None => SortMode::Asc,
                SortMode::Asc => SortMode::Desc,
                SortMode::Desc => SortMode::None,
            };
            self.apply_sort();
        }
        if btn_undo(ui, self.can_undo()).clicked() {
            self.undo_edit();
        }
        if btn_redo(ui, self.can_redo()).clicked() {
            self.redo_edit();
        }
        if btn_compress(ui).clicked() {
            self.compress_json();
        }
        if btn_simplify(ui, self.parsed.is_some()).clicked() {
            self.simplify_json();
        }
        if btn_copy(ui).clicked() {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(self.input.clone());
            }
        }
        if btn_clear(ui).clicked() {
            self.clear_document();
        }
    }

    fn render_editor_card(
        &mut self,
        ui: &mut egui::Ui,
        card_rect: egui::Rect,
        colors: ThemeColors,
        diff: Option<(&[usize], egui::Color32)>,
    ) {
        let p = colors.ui;
        ui.set_clip_rect(ui.clip_rect().intersect(card_rect));
        self.editor_card_rect = card_rect;
        if self.editor_map.is_empty() {
            self.rebuild_editor_view();
        }
        self.poll_request_loading(ui.ctx());
        let card_frame = egui::Frame::default()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(8, 0));

        card_frame.show(ui, |ui| {
            // Syntax Error Card (placed inside card to preserve card top alignment)
            if let Some(err) = &self.parse_error {
                if !self.input.trim().is_empty() {
                    let err_frame = egui::Frame::default()
                        .fill(p.danger_bg)
                        .stroke(egui::Stroke::new(
                            1.0,
                            p.danger_border,
                        ))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(8, 4));

                    err_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "⚠ {} (Line {}, Col {}): {}",
                                    tr(ui.ctx(), "Syntax Error"),
                                    err.line, err.column, err.message
                                ))
                                .color(p.danger)
                                .small()
                                .strong(),
                            );
                        });
                    });
                    ui.add_space(4.0);
                }
            }

            // Digits follow the source, so folding does not make the gutter jump.
            let source_line_count = self.input.split('\n').count().max(1);
            let digits = source_line_count.to_string().len().max(2);
            let fold_w = 16.0;

            // Relaxed line height (19.5pt) and letter spacing (0.15pt).
            // Use the galley's pixel-rounded row step so the gutter does not drift
            // away from the text as the document gets long.
            let requested_line_h = 19.5;
            let font_id = egui::FontId::monospace(13.0);
            let line_height_val = editor_row_height(ui, &font_id, requested_line_h);

            let measured_content_w = std::rc::Rc::new(std::cell::Cell::new(None));
            let last_galley = std::rc::Rc::new(std::cell::RefCell::new(None));
            let mcw = measured_content_w.clone();
            let lg = last_galley.clone();
            let layout_font_id = font_id.clone();
            let mut code_layouter = move |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                let job = highlight_json(text, layout_font_id.clone(), line_height_val, colors);
                let galley = ui.fonts(|f| f.layout_job(job));
                mcw.set(Some(galley.rect.width()));
                *lg.borrow_mut() = Some(galley.clone());
                galley
            };

            let digit_w = ui.fonts(|f| f.glyph_width(&font_id, '0')).max(7.2);
            let gutter_width = (digits as f32 * digit_w + 12.0).max(28.0);
            let visible_editor_w = (card_rect.width() - gutter_width - fold_w - 22.0).max(100.0);
            let fold_marks: Vec<Option<usize>> = {
                let rows = self.editor_map.len().max(1);
                (0..rows)
                    .map(|i| {
                        self.editor_map.get(i).and_then(|line| {
                            self.editor_spans
                                .iter()
                                .any(|span| span.start_line == line.source_start)
                                .then_some(line.source_start)
                        })
                    })
                    .collect()
            };
            let folded_now = self.editor_folds.clone();

            // Content width: use measured width if available, with approx fallback for first frame
            let approx_char_w = digit_w;
            let approx_w = self
                .editor_view
                .lines()
                .map(|l| {
                    let mut w = 0.0;
                    for c in l.chars() {
                        w += if c.is_ascii() {
                            approx_char_w
                        } else {
                            approx_char_w * 1.8
                        };
                    }
                    w
                })
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0);
            let content_w = self.editor_content_width.max(approx_w);
            let max_content_w = if content_w > 0.0 {
                content_w + 30.0
            } else {
                visible_editor_w
            };
            let text_exceeds_width = max_content_w > visible_editor_w + 4.0;
            let max_scroll_x = if text_exceeds_width {
                max_content_w - visible_editor_w
            } else {
                0.0
            };

            // Track horizontal scrolling from mouse wheel or Shift+scroll,
            // while explicitly disabling touchpad horizontal scrolling.
            let (manual_scroll_x, is_over_editor) = ui.input(|i| {
                let over = i
                    .pointer
                    .hover_pos()
                    .map_or(false, |p| card_rect.contains(p));
                let mut dx = 0.0;
                if over {
                    for event in &i.events {
                        if let egui::Event::MouseWheel {
                            unit,
                            delta,
                            modifiers,
                        } = event
                        {
                            match unit {
                                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                                    let factor = if *unit == egui::MouseWheelUnit::Line {
                                        40.0
                                    } else {
                                        200.0
                                    };
                                    if modifiers.shift {
                                        dx += (delta.x + delta.y) * factor;
                                    } else {
                                        dx += delta.x * factor;
                                    }
                                }
                                egui::MouseWheelUnit::Point => {
                                    // Touchpad: disable horizontal scrolling unless Shift is explicitly held
                                    if modifiers.shift {
                                        dx += delta.x + delta.y;
                                    }
                                }
                            }
                        }
                    }
                }
                (dx, over)
            });

            let do_reset = self.reset_editor_scroll;
            if do_reset || !text_exceeds_width {
                self.editor_scroll_x = 0.0;
                self.reset_editor_scroll = false;
            } else if is_over_editor && manual_scroll_x != 0.0 {
                self.editor_scroll_x =
                    (self.editor_scroll_x - manual_scroll_x).clamp(0.0, max_scroll_x);
            } else {
                self.editor_scroll_x = self.editor_scroll_x.clamp(0.0, max_scroll_x);
            }

            let editor_scroll_id = self.id;
            // Keep the full horizontal track inside the clipped pane.
            let scrollbar_h = if text_exceeds_width {
                10.0 + ui.spacing().item_spacing.y
            } else {
                0.0
            };
            let available_h = ui.available_height();
            let sa_h = (available_h - scrollbar_h).max(30.0);

            let mut vert_sa = egui::ScrollArea::vertical()
                .id_salt(("editor_vert_scroll", editor_scroll_id))
                .drag_to_scroll(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false; 2])
                .max_height(sa_h);

            let sync_scroll = std::mem::take(&mut self.editor_sync_scroll);
            if do_reset {
                vert_sa = vert_sa.vertical_scroll_offset(0.0);
            } else if sync_scroll != 0.0 {
                // ScrollArea clamps the offset to the content height.
                vert_sa = vert_sa.vertical_scroll_offset(self.editor_scroll_y - sync_scroll);
            }

            let vert_out = vert_sa.show(ui, |ui| {
                if let Some((lines, color)) = diff {
                    // Glyphs sit at the top of each row with the extra line spacing
                    // below them, so shift the bars up to center them on the text.
                    let font_h = ui.fonts(|f| f.row_height(&font_id));
                    let top = ui.cursor().top() - ((line_height_val - font_h) * 0.5).max(0.0);
                    // Only paint visible rows, including folded rows that contain a change.
                    let first = ((ui.clip_rect().top() - top) / line_height_val)
                        .floor()
                        .max(0.0) as usize;
                    let end = (((ui.clip_rect().bottom() - top) / line_height_val)
                        .ceil()
                        .max(0.0) as usize)
                        .min(self.editor_map.len());
                    for row in first..end {
                        let line = self.editor_map[row];
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(card_rect.left() + 8.0, top + row as f32 * line_height_val),
                            egui::vec2(card_rect.width() - 16.0, line_height_val),
                        );
                        let at = lines.partition_point(|&changed| changed < line.source_start);
                        if lines
                            .get(at)
                            .is_some_and(|&changed| changed <= line.source_end)
                        {
                            ui.painter()
                                .rect_filled(rect, egui::CornerRadius::ZERO, color);
                        }
                    }
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    let mut clicked_fold = None;
                    let fold_h = fold_marks.len() as f32 * line_height_val;
                    ui.allocate_ui_with_layout(
                        egui::vec2(fold_w, fold_h),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for mark in &fold_marks {
                                let sense = if mark.is_some() {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                };
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(fold_w, line_height_val),
                                    sense,
                                );
                                let Some(start) = mark else {
                                    continue;
                                };
                                let collapsed = folded_now.contains(start);
                                let color = if resp.hovered() {
                                    p.strong
                                } else {
                                    p.faint
                                };
                                draw_fold_chevron(ui.painter(), rect.center(), collapsed, color);
                                let tip = if collapsed { tr(ui.ctx(), "Expand") } else { tr(ui.ctx(), "Collapse") };
                                if resp.clicked() {
                                    clicked_fold = Some(*start);
                                }
                                resp.on_hover_text(tip)
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                        },
                    );
                    if let Some(start) = clicked_fold {
                        self.toggle_editor_fold(start);
                    }

                    let visible_lines = self.editor_map.len().max(1);
                    let mut line_numbers_str = String::with_capacity(visible_lines * 5);
                    for line in &self.editor_map {
                        use std::fmt::Write;
                        let _ = writeln!(
                            &mut line_numbers_str,
                            "{:>width$}",
                            line.source_start + 1,
                            width = digits
                        );
                    }
                    if line_numbers_str.ends_with('\n') {
                        line_numbers_str.pop();
                    }

                    let gutter_h = visible_lines as f32 * line_height_val;
                    ui.allocate_ui_with_layout(
                        egui::vec2(gutter_width, gutter_h),
                        egui::Layout::top_down(egui::Align::RIGHT),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&line_numbers_str)
                                        .font(font_id.clone())
                                        .line_height(Some(line_height_val))
                                        .color(p.muted),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .selectable(false),
                            );
                        },
                    );

                    let sep_x = ui.cursor().min.x - 1.0;
                    let sep_h = (visible_lines as f32 * line_height_val).max(card_rect.height());
                    ui.painter().vline(
                        sep_x,
                        ui.cursor().min.y..=ui.cursor().min.y + sep_h,
                        egui::Stroke::new(1.0, p.border),
                    );

                    let horiz_offset = self.editor_scroll_x;
                    egui::ScrollArea::horizontal()
                        .id_salt(("editor_horiz_scroll", editor_scroll_id))
                        .horizontal_scroll_offset(horiz_offset)
                        .drag_to_scroll(false)
                        .enable_scrolling(false)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, true])
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.visuals_mut().text_cursor.stroke = egui::Stroke::NONE;

                            let min_rows = ((card_rect.height() / line_height_val).ceil() as usize)
                                .clamp(6, 200);
                            let editor_id = json_editor_id(editor_scroll_id);
                            let text_edit = egui::TextEdit::multiline(&mut self.editor_view)
                                .id(editor_id)
                                .layouter(&mut code_layouter)
                                .desired_width(max_content_w.max(visible_editor_w))
                                .desired_rows(min_rows)
                                .margin(egui::Margin::ZERO)
                                .frame(false)
                                .code_editor();

                            let resp = ui.add(text_edit);
                            if resp.has_focus() || resp.clicked() || resp.dragged() {
                                self.editor_editing = true;
                            }
                            if resp.changed() {
                                self.commit_editor_view(false);
                            }

                            let now = ui.input(|i| i.time);
                            if resp.changed()
                                || resp.clicked()
                                || resp.dragged()
                                || (resp.has_focus() && ui.input(|i| !i.events.is_empty()))
                            {
                                self.editor_last_interaction = now;
                            }

                            // Custom refined editor cursor: reduced height (13.5px) and softer/lighter blue
                            let has_focus = ui.memory(|m| m.has_focus(editor_id));
                            let is_window_focused = ui.input(|i| i.focused);
                            if has_focus && is_window_focused {
                                let state_opt = egui::text_edit::TextEditState::load(ui.ctx(), editor_id);
                                let galley_opt = last_galley.borrow().clone();
                                if let (Some(state), Some(galley)) = (state_opt, galley_opt) {
                                    if let Some(cursor_range) = state.cursor.range(&galley) {
                                        let time_since_interaction = now - self.editor_last_interaction;
                                        let on_duration = 0.55_f32;
                                        let off_duration = 0.45_f32;
                                        let total = on_duration + off_duration;
                                        let time_in_cycle =
                                            (time_since_interaction % (total as f64)) as f32;
                                        let visible = time_in_cycle < on_duration;
                                        let wake_in = if visible {
                                            on_duration - time_in_cycle
                                        } else {
                                            total - time_in_cycle
                                        };
                                        ui.ctx().request_repaint_after_secs(wake_in);

                                        if visible {
                                            let cursor = cursor_range.primary;
                                            let cursor_pos = galley.pos_from_cursor(&cursor);
                                            let row_idx = cursor.rcursor.row;
                                            let baseline = galley
                                                .rows
                                                .get(row_idx)
                                                .and_then(|r| r.glyphs.first().map(|g| g.pos.y))
                                                .unwrap_or(cursor_pos.min.y + 13.0);
                                            let text_center_y = baseline - 5.0;

                                            let center_x = resp.rect.min.x + cursor_pos.min.x;
                                            let center_y = resp.rect.min.y + text_center_y;
                                            let cursor_rect = egui::Rect::from_center_size(
                                                egui::pos2(center_x, center_y),
                                                egui::vec2(1.2, 14.0),
                                            );
                                            ui.painter().rect_filled(
                                                cursor_rect,
                                                egui::CornerRadius::same(1),
                                                p.blue,
                                            );
                                        }
                                    }
                                }
                            }

                            if resp.double_clicked() {
                                let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                                let galley_opt = last_galley.borrow().clone();
                                let char_idx = if let (Some(galley), Some(pos)) = (&galley_opt, pointer_pos) {
                                    let rel_pos = pos - resp.rect.min;
                                    galley.cursor_from_pos(rel_pos).ccursor.index
                                } else {
                                    let state = egui::text_edit::TextEditState::load(ui.ctx(), editor_id);
                                    state.and_then(|s| s.cursor.char_range()).map(|r| r.primary.index).unwrap_or(0)
                                };

                                let (sel_start, sel_end) = find_double_click_selection(&self.editor_view, char_idx);

                                let mut state = egui::text_edit::TextEditState::load(ui.ctx(), editor_id).unwrap_or_default();
                                if let Some(galley) = &galley_opt {
                                    state.cursor.set_range(Some(egui::text_selection::CursorRange {
                                        primary: galley.from_ccursor(egui::text::CCursor::new(sel_end)),
                                        secondary: galley.from_ccursor(egui::text::CCursor::new(sel_start)),
                                    }));
                                } else {
                                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                        egui::text::CCursor::new(sel_start),
                                        egui::text::CCursor::new(sel_end),
                                    )));
                                }
                                state.store(ui.ctx(), editor_id);
                                ui.ctx().request_repaint();
                            }
                        });
                });
            });
            self.editor_scroll_y = vert_out.state.offset.y;
            self.editor_scroll_max_y =
                (vert_out.content_size.y - vert_out.inner_rect.height()).max(0.0);

            if let Some(w) = measured_content_w.get() {
                self.editor_content_width = w;
            }

            if text_exceeds_width {
                ui.add_space(2.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    let left_gutter_space = fold_w + gutter_width + 8.0;
                    ui.add_space(left_gutter_space);

                    let track_w = (ui.available_width() - 4.0).max(20.0);
                    let (track_rect, scroll_resp) = ui.allocate_exact_size(
                        egui::vec2(track_w, 8.0),
                        egui::Sense::click_and_drag(),
                    );
                    let scroll_resp = scroll_resp.on_hover_cursor(egui::CursorIcon::Default);

                    let max_scroll_x = (max_content_w - visible_editor_w).max(1.0);
                    let thumb_w = ((visible_editor_w / max_content_w) * track_w).clamp(24.0, track_w);
                    let scrollable_track = (track_w - thumb_w).max(1.0);

                    if scroll_resp.dragged() {
                        let drag_x = scroll_resp.drag_delta().x;
                        let delta_scroll = drag_x * (max_scroll_x / scrollable_track);
                        self.editor_scroll_x = (self.editor_scroll_x + delta_scroll).clamp(0.0, max_scroll_x);
                    } else if scroll_resp.clicked() {
                        if let Some(pos) = scroll_resp.interact_pointer_pos() {
                            let target_ratio = ((pos.x - track_rect.min.x - thumb_w * 0.5) / scrollable_track).clamp(0.0, 1.0);
                            self.editor_scroll_x = (target_ratio * max_scroll_x).clamp(0.0, max_scroll_x);
                        }
                    }

                    let scroll_ratio = (self.editor_scroll_x / max_scroll_x).clamp(0.0, 1.0);
                    let thumb_x = track_rect.min.x + scroll_ratio * scrollable_track;
                    let thumb_rect = egui::Rect::from_min_size(
                        egui::pos2(thumb_x, track_rect.min.y + 1.0),
                        egui::vec2(thumb_w, track_rect.height() - 2.0),
                    );

                    ui.painter().rect_filled(
                        track_rect,
                        egui::CornerRadius::same(4),
                        p.subtle,
                    );

                    let thumb_color = if scroll_resp.is_pointer_button_down_on() {
                        p.strong
                    } else if scroll_resp.hovered() {
                        p.muted
                    } else {
                        p.border
                    };
                    ui.painter().rect_filled(
                        thumb_rect,
                        egui::CornerRadius::same(3),
                        thumb_color,
                    );
                });
            }
        });

        if let Some(loading) = &self.request_loading {
            let center = card_rect.center();
            let overlay_size = egui::vec2(280.0, 110.0);
            let overlay_rect = egui::Rect::from_center_size(center, overlay_size);

            ui.painter().rect_filled(
                card_rect,
                egui::CornerRadius::ZERO,
                egui::Color32::from_black_alpha(if ui.visuals().dark_mode { 120 } else { 40 }),
            );

            let card_frame = egui::Frame::default()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(10))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 20,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(if ui.visuals().dark_mode { 80 } else { 30 }),
                })
                .inner_margin(egui::Margin::symmetric(20, 16));

            ui.allocate_new_ui(
                egui::UiBuilder::default().max_rect(overlay_rect),
                |ui| {
                    card_frame.show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(22.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(tr(ui.ctx(), "Loading JSON from request..."))
                                    .size(13.0)
                                    .strong()
                                    .color(p.strong),
                            );
                            ui.add_space(2.0);
                            let target = if loading.target_url.len() > 34 {
                                format!("{}...", &loading.target_url[..31])
                            } else {
                                loading.target_url.clone()
                            };
                            ui.label(
                                egui::RichText::new(target)
                                    .size(11.0)
                                    .monospace()
                                    .color(p.muted),
                            );
                        });
                    });
                },
            );

            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.request_loading = None;
            }
        }
    }

    fn poll_request_loading(&mut self, ctx: &egui::Context) {
        let Some(loading) = &self.request_loading else {
            return;
        };

        match loading.receiver.try_recv() {
            Ok(Ok(json_str)) => {
                let for_paste = loading.for_editor_paste;
                self.request_loading = None;
                if for_paste {
                    self.paste_into_editor(ctx, &json_str);
                } else {
                    self.replace_document(json_str);
                }
            }
            Ok(Err(_)) => {
                let raw = loading.raw_text.clone();
                let for_paste = loading.for_editor_paste;
                self.request_loading = None;
                if for_paste {
                    self.paste_into_editor(ctx, &raw);
                } else {
                    self.replace_document(raw);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.request_loading = None;
            }
        }
    }
}

impl JsonViewerApp {
    fn render_tree_card(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect, colors: ThemeColors) {
        let p = colors.ui;
        ui.set_clip_rect(ui.clip_rect().intersect(card_rect));
        let card_frame = egui::Frame::default()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(8, 0));

        card_frame.show(ui, |ui| {
            if self.parsed.is_none() {
                // Empty data: no text prompts, allocate full space to stretch top-to-bottom
                ui.allocate_space(ui.available_size());
                return;
            }

            let mut action: Option<TreeAction> = None;

            let parsed = self.parsed.clone().unwrap();
            let search_query = self.search_query.clone();
            let search_matches = self.search_matches.clone();
            let cur_match_path = if !self.search_matches.is_empty() {
                Some(self.search_matches[self.search_index].clone())
            } else {
                None
            };
            let selected_path = self.selected_path.clone();
            let reveal_match = self.reveal_search_match;
            if reveal_match {
                if let Some(path) = &cur_match_path {
                    let mut ancestor = path.clone();
                    while let Some(pos) = ancestor.rfind(|c| c == '.' || c == '[') {
                        ancestor.truncate(pos);
                        self.collapsed.remove(&ancestor);
                    }
                }
                // Reset manual horizontal scrolling so the matched row's key is visible.
                self.tree_scroll_x = 0.0;
            }

            // Track horizontal scrolling from mouse wheel or Shift+scroll,
            // while explicitly disabling touchpad horizontal scrolling.
            let (manual_scroll_x, is_over_tree) = ui.input(|i| {
                let over = i
                    .pointer
                    .hover_pos()
                    .map_or(false, |p| card_rect.contains(p));
                let mut dx = 0.0;
                if over {
                    for event in &i.events {
                        if let egui::Event::MouseWheel {
                            unit,
                            delta,
                            modifiers,
                        } = event
                        {
                            match unit {
                                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                                    let factor = if *unit == egui::MouseWheelUnit::Line {
                                        40.0
                                    } else {
                                        200.0
                                    };
                                    if modifiers.shift {
                                        dx += (delta.x + delta.y) * factor;
                                    } else {
                                        dx += delta.x * factor;
                                    }
                                }
                                egui::MouseWheelUnit::Point => {
                                    // Touchpad: disable horizontal scrolling unless Shift is explicitly held
                                    if modifiers.shift {
                                        dx += delta.x + delta.y;
                                    }
                                }
                            }
                        }
                    }
                }
                (dx, over)
            });

            let visible_w = (card_rect.width() - 16.0).max(50.0);
            let text_exceeds_width = self.tree_content_width > visible_w + 2.0;
            let scroll_content_w = if text_exceeds_width {
                self.tree_content_width + 16.0
            } else {
                visible_w
            };
            let max_scroll_x = if text_exceeds_width {
                scroll_content_w - visible_w
            } else {
                0.0
            };

            if !text_exceeds_width {
                self.tree_scroll_x = 0.0;
            } else if is_over_tree && manual_scroll_x != 0.0 {
                self.tree_scroll_x =
                    (self.tree_scroll_x - manual_scroll_x).clamp(0.0, max_scroll_x);
            } else {
                self.tree_scroll_x = self.tree_scroll_x.clamp(0.0, max_scroll_x);
            }

            // Keep the full horizontal track inside the clipped pane.
            let scrollbar_h = if text_exceeds_width {
                10.0 + ui.spacing().item_spacing.y
            } else {
                0.0
            };
            let available_h = ui.available_height();
            let sa_h = (available_h - scrollbar_h).max(30.0);

            let tree_scroll_id = self.id;
            let mut max_row_w = 0.0f32;
            let mut current_match_rect = None;

            egui::ScrollArea::vertical()
                .id_salt(("tree_vert_scroll", tree_scroll_id))
                .drag_to_scroll(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false; 2])
                .max_height(sa_h)
                .show(ui, |ui| {
                    let horiz_offset = self.tree_scroll_x;
                    egui::ScrollArea::horizontal()
                        .id_salt(("tree_horiz_scroll", tree_scroll_id))
                        .horizontal_scroll_offset(horiz_offset)
                        .drag_to_scroll(false)
                        .enable_scrolling(false)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if text_exceeds_width {
                                ui.set_min_width(scroll_content_w);
                            }
                            render_tree_node(
                                ui,
                                &parsed,
                                "$",
                                None,
                                0,
                                &mut self.collapsed,
                                &search_query,
                                &search_matches,
                                cur_match_path.as_deref(),
                                selected_path.as_deref(),
                                colors,
                                &mut action,
                                &mut max_row_w,
                                &mut current_match_rect,
                            );
                        });
                    // Issue the request outside the horizontal area so the vertical area handles it.
                    if reveal_match {
                        if let Some(rect) = current_match_rect {
                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                            self.reveal_search_match = false;
                        }
                    }
                });

            self.tree_content_width = max_row_w;

            if text_exceeds_width {
                ui.add_space(2.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    let track_w = (ui.available_width() - 4.0).max(20.0);
                    let (track_rect, scroll_resp) = ui.allocate_exact_size(
                        egui::vec2(track_w, 8.0),
                        egui::Sense::click_and_drag(),
                    );
                    let scroll_resp = scroll_resp.on_hover_cursor(egui::CursorIcon::Default);

                    let thumb_w = ((visible_w / scroll_content_w) * track_w).clamp(24.0, track_w);
                    let scrollable_track = (track_w - thumb_w).max(1.0);

                    if scroll_resp.dragged() {
                        let drag_x = scroll_resp.drag_delta().x;
                        let delta_scroll = drag_x * (max_scroll_x / scrollable_track);
                        self.tree_scroll_x =
                            (self.tree_scroll_x + delta_scroll).clamp(0.0, max_scroll_x);
                    } else if scroll_resp.clicked() {
                        if let Some(pos) = scroll_resp.interact_pointer_pos() {
                            let target_ratio = ((pos.x - track_rect.min.x - thumb_w * 0.5)
                                / scrollable_track)
                                .clamp(0.0, 1.0);
                            self.tree_scroll_x =
                                (target_ratio * max_scroll_x).clamp(0.0, max_scroll_x);
                        }
                    }

                    let scroll_ratio = (self.tree_scroll_x / max_scroll_x).clamp(0.0, 1.0);
                    let thumb_x = track_rect.min.x + scroll_ratio * scrollable_track;
                    let thumb_rect = egui::Rect::from_min_size(
                        egui::pos2(thumb_x, track_rect.min.y + 1.0),
                        egui::vec2(thumb_w, track_rect.height() - 2.0),
                    );

                    ui.painter().rect_filled(
                        track_rect,
                        egui::CornerRadius::same(4),
                        p.subtle,
                    );

                    let thumb_color = if scroll_resp.is_pointer_button_down_on() {
                        p.strong
                    } else if scroll_resp.hovered() {
                        p.muted
                    } else {
                        p.border
                    };
                    ui.painter().rect_filled(
                        thumb_rect,
                        egui::CornerRadius::same(3),
                        thumb_color,
                    );
                });
            }

            if let Some(act) = action {
                match act {
                    TreeAction::Select(path) => {
                        self.selected_path = Some(path);
                    }
                    TreeAction::CopyText(txt) => {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(txt);
                        }
                    }
                    TreeAction::StartEdit(path, key, val) => {
                        self.open_edit_modal(&path, &key, &val);
                    }
                    TreeAction::DeleteNode(path) => {
                        self.delete_node_at(&path);
                    }
                    TreeAction::ExpandSubtree(path) => {
                        self.collapsed.retain(|p| !p.starts_with(&path));
                    }
                    TreeAction::CollapseSubtree(path, val) => {
                        collect_all_expandable_paths(&val, &path, &mut self.collapsed);
                    }
                    TreeAction::DownloadImage(url) => {
                        crate::image_preview::save_image_dialog(&url);
                    }
                }
            }
        });
    }
}

fn editor_row_height(ui: &egui::Ui, font_id: &egui::FontId, requested: f32) -> f32 {
    ui.fonts(|fonts| {
        let mut job = egui::text::LayoutJob::default();
        job.break_on_newline = true;
        job.wrap.max_width = f32::INFINITY;
        job.append(
            "0\n0",
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                line_height: Some(requested),
                extra_letter_spacing: 0.15,
                ..Default::default()
            },
        );
        let galley = fonts.layout_job(job);
        if galley.rows.len() >= 2 {
            galley.rows[1].rect.min.y - galley.rows[0].rect.min.y
        } else {
            galley
                .rows
                .first()
                .map(|row| row.rect.height())
                .unwrap_or(requested)
        }
    })
}

/// Computes the selection range when double-clicking text in the editor.
/// Prioritizes selecting the content inside quotes (without the quotes),
/// never spans across line boundaries, and handles numbers/identifiers cleanly.
fn find_double_click_selection(text: &str, click_char_idx: usize) -> (usize, usize) {
    let mut current_char_idx = 0;
    let mut line_start_char_idx = 0;
    let mut line_start_byte_idx = 0;
    let mut line_end_byte_idx = text.len();

    for (b_idx, c) in text.char_indices() {
        if current_char_idx <= click_char_idx && c == '\n' {
            line_start_char_idx = current_char_idx + 1;
            line_start_byte_idx = b_idx + 1;
        }
        if current_char_idx >= click_char_idx && (c == '\n' || c == '\r') {
            line_end_byte_idx = b_idx;
            break;
        }
        current_char_idx += 1;
    }

    let line = &text[line_start_byte_idx..line_end_byte_idx];
    let line_chars: Vec<char> = line.chars().collect();
    if line_chars.is_empty() {
        return (line_start_char_idx, line_start_char_idx);
    }

    let local_click = (click_char_idx.saturating_sub(line_start_char_idx)).min(line_chars.len());

    // Scan for quoted strings on this line
    let mut quotes = Vec::new();
    let mut i = 0;
    while i < line_chars.len() {
        if line_chars[i] == '"' {
            let open_q = i;
            let mut close_q = None;
            let mut j = i + 1;
            while j < line_chars.len() {
                if line_chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                if line_chars[j] == '"' {
                    close_q = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(c) = close_q {
                quotes.push((open_q, c));
                i = c + 1;
            } else {
                quotes.push((open_q, line_chars.len()));
                break;
            }
        } else {
            i += 1;
        }
    }

    // Check if the click falls within or right on the quotes of a quoted string
    for (open_q, close_q) in quotes {
        if local_click >= open_q && local_click <= close_q {
            let sel_s = open_q + 1;
            let sel_e = close_q;
            if sel_s > sel_e {
                // Empty string: "" -> cursor between quotes
                return (line_start_char_idx + open_q + 1, line_start_char_idx + open_q + 1);
            }
            return (line_start_char_idx + sel_s, line_start_char_idx + sel_e);
        }
        if local_click == close_q + 1 {
            let next_char = line_chars.get(local_click).copied();
            if next_char.is_none() || next_char == Some(' ') || next_char == Some('\t') {
                let sel_s = open_q + 1;
                let sel_e = close_q;
                if sel_s > sel_e {
                    return (line_start_char_idx + open_q + 1, line_start_char_idx + open_q + 1);
                }
                return (line_start_char_idx + sel_s, line_start_char_idx + sel_e);
            }
        }
    }

    // Outside quotes: select number, identifier/word, or single punctuation on this line
    let c_idx = local_click.min(line_chars.len().saturating_sub(1));
    let c = line_chars[c_idx];
    if c.is_ascii_digit() || (c == '-' && c_idx + 1 < line_chars.len() && line_chars[c_idx + 1].is_ascii_digit()) {
        let mut s = c_idx;
        while s > 0 && (line_chars[s - 1].is_ascii_digit() || matches!(line_chars[s - 1], '.' | '-' | '+' | 'e' | 'E')) {
            s -= 1;
        }
        let mut e = c_idx + 1;
        while e < line_chars.len() && (line_chars[e].is_ascii_digit() || matches!(line_chars[e], '.' | '-' | '+' | 'e' | 'E')) {
            e += 1;
        }
        (line_start_char_idx + s, line_start_char_idx + e)
    } else if c.is_alphanumeric() || c == '_' || (!c.is_ascii() && !c.is_whitespace() && !c.is_control()) {
        let mut s = c_idx;
        while s > 0 && (line_chars[s - 1].is_alphanumeric() || line_chars[s - 1] == '_' || (!line_chars[s - 1].is_ascii() && !line_chars[s - 1].is_whitespace() && !line_chars[s - 1].is_control())) {
            s -= 1;
        }
        let mut e = c_idx + 1;
        while e < line_chars.len() && (line_chars[e].is_alphanumeric() || line_chars[e] == '_' || (!line_chars[e].is_ascii() && !line_chars[e].is_whitespace() && !line_chars[e].is_control())) {
            e += 1;
        }
        (line_start_char_idx + s, line_start_char_idx + e)
    } else if c.is_whitespace() {
        let mut s = c_idx;
        while s > 0 && line_chars[s - 1].is_whitespace() {
            s -= 1;
        }
        let mut e = c_idx + 1;
        while e < line_chars.len() && line_chars[e].is_whitespace() {
            e += 1;
        }
        (line_start_char_idx + s, line_start_char_idx + e)
    } else {
        (line_start_char_idx + c_idx, line_start_char_idx + (c_idx + 1).min(line_chars.len()))
    }
}

fn rail_add_button(ui: &mut egui::Ui) -> egui::Response {
    let p = UiColors::current(ui);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(6),
            p.hover,
        );
    }
    let color = if resp.hovered() {
        p.blue
    } else {
        p.text
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(14.0, 14.0),
    );
    draw_lucide_icon(ui.painter(), icon_rect, LucideIcon::Plus, color);
    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tr(ui.ctx(), "New Tab"),
        egui::FontId::proportional(13.0),
        color,
    );
    resp.on_hover_text(tr(ui.ctx(), "New Tab"))
}

fn tab_row(
    ui: &mut egui::Ui,
    index: usize,
    id: u64,
    name: &str,
    pinned: bool,
    comparison: bool,
    active: bool,
    reveal: bool,
    action: &mut RailAction,
) {
    let p = UiColors::current(ui);
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());
    if reveal {
        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
    }

    let bg = if active {
        p.blue_bg
    } else if resp.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(8), bg);
    }
    if active {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0, p.blue_border),
            egui::StrokeKind::Inside,
        );
    }
    let show_actions = active || resp.hovered();
    let icon = 14.0;
    let text_left = rect.left() + 8.0;
    let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
    let close_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 8.0 - icon * 0.5, rect.center().y),
        egui::vec2(icon, icon),
    )
    .expand(3.0);
    let star_rect = egui::Rect::from_center_size(
        egui::pos2(close_rect.center().x - icon - 8.0, rect.center().y),
        egui::vec2(icon, icon),
    )
    .expand(3.0);
    let name_right = if show_actions {
        star_rect.left() - 2.0
    } else {
        rect.right() - 8.0
    };
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(text_left, rect.top()),
        egui::pos2(name_right.max(text_left + 8.0), rect.bottom()),
    );
    let name_color = if comparison {
        p.purple
    } else if pinned {
        p.amber
    } else if active {
        p.strong
    } else {
        p.text
    };
    let over_pin = show_actions && pointer.is_some_and(|pos| star_rect.contains(pos));
    let over_close = show_actions && pointer.is_some_and(|pos| close_rect.contains(pos));
    if show_actions {
        let pin_color = if over_pin {
            p.strong
        } else {
            p.muted
        };
        let pin_icon = if pinned {
            LucideIcon::BookmarkOff
        } else {
            LucideIcon::Bookmark
        };
        draw_lucide_icon(
            ui.painter(),
            star_rect.shrink(3.0),
            pin_icon,
            pin_color,
        );
        let close_color = if over_close {
            p.danger
        } else {
            p.muted
        };
        draw_lucide_icon(
            ui.painter(),
            close_rect.shrink(3.0),
            LucideIcon::X,
            close_color,
        );
    }
    let mut name_ui = ui.new_child(
        egui::UiBuilder::default()
            .max_rect(name_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    name_ui.set_clip_rect(name_rect);
    let mut name_text = egui::RichText::new(name).size(13.0).color(name_color);
    if active {
        name_text = name_text.strong();
    }
    name_ui.add(
        egui::Label::new(name_text)
            .truncate()
            .selectable(false)
            .sense(egui::Sense::empty()),
    );

    if resp.clicked() || resp.double_clicked() {
        if over_pin {
            action.pin = Some(index);
        } else if over_close {
            action.close = Some(index);
        } else if resp.double_clicked() {
            action.start_rename = Some(id);
            action.activate = Some(index);
        } else if resp.clicked() {
            action.activate = Some(index);
        }
    }
    resp.context_menu(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 1.0);
        ui.set_width(112.0);
        if menu_item(ui, "Rename", LucideIcon::Pencil, false) {
            action.start_rename = Some(id);
        }
        if pinned {
            if menu_item(ui, "Unpin", LucideIcon::BookmarkOff, false) {
                action.pin = Some(index);
            }
        } else if menu_item(ui, "Pin", LucideIcon::Bookmark, false) {
            action.pin = Some(index);
        }
        menu_separator(ui);
        if menu_item(ui, "Close", LucideIcon::X, true) {
            action.close = Some(index);
        }
    });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
}
