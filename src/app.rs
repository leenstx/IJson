use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;
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
    collapse_beyond_depth, collect_all_expandable_paths, is_request_candidate, value_at_path, try_request_json, try_unescape_json,
    Indentation, ParseError, SortMode, TreeAction, ValueTypeTag,
};
use crate::tab::{
    local_stamp_now, next_compare_tab_name, next_day_tab_name, stamp_from_system_time,
    tab_from_value, EditorGalleyCache, JsonTab, TAB_RAIL_WIDTH,
};
use crate::i18n::{tr, Language};
use crate::keymap::{record_key, tip_with_shortcut, Action, Keymap, Recorded};
use crate::theme::{ThemeColors, ThemeMode, UiColors};
use crate::tree::{render_tree_node, TreeLabel, TREE_ROW_HEIGHT};
use crate::updater::{Updater, UpdateStatus, CURRENT_VERSION};
use crate::widgets::{
    btn_clear, mini_icon_button, btn_mode, toolbar_separator, btn_collapse_all, btn_compare, btn_compress, btn_copy, btn_expand_all, btn_format,
    btn_new_tab, btn_redo, btn_sidebar, segmented, settings_tab_bar, compact_button, btn_simplify, btn_sort, btn_undo, menu_item,
    menu_separator, modal_button, type_chip,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SettingsTab {
    #[default]
    General,
    Font,
    Shortcuts,
    About,
}

pub(crate) struct JsonViewerApp {
    tabs: Vec<JsonTab>,
    active: usize,
    next_id: u64,
    renaming: Option<TabRename>,
    reveal_active: bool,
    // Cmd+W also asks the window to close. Cancel that for a couple of frames.
    suppress_close_frames: u8,

    tab_rail_width: f32,
    sidebar_hidden: bool,
    keymap: Keymap,
    /// Settings panel state.
    settings_open: bool,
    settings_tab: SettingsTab,
    /// The action whose new shortcut is being recorded.
    keymap_recording: Option<Action>,
    /// Message after a rebind took a shortcut from another action.
    keymap_note: Option<String>,
    /// View mode: the right (selected node) card has focus instead of the tree.
    node_focused: bool,
    left_panel_width: f32,
    // False until the user drags the splitter. Then the saved width is restored.
    split_customized: bool,
    language: Language,
    theme_mode: ThemeMode,
    indentation: Indentation,
    fonts: AppFonts,
    updater: Updater,
    auto_check_updates: bool,
    update_viewed: bool,

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
            tabs: vec![JsonTab::empty(1, local_stamp_now(), Indentation::default())],
            active: 0,
            next_id: 2,
            renaming: None,
            reveal_active: false,
            suppress_close_frames: 0,
            tab_rail_width: TAB_RAIL_WIDTH,
            sidebar_hidden: false,
            keymap: Keymap::default(),
            settings_open: false,
            settings_tab: SettingsTab::default(),
            keymap_recording: None,
            keymap_note: None,
            node_focused: false,
            left_panel_width: 380.0,
            split_customized: false,
            language: Language::English,
            theme_mode: ThemeMode::Light,
            indentation: Indentation::default(),
            fonts: AppFonts::new(ctx),
            updater: Updater::new(),
            auto_check_updates: true,
            update_viewed: false,
            cache_dirty: false,
            last_cache_save: None,
            window_geom: read_window_geom(),
            window_pos_restored: false,
            window_settle_frames: 0,
        };
        app.load_cache();
        if app.auto_check_updates {
            app.updater.check(ctx.clone(), false);
        }
        // ⌘+ / ⌘- are ours (expand / collapse all), not egui's zoom.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        app.language.apply(ctx);
        app.theme_mode.apply(ctx);
        app.indentation.apply(ctx);
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
        if let Some(auto_check) = v.get("auto_check_updates").and_then(Value::as_bool) {
            self.auto_check_updates = auto_check;
        }
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
        if let Some(n) = v.get("indentation").and_then(Value::as_u64) {
            self.indentation = Indentation::from_spaces(n as usize);
            for tab in &mut self.tabs {
                tab.indentation = self.indentation;
            }
        }
        if let Some(width) = v.get("tab_rail_width").and_then(Value::as_f64) {
            if width.is_finite() {
                self.tab_rail_width = width.clamp(160.0, 400.0) as f32;
            }
        }
        self.sidebar_hidden = v
            .get("sidebar_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(keymap) = v.get("keymap") {
            self.keymap = Keymap::from_json(keymap);
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
                if let Some(comparison) = &mut item.tab.comparison {
                    max_id = max_id.max(comparison.right.id);
                    comparison.right.indentation = self.indentation;
                }
                item.tab.indentation = self.indentation;
                item.tab.finish_load(item.collapsed, item.search_index);
                item.tab
            })
            .collect();
        if self.tabs.is_empty() {
            let tab = JsonTab::empty(1, local_stamp_now(), self.indentation);
            self.tabs.push(tab);
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
            "sidebar_hidden": self.sidebar_hidden,
            "keymap": self.keymap.to_json(),
            "left_panel_width": self.left_panel_width,
            "split_customized": self.split_customized,
            "language": self.language.code(),
            "theme_mode": self.theme_mode.code(),
            "indentation": self.indentation.spaces(),
            "font_choice": self.fonts.available[self.fonts.selected].key,
            "auto_check_updates": self.auto_check_updates,
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
        let name = next_day_tab_name(&self.tabs);
        let mut tab = JsonTab::empty(id, name, self.indentation);

        let clipboard_text = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok())
            .filter(|text| !text.trim().is_empty());

        if let Some(text) = clipboard_text {
            if is_request_candidate(&text) {
                tab.start_request_loading(text, false, ctx);
            } else {
                tab.replace_document(text);
                if tab.parse_error.is_some() {
                    tab.set_edit_mode(ctx, true);
                }
            }
        }

        // New tabs are inserted at the top so they are visible immediately.
        self.tabs.insert(0, tab);
        self.active = 0;
        self.reveal_active = true;
        self.cache_dirty = true;
        ctx.request_repaint();
    }

    fn add_comparison_tab(&mut self, ctx: &egui::Context) {
        self.commit_rename();
        let source = &self.tabs[self.active];
        let input = source.input.clone();
        let name = next_compare_tab_name(&self.tabs);
        let previous = source.id;
        self.surrender_focus(ctx, previous);
        let id = self.alloc_id();
        let right_id = self.alloc_id();
        let mut tab = JsonTab::empty(id, name, self.indentation);
        // Keep an exact snapshot; later edits never change the source tab.
        tab.input = input;
        tab.reparse();
        tab.comparison = Some(Box::new(Comparison::new(JsonTab::empty(
            right_id,
            String::new(),
            self.indentation,
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

    /// Drag handle in the gap between the two cards.
    fn render_split_handle(&mut self, ui: &mut egui::Ui, cards_rect: egui::Rect) {
        let x = cards_rect.left() + self.left_panel_width;
        let handle_rect = egui::Rect::from_center_size(
            egui::pos2(x, cards_rect.center().y),
            egui::vec2(CARD_GAP + 4.0, cards_rect.height()),
        );
        let response = ui.interact(handle_rect, ui.id().with("split_handle"), egui::Sense::drag());
        if response.dragged() {
            self.split_customized = true;
            self.left_panel_width += response.drag_delta().x;
            self.cache_dirty = true;
        }
        if response.contains_pointer() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }

    fn render_comparison(
        &mut self,
        ui: &mut egui::Ui,
        cards_rect: egui::Rect,
        bar_rect: egui::Rect,
        bar_left: f32,
        colors: ThemeColors,
    ) -> bool {
        let total_w = cards_rect.width();
        if !self.split_customized {
            self.left_panel_width = total_w * 0.5;
        }
        // Both toolbars must fit their eight buttons at the minimum window size.
        self.left_panel_width = self
            .left_panel_width
            .clamp(240.0, (total_w - 240.0).max(240.0));
        let (left_card, right_card) = split_cards(cards_rect, self.left_panel_width);
        let pane_rect = cards_rect;
        let p = colors.ui;
        let right_active = self.tabs[self.active]
            .comparison
            .as_ref()
            .is_some_and(|c| c.right_active);
        let left_body =
            draw_card(ui, left_card, !right_active);
        let right_body = draw_card(ui, right_card, right_active);

        // Temporarily detach the right document to reuse the same editor for both sides.
        let tab = &mut self.tabs[self.active];
        let mut comparison = tab.comparison.take().unwrap();

        let mut open_comparison = false;
        for (i, (document, x, right)) in [
            (&mut *tab, left_card.left().max(bar_left), left_card.right()),
            (&mut comparison.right, right_card.left(), right_card.right()),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = egui::Rect::from_min_max(
                egui::pos2(x, bar_rect.top()),
                egui::pos2(right.max(x + 1.0), bar_rect.bottom()),
            );
            let mut toolbar = ui.new_child(
                egui::UiBuilder::default()
                    .id_salt(("editor_toolbar", document.id))
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            toolbar.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
            if document.render_editor_toolbar(&mut toolbar) {
                open_comparison = true;
            }
            document.poll_request_loading(ui.ctx());

            let empty_left = toolbar.min_rect().right() + 4.0;
            if right > empty_left + 8.0 {
                let drag_rect = egui::Rect::from_min_max(
                    egui::pos2(empty_left, bar_rect.top()),
                    egui::pos2(right, bar_rect.bottom()),
                );
                window_drag_area(ui, drag_rect, if i == 0 { "comp_drag_left" } else { "comp_drag_right" });
            }
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

        for (document, rect, lines, color) in [
            (&mut *tab, left_body, comparison.removed.as_slice(), p.danger_bg),
            (&mut comparison.right, right_body, comparison.added.as_slice(), p.added),
        ] {
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
            document.draw_copy_notice(&editor_ui, rect, p);
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
        self.render_split_handle(ui, cards_rect);
        open_comparison
    }

    /// Theme toggle (left) and Settings button (right) shown in the sidebar footer.
    fn render_settings(&mut self, controls_ui: &mut egui::Ui, p: UiColors) {
        // 1. Theme toggle button (left edge)
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

        // 2. Settings button (bottom-right edge)
        let mut right_ui = controls_ui.new_child(
            egui::UiBuilder::default()
                .max_rect(controls_ui.max_rect())
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right_ui.spacing_mut().item_spacing = controls_ui.spacing().item_spacing;
        let controls_ui = &mut right_ui;

        let (btn_rect, resp) = controls_ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
        if resp.hovered() || self.settings_open {
            controls_ui.painter().rect_filled(btn_rect, egui::CornerRadius::same(4), p.hover);
        }
        let color = if resp.hovered() || self.settings_open { p.strong } else { p.muted };
        draw_lucide_icon(
            controls_ui.painter(),
            egui::Rect::from_center_size(btn_rect.center(), egui::vec2(15.0, 15.0)),
            LucideIcon::SlidersHorizontal,
            color,
        );
        if resp.clicked() {
            self.settings_open = !self.settings_open;
            self.keymap_recording = None;
            self.keymap_note = None;
            if self.settings_open {
                controls_ui.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
            }
        }
        let has_update = self.updater.has_update();
        if has_update && !self.update_viewed {
            let dot_pos = egui::pos2(btn_rect.right() - 4.0, btn_rect.top() + 4.0);
            controls_ui.painter().circle_filled(dot_pos, 3.5, egui::Color32::from_rgb(239, 68, 68));
            controls_ui.painter().circle_stroke(
                dot_pos,
                3.5,
                egui::Stroke::new(1.2, if resp.hovered() || self.settings_open { p.hover } else { p.sidebar }),
            );
        }
        let tip_base = tip_with_shortcut(controls_ui.ctx(), &tr(controls_ui.ctx(), "Settings"), Action::Settings);
        let tip = if has_update {
            format!("{} · {}", tip_base, tr(controls_ui.ctx(), "Update available"))
        } else {
            tip_base
        };
        resp.on_hover_text(tip)
            .on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    /// While the settings panel is open: records the next key press for the selected row
    /// (or closes the panel on Esc). Runs before other handlers so they never see the keys.
    fn capture_keymap_keys(&mut self, ctx: &egui::Context) {
        // Read the next key press for the row being recorded, before anything else sees it.
        if let Some(action) = self.keymap_recording {
            match record_key(ctx) {
                Some(Recorded::Shortcut(sc)) => {
                    self.keymap_note = self.keymap.set(action, Some(sc)).map(|other| {
                        format!(
                            "{} {} “{}”",
                            sc.label(),
                            tr(ctx, "was removed from"),
                            tr(ctx, other.label())
                        )
                    });
                    self.keymap_recording = None;
                    self.cache_dirty = true;
                }
                Some(Recorded::Clear) => {
                    self.keymap.set(action, None);
                    self.keymap_recording = None;
                    self.keymap_note = None;
                    self.cache_dirty = true;
                }
                Some(Recorded::Cancel) => self.keymap_recording = None,
                None => {}
            }
        } else if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)
        }) {
            self.settings_open = false;
            self.keymap_recording = None;
            self.keymap_note = None;
        }
    }

    /// Settings General tab: Language and JSON Indentation.
    fn render_settings_general(&mut self, ui: &mut egui::Ui, p: &UiColors, dark: bool, card_h: f32) {
        let group = egui::Frame::default()
            .fill(p.surface)
            .stroke(egui::Stroke::new(
                0.5,
                if dark {
                    egui::Color32::from_white_alpha(28)
                } else {
                    egui::Color32::from_rgba_unmultiplied(15, 23, 42, 30)
                },
            ))
            .corner_radius(egui::CornerRadius::same(10))
            .shadow(egui::epaint::Shadow {
                offset: [0, 1],
                blur: 6,
                spread: 0,
                color: egui::Color32::from_black_alpha(if dark { 60 } else { 10 }),
            })
            .inner_margin(egui::Margin::symmetric(16, 14));

        group.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(card_h - 28.0);

            // 1. Language setting
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Language"))
                            .size(13.0)
                            .strong()
                            .color(p.strong),
                    );
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Interface language"))
                            .size(11.5)
                            .color(p.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cur_idx = if self.language == Language::English { 0 } else { 1 };
                    if let Some(idx) = segmented(ui, &["English", "中文"], cur_idx) {
                        let new_lang = if idx == 0 { Language::English } else { Language::Chinese };
                        if new_lang != self.language {
                            self.language = new_lang;
                            self.language.apply(ui.ctx());
                            self.cache_dirty = true;
                        }
                    }
                });
            });

            ui.add_space(14.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(14.0);

            // 2. JSON Indentation setting
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(tr(ui.ctx(), "JSON Indentation"))
                        .size(13.0)
                        .strong()
                        .color(p.strong),
                );
                ui.label(
                    egui::RichText::new(tr(
                        ui.ctx(),
                        "Number of spaces used for indentation when formatting",
                    ))
                    .size(11.5)
                    .color(p.muted),
                );
            });
            ui.add_space(12.0);

            // Pills for 2, 4, 6, 8
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                for &indent in &Indentation::ALL {
                    let spaces = indent.spaces();
                    let is_sel = self.indentation == indent;
                    let text = format!("{}", spaces);
                    let font = egui::FontId::proportional(12.5);
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(44.0, 26.0),
                        egui::Sense::click(),
                    );
                    let (bg, fg) = if is_sel {
                        (p.primary, egui::Color32::WHITE)
                    } else if resp.hovered() {
                        (p.hover, p.strong)
                    } else {
                        (p.subtle, p.text)
                    };
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(6), bg);
                    if !is_sel {
                        ui.painter().rect_stroke(
                            rect,
                            egui::CornerRadius::same(6),
                            egui::Stroke::new(1.0, p.border),
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, fg);
                    if resp.clicked() && !is_sel {
                        self.indentation = indent;
                        self.indentation.apply(ui.ctx());
                        for tab in &mut self.tabs {
                            tab.set_indentation(indent);
                        }
                        self.cache_dirty = true;
                    }
                    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("{} {}", spaces, tr(ui.ctx(), "spaces")));
                }
            });
        });
    }

    /// Settings Font tab: Search and choose global application font.
    /// Settings Font tab: Search and choose global application font.
    fn render_settings_font(&mut self, ui: &mut egui::Ui, p: &UiColors, dark: bool, card_h: f32) {
        let group = egui::Frame::default()
            .fill(p.surface)
            .stroke(egui::Stroke::new(
                0.5,
                if dark {
                    egui::Color32::from_white_alpha(28)
                } else {
                    egui::Color32::from_rgba_unmultiplied(15, 23, 42, 30)
                },
            ))
            .corner_radius(egui::CornerRadius::same(10))
            .shadow(egui::epaint::Shadow {
                offset: [0, 1],
                blur: 6,
                spread: 0,
                color: egui::Color32::from_black_alpha(if dark { 60 } else { 10 }),
            })
            .inner_margin(egui::Margin::symmetric(16, 14));

        group.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(card_h - 28.0);

            // 1. Header
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(tr(ui.ctx(), "Global font"))
                        .size(13.0)
                        .strong()
                        .color(p.strong),
                );
                ui.label(
                    egui::RichText::new(tr(ui.ctx(), "Choose the font used throughout the app"))
                        .size(11.5)
                        .color(p.muted),
                );
            });
            ui.add_space(10.0);

            // 2. Search bar with search icon and clear button
            let search_w = ui.available_width();
            let (search_rect, search_box_resp) =
                ui.allocate_exact_size(egui::vec2(search_w, 28.0), egui::Sense::click());
            let search_id = ui.id().with("font_search_input");
            if search_box_resp.clicked() {
                ui.ctx().memory_mut(|m| m.request_focus(search_id));
            }
            let is_focused = ui.ctx().memory(|m| m.has_focus(search_id));

            ui.painter().rect_filled(search_rect, egui::CornerRadius::same(7), p.subtle);
            ui.painter().rect_stroke(
                search_rect,
                egui::CornerRadius::same(7),
                egui::Stroke::new(1.0, if is_focused { p.blue } else { p.border }),
                egui::StrokeKind::Inside,
            );
            draw_lucide_icon(
                ui.painter(),
                egui::Rect::from_center_size(
                    egui::pos2(search_rect.left() + 14.0, search_rect.center().y),
                    egui::vec2(13.0, 13.0),
                ),
                LucideIcon::Search,
                if is_focused { p.blue } else { p.muted },
            );

            let has_query = !self.fonts.search.is_empty();
            let edit_right = search_rect.right() - if has_query { 28.0 } else { 8.0 };
            let edit_rect = egui::Rect::from_min_max(
                egui::pos2(search_rect.left() + 26.0, search_rect.top()),
                egui::pos2(edit_right, search_rect.bottom()),
            );
            let mut search_ui = ui.new_child(
                egui::UiBuilder::default()
                    .max_rect(edit_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let search_resp = search_ui.add(
                egui::TextEdit::singleline(&mut self.fonts.search)
                    .id(search_id)
                    .hint_text(
                        egui::RichText::new(tr(ui.ctx(), "Search fonts…")).color(p.faint),
                    )
                    .desired_width(edit_rect.width())
                    .margin(egui::Margin::ZERO)
                    .frame(false),
            );

            if has_query {
                let clear_rect = egui::Rect::from_center_size(
                    egui::pos2(search_rect.right() - 14.0, search_rect.center().y),
                    egui::vec2(16.0, 16.0),
                );
                let clear_resp = ui.interact(clear_rect, ui.id().with("font_search_clear"), egui::Sense::click());
                if clear_resp.hovered() {
                    ui.painter().circle_filled(clear_rect.center(), 8.0, p.hover);
                }
                draw_lucide_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(clear_rect.center(), egui::vec2(10.0, 10.0)),
                    LucideIcon::X,
                    if clear_resp.hovered() { p.strong } else { p.muted },
                );
                if clear_resp.clicked() {
                    self.fonts.search.clear();
                    ui.ctx().memory_mut(|m| m.request_focus(search_id));
                }
                clear_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
            ui.add_space(8.0);

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

            let row_h = 28.0;
            let list_h = 138.0;
            let mut picked = None;
            let list_start_y = ui.cursor().top();
            let track_rect = egui::Rect::from_min_max(
                egui::pos2(ui.available_rect_before_wrap().left(), list_start_y + 11.0),
                egui::pos2(ui.available_rect_before_wrap().right(), list_start_y + list_h - 11.0),
            );
            let mut list = egui::ScrollArea::vertical()
                .id_salt("settings_font_list")
                .max_height(list_h)
                .min_scrolled_height(list_h)
                .auto_shrink([false, false])
                .scroll_bar_rect(track_rect);
            if search_resp.changed() {
                list = list.vertical_scroll_offset(0.0);
            }
            let avail_w = ui.available_width();
            let scroll_pad = 18.0;
            let row_w = avail_w - scroll_pad;
            list.show_rows(ui, row_h, filtered.len(), |ui, rows| {
                for row in rows {
                    let index = filtered[row];
                    let choice = &self.fonts.available[index];
                    let is_sel = self.fonts.selected == index;
                    let (row_rect, row_resp) =
                        ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::click());
                    let pill = egui::Rect::from_min_max(
                        egui::pos2(row_rect.left() - 2.0, row_rect.top() + 1.0),
                        egui::pos2(row_rect.right(), row_rect.bottom() - 1.0),
                    );
                    if row_resp.hovered() {
                        let hover_bg = if dark {
                            egui::Color32::from_white_alpha(14)
                        } else {
                            egui::Color32::from_black_alpha(10)
                        };
                        ui.painter().rect_filled(pill, egui::CornerRadius::same(6), hover_bg);
                    }
                    let font_name = tr(ui.ctx(), &choice.label);
                    let color = if is_sel { p.primary } else if row_resp.hovered() { p.strong } else { p.text };
                    ui.painter().text(
                        egui::pos2(row_rect.left() + 2.0, row_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        font_name,
                        egui::FontId::proportional(12.5),
                        color,
                    );
                    if row_resp.clicked() {
                        picked = Some(index);
                    }
                    row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            });
            if filtered.is_empty() {
                ui.label(egui::RichText::new(tr(ui.ctx(), "No matching fonts")).size(12.0).color(p.muted));
            }
            if let Some(index) = picked {
                if self.fonts.apply(ui.ctx(), index) {
                    for tab in &mut self.tabs {
                        tab.editor_content_width = 0.0;
                        tab.tree_content_width = 0.0;
                        if let Some(comparison) = &mut tab.comparison {
                            comparison.right.editor_content_width = 0.0;
                            comparison.right.tree_content_width = 0.0;
                        }
                    }
                    self.cache_dirty = true;
                }
            }

            ui.add_space(8.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(8.0);

            // 3. Preview section (no background box)
            ui.label(
                egui::RichText::new(tr(ui.ctx(), "Preview"))
                    .size(10.5)
                    .color(p.muted),
            );
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new("AaBbCc 0123456789  { \"json\": true }  中文字体效果")
                    .size(12.5)
                    .color(p.strong),
            );
            if let Some(error) = &self.fonts.error {
                ui.add_space(4.0);
                ui.colored_label(p.danger, tr(ui.ctx(), "Unable to load font"));
                ui.label(egui::RichText::new(error).small().color(p.muted));
            }
        });
    }

    /// Settings Shortcuts tab: Keyboard shortcuts remapping.
    fn render_settings_shortcuts(&mut self, ui: &mut egui::Ui, p: &UiColors, dark: bool, card_h: f32) {
        let group = egui::Frame::default()
            .fill(p.surface)
            .stroke(egui::Stroke::new(
                0.5,
                if dark {
                    egui::Color32::from_white_alpha(28)
                } else {
                    egui::Color32::from_rgba_unmultiplied(15, 23, 42, 30)
                },
            ))
            .corner_radius(egui::CornerRadius::same(10))
            .shadow(egui::epaint::Shadow {
                offset: [0, 1],
                blur: 6,
                spread: 0,
                color: egui::Color32::from_black_alpha(if dark { 60 } else { 10 }),
            })
            .inner_margin(egui::Margin::symmetric(16, 14));

        group.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(card_h - 28.0);

            // Header
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Shortcuts"))
                            .size(13.0)
                            .strong()
                            .color(p.strong),
                    );
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Click any action to reassign its key combination"))
                            .size(11.5)
                            .color(p.muted),
                    );
                });
            });
            ui.add_space(10.0);

            let row_h = 32.0;
            let list_h = 208.0;
            let list_start_y = ui.cursor().top();
            let track_rect = egui::Rect::from_min_max(
                egui::pos2(ui.available_rect_before_wrap().left(), list_start_y + 11.0),
                egui::pos2(ui.available_rect_before_wrap().right(), list_start_y + list_h - 11.0),
            );
            egui::ScrollArea::vertical()
                .id_salt("settings_keymap_list")
                .max_height(list_h)
                .min_scrolled_height(list_h)
                .auto_shrink([false, false])
                .scroll_bar_rect(track_rect)
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let scroll_pad = 18.0;
                    let row_w = avail_w - scroll_pad;
                    let mut prev_active = false;
                    for (i, action) in Action::ALL.iter().copied().enumerate() {
                        let (row, row_resp) =
                            ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::click());
                        let hovered = row_resp.hovered();
                        let recording = self.keymap_recording == Some(action);
                        let is_active = hovered || recording;

                        let pill = egui::Rect::from_min_max(
                            egui::pos2(row.left() - 2.0, row.top() + 1.0),
                            egui::pos2(row.right(), row.bottom() - 1.0),
                        );
                        if recording {
                            ui.painter().rect_filled(
                                pill,
                                egui::CornerRadius::same(6),
                                if dark {
                                    p.primary.gamma_multiply(0.18)
                                } else {
                                    p.primary.gamma_multiply(0.10)
                                },
                            );
                            ui.painter().rect_stroke(
                                pill,
                                egui::CornerRadius::same(6),
                                egui::Stroke::new(1.0, p.primary.gamma_multiply(0.35)),
                                egui::StrokeKind::Inside,
                            );
                        } else if hovered {
                            let hover_bg = if dark {
                                egui::Color32::from_white_alpha(14)
                            } else {
                                egui::Color32::from_black_alpha(10)
                            };
                            ui.painter().rect_filled(
                                pill,
                                egui::CornerRadius::same(6),
                                hover_bg,
                            );
                        }

                        let left = row.left() + 2.0;
                        let right = row.right() - 6.0;

                        if i > 0 && !is_active && !prev_active {
                            ui.painter().hline(
                                egui::Rangef::new(left, right),
                                row.top(),
                                egui::Stroke::new(0.5, p.border.gamma_multiply(0.5)),
                            );
                        }
                        prev_active = is_active;
                        ui.painter().text(
                            egui::pos2(left, row.center().y),
                            egui::Align2::LEFT_CENTER,
                            tr(ui.ctx(), action.label()),
                            egui::FontId::proportional(12.5),
                            if recording { p.primary } else if hovered { p.strong } else { p.text },
                        );

                        let cy = row.center().y;
                        if recording {
                            let text = tr(ui.ctx(), "Press keys…");
                            let font = egui::FontId::proportional(12.0);
                            let w = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), p.primary).size().x + 20.0;
                            let field = egui::Rect::from_min_max(egui::pos2(right - w, cy - 11.0), egui::pos2(right, cy + 11.0));
                            ui.painter().rect_filled(field, egui::CornerRadius::same(6), p.primary.gamma_multiply(0.15));
                            ui.painter().rect_stroke(
                                field,
                                egui::CornerRadius::same(6),
                                egui::Stroke::new(1.5, p.primary),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(field.center(), egui::Align2::CENTER_CENTER, text, font, p.primary);
                        } else if let Some(sc) = self.keymap.get(action) {
                            let font = egui::FontId::proportional(11.5);
                            let mut x = right;
                            for key in sc.keys().iter().rev() {
                                let galley = ui.painter().layout_no_wrap(key.clone(), font.clone(), p.strong);
                                let w = (galley.size().x + 10.0).max(22.0);
                                let cap = egui::Rect::from_min_max(egui::pos2(x - w, cy - 10.5), egui::pos2(x, cy + 10.5));
                                let shadow_col = egui::Color32::from_black_alpha(if dark { 50 } else { 20 });
                                ui.painter().rect_filled(cap.translate(egui::vec2(0.0, 1.0)), egui::CornerRadius::same(5), shadow_col);
                                ui.painter().rect_filled(
                                    cap,
                                    egui::CornerRadius::same(5),
                                    if dark { egui::Color32::from_rgb(38, 44, 58) } else { egui::Color32::WHITE },
                                );
                                ui.painter().rect_stroke(
                                    cap,
                                    egui::CornerRadius::same(5),
                                    egui::Stroke::new(1.0, if hovered { p.pressed } else { p.border }),
                                    egui::StrokeKind::Inside,
                                );
                                ui.painter().galley(
                                    cap.center() - galley.size() * 0.5,
                                    galley,
                                    if hovered { p.strong } else { p.text },
                                );
                                x -= w + 4.0;
                            }
                        } else {
                            ui.painter().text(
                                egui::pos2(right, cy),
                                egui::Align2::RIGHT_CENTER,
                                tr(ui.ctx(), "Not set"),
                                egui::FontId::proportional(12.0),
                                if hovered { p.muted } else { p.faint },
                            );
                        }

                        if row_resp.clicked() {
                            self.keymap_recording = if recording { None } else { Some(action) };
                            self.keymap_note = None;
                        }
                        row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                });

            ui.add_space(8.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(6.0);
            if let Some(note) = &self.keymap_note {
                ui.label(egui::RichText::new(note).size(11.5).color(p.amber));
                ui.add_space(2.0);
            }
            ui.label(
                egui::RichText::new(tr(ui.ctx(), "Click a row, then press the new keys. Delete clears, Esc cancels."))
                    .size(11.5)
                    .color(p.muted),
            );
        });
    }

fn app_logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("app_logo_texture");
    ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)).unwrap_or_else(|| {
        let bytes = include_bytes!("../assets/logo.png");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes.as_slice()));
        let mut reader = decoder.read_info().expect("IJson logo");
        let mut rgba = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).expect("IJson logo frame");
        rgba.truncate(info.buffer_size());
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [info.width as usize, info.height as usize],
            &rgba,
        );
        let texture = ctx.load_texture("app_logo", color_image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
        texture
    })
}

    /// Settings About tab: App info, update status, and auto-updater.
    fn render_settings_about(&mut self, ui: &mut egui::Ui, p: &UiColors, dark: bool, card_h: f32) {
        let group = egui::Frame::default()
            .fill(p.surface)
            .stroke(egui::Stroke::new(
                0.5,
                if dark {
                    egui::Color32::from_white_alpha(28)
                } else {
                    egui::Color32::from_rgba_unmultiplied(15, 23, 42, 30)
                },
            ))
            .corner_radius(egui::CornerRadius::same(10))
            .shadow(egui::epaint::Shadow {
                offset: [0, 1],
                blur: 6,
                spread: 0,
                color: egui::Color32::from_black_alpha(if dark { 60 } else { 10 }),
            })
            .inner_margin(egui::Margin::symmetric(16, 14));

        group.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(card_h - 28.0);

            // 1. App identity with genuine app logo
            ui.horizontal(|ui| {
                let logo = Self::app_logo_texture(ui.ctx());
                let icon_size = egui::vec2(44.0, 44.0);
                let (icon_rect, _) = ui.allocate_exact_size(icon_size, egui::Sense::hover());
                ui.painter().add(
                    egui::epaint::RectShape::filled(
                        icon_rect,
                        egui::CornerRadius::same(10),
                        egui::Color32::WHITE,
                    )
                    .with_texture(
                        logo.id(),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    ),
                );

                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("IJson").size(16.0).strong().color(p.strong));
                        let font = egui::FontId::proportional(11.0);
                        let text = format!("v{}", CURRENT_VERSION);
                        let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), p.muted);
                        let (v_rect, _) = ui.allocate_exact_size(egui::vec2(galley.size().x + 12.0, 18.0), egui::Sense::hover());
                        ui.painter().rect_filled(v_rect, egui::CornerRadius::same(9), p.subtle);
                        ui.painter().text(v_rect.center(), egui::Align2::CENTER_CENTER, text, font, p.muted);
                    });
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "A fast, native JSON viewer and editor"))
                            .size(11.5)
                            .color(p.muted),
                    );
                });
            });

            ui.add_space(14.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(14.0);

            // 2. Software update section
            let green = if dark {
                egui::Color32::from_rgb(74, 222, 128)
            } else {
                egui::Color32::from_rgb(22, 163, 74)
            };
            let status = self.updater.status();

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Software Update"))
                            .size(13.0)
                            .strong()
                            .color(p.strong),
                    );
                    ui.add_space(1.0);
                    match &status {
                        UpdateStatus::Available(release) => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let (dot_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(8.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(dot_rect.center(), 3.5, p.primary);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} · v{}",
                                        tr(ui.ctx(), "New version available"),
                                        release.version
                                    ))
                                    .size(11.5)
                                    .color(p.text),
                                );
                            });
                        }
                        UpdateStatus::Downloading { version, .. } => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} v{}",
                                    tr(ui.ctx(), "Downloading…"),
                                    version
                                ))
                                .size(11.5)
                                .color(p.muted),
                            );
                        }
                        UpdateStatus::Installing { version } => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.add(egui::Spinner::new().size(11.0));
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} (v{})…",
                                        tr(ui.ctx(), "Installing update and restarting"),
                                        version
                                    ))
                                    .size(11.5)
                                    .color(p.muted),
                                );
                            });
                        }
                        UpdateStatus::Downloaded { version, .. } => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let (icon_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(13.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                draw_lucide_icon(
                                    ui.painter(),
                                    icon_rect,
                                    LucideIcon::Check,
                                    green,
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} · v{}",
                                        tr(ui.ctx(), "Update ready to install"),
                                        version
                                    ))
                                    .size(11.5)
                                    .color(p.text),
                                );
                            });
                        }
                        UpdateStatus::Checking => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.add(egui::Spinner::new().size(11.0));
                                ui.label(
                                    egui::RichText::new(tr(ui.ctx(), "Checking for updates…"))
                                        .size(11.5)
                                        .color(p.muted),
                                );
                            });
                        }
                        UpdateStatus::UpToDate => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let (icon_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(13.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                draw_lucide_icon(
                                    ui.painter(),
                                    icon_rect,
                                    LucideIcon::Check,
                                    green,
                                );
                                ui.label(
                                    egui::RichText::new(tr(ui.ctx(), "You're up to date"))
                                        .size(11.5)
                                        .color(p.muted),
                                );
                            });
                        }
                        UpdateStatus::Error(_) => {
                            ui.label(
                                egui::RichText::new(tr(ui.ctx(), "Failed to check for updates"))
                                    .size(11.5)
                                    .color(p.danger),
                            );
                        }
                        UpdateStatus::Idle => {
                            ui.label(
                                egui::RichText::new(tr(ui.ctx(), "Check for the latest release"))
                                    .size(11.5)
                                    .color(p.muted),
                            );
                        }
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &status {
                        UpdateStatus::Available(release) => {
                            if settings_action_button(ui, Some(LucideIcon::Download), "Update & Restart", true, p).clicked() {
                                self.updater.download(ui.ctx().clone(), release.clone(), true);
                            }
                        }
                        UpdateStatus::Downloaded { file_path, .. } => {
                            if settings_action_button(ui, None, "Restart & Install", true, p).clicked() {
                                self.updater.apply_and_restart(file_path);
                            }
                        }
                        UpdateStatus::UpToDate | UpdateStatus::Idle => {
                            if settings_action_button(ui, Some(LucideIcon::RefreshCw), "Check for Updates", false, p).clicked() {
                                self.updater.check(ui.ctx().clone(), true);
                            }
                        }
                        UpdateStatus::Error(_) => {
                            if settings_action_button(ui, Some(LucideIcon::RefreshCw), "Retry", false, p).clicked() {
                                self.updater.check(ui.ctx().clone(), true);
                            }
                        }
                        UpdateStatus::Checking
                        | UpdateStatus::Downloading { .. }
                        | UpdateStatus::Installing { .. } => {}
                    }
                });
            });

            // Optional detail banner for active update states (available, downloading, downloaded, error)
            match &status {
                UpdateStatus::Available(release) => {
                    ui.add_space(10.0);
                    egui::Frame::default()
                        .fill(p.subtle)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let note = if !release.name.is_empty() && release.name != release.tag {
                                    release.name.as_str()
                                } else {
                                    release.tag.as_str()
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(note).size(11.5).color(p.muted),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let link = ui
                                        .add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{} ↗",
                                                    tr(ui.ctx(), "Release Notes")
                                                ))
                                                .size(11.5)
                                                .color(p.blue),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if link.clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::new_tab(&release.html_url));
                                    }
                                });
                            });
                        });
                }
                UpdateStatus::Downloading {
                    downloaded_bytes,
                    total_bytes,
                    progress,
                    ..
                } => {
                    ui.add_space(10.0);
                    egui::Frame::default()
                        .fill(p.subtle)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let pct = (progress.clamp(0.0, 1.0) * 100.0).round() as u32;
                            let size_text = if let Some(tot) = total_bytes {
                                format!(
                                    "{:.1} MB / {:.1} MB",
                                    *downloaded_bytes as f64 / 1_048_576.0,
                                    *tot as f64 / 1_048_576.0
                                )
                            } else {
                                format!("{:.1} MB", *downloaded_bytes as f64 / 1_048_576.0)
                            };
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(size_text).size(11.0).color(p.muted));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{pct}%"))
                                            .size(11.0)
                                            .strong()
                                            .color(p.text),
                                    );
                                });
                            });
                            ui.add_space(5.0);
                            let (bar_rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 5.0),
                                egui::Sense::hover(),
                            );
                            let radius = egui::CornerRadius::same(3);
                            ui.painter().rect_filled(bar_rect, radius, p.border);
                            let fill_w = (bar_rect.width() * progress.clamp(0.0, 1.0)).max(0.0);
                            if fill_w > 0.0 {
                                let fill_rect = egui::Rect::from_min_size(
                                    bar_rect.min,
                                    egui::vec2(fill_w, bar_rect.height()),
                                );
                                ui.painter().rect_filled(fill_rect, radius, p.primary);
                            }
                        });
                }
                UpdateStatus::Downloaded { file_path, .. } => {
                    ui.add_space(10.0);
                    egui::Frame::default()
                        .fill(p.subtle)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(tr(
                                            ui.ctx(),
                                            "The update has been downloaded. Restart to apply.",
                                        ))
                                        .size(11.5)
                                        .color(p.muted),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let link = ui
                                        .add(
                                            egui::Label::new(
                                                egui::RichText::new(tr(ui.ctx(), "Open Installer"))
                                                    .size(11.5)
                                                    .color(p.blue),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if link.clicked() {
                                        self.updater.open_installer(file_path);
                                    }
                                });
                            });
                        });
                }
                UpdateStatus::Error(err) => {
                    ui.add_space(8.0);
                    egui::Frame::default()
                        .fill(p.danger_bg)
                        .stroke(egui::Stroke::new(1.0, p.danger_border))
                        .corner_radius(egui::CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(err).size(11.0).color(p.danger),
                                )
                                .wrap(),
                            );
                        });
                }
                _ => {}
            }

            ui.add_space(14.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(14.0);

            // 3. Auto-check preference row (matching Language row layout)
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Automatically check for updates"))
                            .size(13.0)
                            .strong()
                            .color(p.strong),
                    );
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Check for new versions on startup"))
                            .size(11.5)
                            .color(p.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if settings_toggle_switch(
                        ui,
                        egui::Id::new("settings_auto_check_updates"),
                        &mut self.auto_check_updates,
                        p,
                    ) {
                        self.cache_dirty = true;
                    }
                });
            });

            ui.add_space(14.0);
            ui.painter().hline(
                ui.available_rect_before_wrap().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, p.border.gamma_multiply(0.6)),
            );
            ui.add_space(10.0);

            // 4. GitHub repository link row
            let (row_rect, row_resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 32.0),
                egui::Sense::click(),
            );
            let row_resp = row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let hover_rect = row_rect.expand2(egui::vec2(6.0, 0.0));
            if row_resp.is_pointer_button_down_on() {
                ui.painter().rect_filled(hover_rect, egui::CornerRadius::same(7), p.pressed);
            } else if row_resp.hovered() {
                ui.painter().rect_filled(hover_rect, egui::CornerRadius::same(7), p.subtle);
            }
            let title_color = if row_resp.hovered() { p.strong } else { p.text };
            ui.painter().text(
                egui::pos2(row_rect.left(), row_rect.center().y),
                egui::Align2::LEFT_CENTER,
                tr(ui.ctx(), "View on GitHub"),
                egui::FontId::proportional(12.5),
                title_color,
            );
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(row_rect.right() - 6.5, row_rect.center().y),
                egui::vec2(13.0, 13.0),
            );
            let icon_color = if row_resp.hovered() { p.blue } else { p.muted };
            draw_lucide_icon(ui.painter(), icon_rect, LucideIcon::ExternalLink, icon_color);
            ui.painter().text(
                egui::pos2(icon_rect.left() - 6.0, row_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "github.com/leenstx/IJson",
                egui::FontId::proportional(11.5),
                if row_resp.hovered() { p.text } else { p.muted },
            );
            if row_resp.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/leenstx/IJson"));
            }
        });
    }

    /// Unified settings panel: General, Font, Shortcuts tabs.
    fn render_settings_panel(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let p = UiColors::from_ctx(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let screen = ctx.screen_rect();
        let mut close = false;

        egui::Area::new(egui::Id::new("settings_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    egui::Color32::from_black_alpha(if dark { 90 } else { 30 }),
                );
                if resp.clicked() {
                    close = true;
                }
            });

        let width = 520.0;
        let card_h = 320.0;
        let window_height = 474.0;
        egui::Window::new("settings_panel")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -8.0))
            .fixed_size(egui::vec2(width, window_height))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(p.canvas)
                    .inner_margin(egui::Margin::symmetric(18, 16)),
            )
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;

                // Header: title and close.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr(ui.ctx(), "Settings"))
                            .size(15.0)
                            .strong()
                            .color(p.strong),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, egui::CornerRadius::same(6), p.hover);
                        }
                        draw_lucide_icon(
                            ui.painter(),
                            rect.shrink(6.0),
                            LucideIcon::X,
                            if resp.hovered() { p.strong } else { p.muted },
                        );
                        if resp.clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(12.0);

                // Segmented tab bar: General, Font, Shortcuts, About
                let tab_idx = match self.settings_tab {
                    SettingsTab::General => 0,
                    SettingsTab::Font => 1,
                    SettingsTab::Shortcuts => 2,
                    SettingsTab::About => 3,
                };
                let badge_idx = if self.updater.has_update() && !self.update_viewed { Some(3) } else { None };
                if self.settings_tab == SettingsTab::About {
                    self.update_viewed = true;
                }
                if let Some(clicked) = settings_tab_bar(ui, &["General", "Font", "Shortcuts", "About"], tab_idx, badge_idx) {
                    self.settings_tab = match clicked {
                        0 => SettingsTab::General,
                        1 => SettingsTab::Font,
                        2 => SettingsTab::Shortcuts,
                        _ => {
                            self.update_viewed = true;
                            SettingsTab::About
                        }
                    };
                    self.keymap_recording = None;
                }
                ui.add_space(14.0);

                // Tab content: exactly card_h across all tabs
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), card_h),
                    egui::Layout::top_down_justified(egui::Align::Min),
                    |ui| {
                        match self.settings_tab {
                            SettingsTab::General => self.render_settings_general(ui, &p, dark, card_h),
                            SettingsTab::Font => self.render_settings_font(ui, &p, dark, card_h),
                            SettingsTab::Shortcuts => self.render_settings_shortcuts(ui, &p, dark, card_h),
                            SettingsTab::About => self.render_settings_about(ui, &p, dark, card_h),
                        }
                    },
                );

                // Footer
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if self.settings_tab == SettingsTab::Shortcuts && !self.keymap.is_default() {
                        let reset = ui.add(
                            egui::Label::new(
                                egui::RichText::new(tr(ui.ctx(), "Restore defaults")).size(12.5).color(p.blue),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if reset.clicked() {
                            self.keymap = Keymap::default();
                            self.keymap_recording = None;
                            self.keymap_note = None;
                            self.cache_dirty = true;
                        }
                        reset.on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if compact_button(ui, "Done", true).clicked() {
                            close = true;
                        }
                    });
                });
            });

        if close {
            self.settings_open = false;
            self.keymap_recording = None;
            self.keymap_note = None;
        }
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        self.language.apply(ctx);
        let colors = ThemeColors::current(ctx);

        self.handle_shortcuts(ctx);
        self.handle_tab_shortcuts(ctx);
        self.select_comparison_editor(ctx);
        // The settings panel owns the keyboard while it is open.
        if !self.settings_open {
            self.handle_edit_history(ctx);
            // Command+V replaces the whole document only when the editor is not being edited.
            // Insert at the selection, then format valid JSON as part of the same edit.
            self.handle_paste(ctx);
            self.handle_folded_copy_cut(ctx);
        }
        self.open_dropped_files(ctx);

        // Solid neutral canvas behind the sidebar and cards.
        ctx.layer_painter(egui::LayerId::background()).rect_filled(
            ctx.screen_rect(),
            egui::CornerRadius::ZERO,
            colors.ui.canvas,
        );
        // Slide the sidebar in and out instead of snapping.
        let sidebar_t = ctx.animate_bool_with_time_and_easing(
            egui::Id::new("sidebar_slide"),
            !self.sidebar_hidden,
            0.22,
            egui::emath::easing::cubic_out,
        );
        if sidebar_t > 0.0 {
            self.render_tab_rail(ctx, colors, sidebar_t);
        }

        let mut open_comparison = false;
        // Layout: a toolbar row level with the window controls, then floating cards.
        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ctx, |ui| {
                let full = ui.available_rect_before_wrap();
                let p = colors.ui;
                let side_gap = snap_to_pixels(ctx, 8.0 - 4.0 * sidebar_t);
                // Whole-point edges keep both cards' borders equally crisp and level.
                let cards_rect = egui::Rect::from_min_max(
                    egui::pos2((full.left() + side_gap).round(), (full.top() + TOP_ROW_H + 6.0).round()),
                    egui::pos2((full.right() - 8.0).round(), (full.bottom() - 8.0).round()),
                );
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(full.left() + side_gap, full.top()),
                    egui::pos2(cards_rect.right(), full.top() + TOP_ROW_H),
                );

                let mut tb = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(bar_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                tb.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
                // Keep clear of the traffic lights whenever the sidebar is too narrow to hold them.
                if cfg!(target_os = "macos") {
                    tb.add_space((72.0 - bar_rect.left()).max(0.0));
                }
                if self.sidebar_hidden {
                    let resp = btn_sidebar(&mut tb, false);
                    if resp.clicked() {
                        self.sidebar_hidden = false;
                        self.cache_dirty = true;
                    }
                    if self.updater.has_update() && !self.update_viewed {
                        let dot_pos = egui::pos2(resp.rect.right() - 4.0, resp.rect.top() + 4.0);
                        tb.painter().circle_filled(dot_pos, 3.5, egui::Color32::from_rgb(239, 68, 68));
                        tb.painter().circle_stroke(
                            dot_pos,
                            3.5,
                            egui::Stroke::new(1.2, if resp.hovered() { p.hover } else { p.titlebar }),
                        );
                    }
                    if btn_new_tab(&mut tb).clicked() {
                        let ctx = tb.ctx().clone();
                        self.add_tab(&ctx);
                    }
                    toolbar_separator(&mut tb);
                }

                if self.tabs[self.active].comparison.is_some() {
                    let bar_left = tb.min_rect().right();
                    if self.render_comparison(ui, cards_rect, bar_rect, bar_left, colors) {
                        self.add_comparison_tab(ctx);
                    }
                    ui.allocate_rect(full, egui::Sense::hover());
                    return;
                }

                let total_w = cards_rect.width();
                let max_w = (total_w - 260.0).max(300.0);
                if !self.split_customized {
                    self.left_panel_width = total_w * 0.5;
                }
                self.left_panel_width = self.left_panel_width.clamp(260.0, max_w);
                let left_w = self.left_panel_width;
                let edit_mode = self.tabs[self.active].edit_mode;

                // Toolbar: actions grouped by feature
                // (mode | reshape | history | folding | output | clear), search on the right.
                if btn_mode(&mut tb, edit_mode).clicked() {
                    let ctx = tb.ctx().clone();
                    self.tabs[self.active].set_edit_mode(&ctx, !edit_mode);
                    self.cache_dirty = true;
                }
                let tab = &mut self.tabs[self.active];
                toolbar_separator(&mut tb);
                tab.toolbar_reshape(&mut tb);
                toolbar_separator(&mut tb);
                tab.toolbar_history(&mut tb);
                toolbar_separator(&mut tb);
                if btn_collapse_all(&mut tb).clicked() {
                    tab.collapse_all();
                }
                if btn_expand_all(&mut tb).clicked() {
                    tab.expand_all();
                }
                toolbar_separator(&mut tb);
                tab.toolbar_output(&mut tb);
                open_comparison = btn_compare(&mut tb).clicked();
                toolbar_separator(&mut tb);
                tab.toolbar_clear(&mut tb);

                let search_rect = egui::Rect::from_min_max(
                    egui::pos2(tb.min_rect().right() + 12.0, bar_rect.top() + 6.0),
                    bar_rect.max,
                );
                let mut search_ui = ui.new_child(
                    egui::UiBuilder::default()
                        .max_rect(search_rect)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                let pill_w = search_rect.width().clamp(120.0, 300.0).min(search_rect.width());
                tab.render_search_box(&mut search_ui, pill_w, p);

                // Register window drag strictly on the empty toolbar space between tb and search box
                let empty_left = tb.min_rect().right() + 4.0;
                let empty_right = search_rect.right() - pill_w - 6.0;
                if empty_right > empty_left {
                    let empty_rect = egui::Rect::from_min_max(
                        egui::pos2(empty_left, bar_rect.top()),
                        egui::pos2(empty_right, bar_rect.bottom()),
                    );
                    window_drag_area(ui, empty_rect, "toolbar_drag");
                }

                if edit_mode {
                    // EDIT MODE: one card with the editor.
                    let body = draw_card(ui, cards_rect, true);
                    let mut editor_ui = ui.new_child(
                        egui::UiBuilder::default()
                            .max_rect(body)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    self.tabs[self.active].render_editor_card(&mut editor_ui, body, colors, None);
                    self.draw_copy_notice(&editor_ui, body, p);
                } else {
                    // VIEW MODE: whole tree on the left, selected node on the right
                    // (selection panel is hidden when there is no parsed JSON yet).
                    let tab = &mut self.tabs[self.active];
                    // The editor is hidden, so clicks and pastes must not target it.
                    tab.editor_card_rect = egui::Rect::NOTHING;
                    tab.poll_request_loading(ui.ctx());

                    if self.tabs[self.active].parsed.is_none() {
                        self.node_focused = false;
                        let tab_name = self.tabs[self.active].name.clone();
                        draw_card(ui, cards_rect, true);
                        let tree_body = draw_card_header(
                            ui,
                            cards_rect,
                            LucideIcon::Braces,
                            p.blue,
                            &tab_name,
                            true,
                            0.0,
                            p,
                        );
                        let mut tree_ui = ui.new_child(
                            egui::UiBuilder::default()
                                .max_rect(tree_body)
                                .layout(egui::Layout::top_down(egui::Align::LEFT)),
                        );
                        self.render_tree_card(&mut tree_ui, tree_body, colors);
                    } else {
                        let (left_rect, right_rect) = split_cards(cards_rect, left_w);
                        // A click inside a card focuses it.
                        if let Some(pos) = ui.input(|i| {
                            i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten()
                        }) {
                            if right_rect.contains(pos) {
                                self.node_focused = true;
                            } else if left_rect.contains(pos) {
                                self.node_focused = false;
                            }
                        }
                        let node_focused = self.node_focused;
                        let tab_name = self.tabs[self.active].name.clone();
                        let node_title = self.tabs[self.active]
                            .selected_path
                            .clone()
                            .unwrap_or_else(|| tr(ui.ctx(), "Selection").to_string());
                        let selected_is_string = self.tabs[self.active]
                            .selected_path
                            .as_deref()
                            .and_then(|path| {
                                self.tabs[self.active]
                                    .parsed
                                    .as_ref()
                                    .and_then(|parsed| value_at_path(parsed, path))
                            })
                            .is_some_and(Value::is_string);
                        draw_card(ui, left_rect, !node_focused);
                        let tree_body = draw_card_header(
                            ui,
                            left_rect,
                            LucideIcon::Braces,
                            p.blue,
                            &tab_name,
                            !node_focused,
                            0.0,
                            p,
                        );
                        let mut tree_ui = ui.new_child(
                            egui::UiBuilder::default()
                                .max_rect(tree_body)
                                .layout(egui::Layout::top_down(egui::Align::LEFT)),
                        );
                        self.render_tree_card(&mut tree_ui, tree_body, colors);

                        draw_card(ui, right_rect, node_focused);
                        let switch_reserve = if selected_is_string {
                            escape_switch_width(ui) + 12.0
                        } else {
                            0.0
                        };
                        let node_body = draw_card_header(
                            ui,
                            right_rect,
                            LucideIcon::Route,
                            p.purple,
                            &node_title,
                            node_focused,
                            switch_reserve,
                            p,
                        );
                        let mut node_ui = ui.new_child(
                            egui::UiBuilder::default()
                                .max_rect(node_body)
                                .layout(egui::Layout::top_down(egui::Align::LEFT)),
                        );
                        // Clicking the title copies the selected node's path.
                        let header = egui::Rect::from_min_size(
                            right_rect.min,
                            egui::vec2(right_rect.width(), CARD_HEADER_H),
                        );
                        if let Some(path) = self.tabs[self.active].selected_path.clone() {
                            let copy_rect = egui::Rect::from_min_max(
                                header.min,
                                egui::pos2(
                                    (header.right() - switch_reserve).max(header.left()),
                                    header.bottom(),
                                ),
                            );
                            let resp = ui
                                .interact(
                                    copy_rect,
                                    egui::Id::new("node_title_copy"),
                                    egui::Sense::click(),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(tr(ui.ctx(), "Click to copy path"));
                            if resp.clicked() {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(path);
                                }
                                let tab = &mut self.tabs[self.active];
                                tab.copy_notice = Some((
                                    tr(ui.ctx(), "Copied path").to_string(),
                                    std::time::Instant::now(),
                                ));
                                tab.copy_notice_in_node = true;
                            }
                        }
                        if selected_is_string {
                            let tab = &mut self.tabs[self.active];
                            draw_escape_switch(ui, header, tab.id, &mut tab.node_string_escaped, p);
                        }
                        self.tabs[self.active].render_node_card(&mut node_ui, node_body, colors);

                        // Register last so the divider takes priority over both cards.
                        self.render_split_handle(ui, cards_rect);
                    }
                }

                // Consume the full panel rect so egui doesn't complain
                ui.allocate_rect(full, egui::Sense::hover());
            });

        self.render_edit_modal(ctx);
        self.render_settings_panel(ctx);
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

    /// ⌘F focuses the search box and selects its text.
    /// Runs the action whose shortcut was pressed this frame, if any.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        self.keymap.publish(ctx);
        if self.settings_open {
            self.capture_keymap_keys(ctx);
            return;
        }
        // Typing in the edit dialog gets the keys instead.
        if self.edit_modal.is_some() {
            return;
        }
        if let Some(action) = self.keymap.take_pressed(ctx) {
            self.run_action(ctx, action);
        }
    }

    fn run_action(&mut self, ctx: &egui::Context, action: Action) {
        let comparing = self.tabs[self.active].comparison.is_some();
        match action {
            Action::ToggleSidebar => {
                self.sidebar_hidden = !self.sidebar_hidden;
                self.cache_dirty = true;
            }
            Action::NewTab => self.add_tab(ctx),
            Action::CloseTab => {
                if self.tabs.len() > 1 {
                    self.close_tab(ctx, self.active);
                    // Keep ⌘W from also closing the window.
                    self.suppress_close_frames = 3;
                }
            }
            Action::NextTab | Action::PrevTab => {
                let n = self.tabs.len();
                let step = if action == Action::NextTab { 1 } else { n - 1 };
                self.activate_tab(ctx, (self.active + step) % n);
            }
            Action::ToggleEditMode => {
                if !comparing {
                    let on = !self.tabs[self.active].edit_mode;
                    self.tabs[self.active].set_edit_mode(ctx, on);
                    self.cache_dirty = true;
                }
            }
            Action::Search => {
                if !comparing {
                    self.focus_search(ctx);
                }
            }
            Action::ExpandAll => self.expand_all(),
            Action::CollapseAll => self.collapse_all(),
            Action::Format => self.format_json(ctx),
            Action::Sort => self.cycle_sort(),
            Action::Simplify => {
                if self.parsed.is_some() {
                    self.simplify_json();
                }
            }
            Action::CopyJson => self.copy_json(ctx),
            Action::CopyMinified => self.copy_minified_with_notice(ctx),
            Action::Compare => self.add_comparison_tab(ctx),
            Action::Clear => self.clear_document(),
            Action::Settings => {
                self.settings_open = !self.settings_open;
                self.keymap_recording = None;
                self.keymap_note = None;
                if self.settings_open {
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
            }
        }
    }

    /// After ⌘W closes a tab, ignore the window close request it can also trigger.
    fn handle_tab_shortcuts(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
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
            if self.tabs[self.active].parse_error.is_some() {
                self.tabs[self.active].set_edit_mode(ctx, true);
            }
        }
    }

    fn render_tab_rail(&mut self, ctx: &egui::Context, colors: ThemeColors, slide: f32) {
        let mut action = RailAction::default();
        // Consume the flag before handling clicks, so a tab created this frame is revealed next frame.
        let reveal = self.reveal_active;
        self.reveal_active = false;
        // Leave enough room for the cards when the window is narrow.
        let max_width = (ctx.screen_rect().width() - 560.0).clamp(160.0, 400.0);
        self.tab_rail_width = snap_to_pixels(ctx, self.tab_rail_width.clamp(160.0, max_width));
        let p = colors.ui;
        // Not resizable by egui (it draws a line on hover); tab_rail_contents adds a handle.
        egui::SidePanel::left(egui::Id::new("json_tab_rail"))
            // While sliding, the panel narrows and the content moves out to the left.
            .exact_width(snap_to_pixels(ctx, self.tab_rail_width * slide))
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::default())
            .show(ctx, |ui| {
                action = self.tab_rail_contents(ui, reveal, p);
            });
        self.apply_rail_action(ctx, action);
    }

    /// Sidebar: window controls row, tab list, and settings footer.
    fn tab_rail_contents(&mut self, ui: &mut egui::Ui, reveal: bool, p: UiColors) -> RailAction {
        let mut action = RailAction::default();
        // Lay out at the full sidebar width, anchored to the panel's right edge, so the
        // content keeps its size and slides (clipped) when the panel narrows.
        let panel = ui.max_rect();
        ui.set_clip_rect(panel);
        let full = egui::Rect::from_min_max(
            egui::pos2(panel.right() - self.tab_rail_width + 8.0, panel.top()),
            egui::pos2(panel.right() - 6.0, panel.bottom() - 6.0),
        );
        let top = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), TOP_ROW_H));
        let footer_h = 30.0;
        let footer = egui::Rect::from_min_max(
            egui::pos2(full.left(), full.bottom() - footer_h),
            full.max,
        );
        let list = egui::Rect::from_min_max(
            egui::pos2(full.left(), top.bottom() + 8.0),
            egui::pos2(full.right(), footer.top() - 4.0),
        );

        // Top row: room for the traffic lights, sidebar toggle, and new tab on the right.
        let mut row = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(top)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);
        if cfg!(target_os = "macos") {
            row.add_space(64.0);
        }
        let sidebar_btn = btn_sidebar(&mut row, true);
        if sidebar_btn.clicked() {
            self.sidebar_hidden = true;
            self.cache_dirty = true;
        }
        let mut new_tab_resp = None;
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
            let resp = btn_new_tab(row);
            if resp.clicked() {
                action.add = true;
            }
            new_tab_resp = Some(resp);
        });

        // Register drag area strictly on the empty space between sidebar button and new tab button
        let empty_left = sidebar_btn.rect.right() + 2.0;
        let empty_right = new_tab_resp
            .map(|r| r.rect.left() - 2.0)
            .unwrap_or(top.right() - 30.0);
        if empty_right > empty_left {
            let empty_rect = egui::Rect::from_min_max(
                egui::pos2(empty_left, top.top()),
                egui::pos2(empty_right, top.bottom()),
            );
            window_drag_area(ui, empty_rect, "sidebar_drag");
        }

        let rows: Vec<(u64, String, bool)> = self
            .tabs
            .iter()
            .map(|tab| (tab.id, tab.name.clone(), tab.pinned))
            .collect();
        let active_id = self.tabs[self.active].id;
        let renaming_id = self.renaming.as_ref().map(|rename| rename.id);

        let mut list_ui = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(list)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        egui::ScrollArea::vertical()
            .id_salt("tab_rail_scroll")
            .auto_shrink([false; 2])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(&mut list_ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (index, (id, name, pinned)) in rows.into_iter().enumerate() {
                    if renaming_id == Some(id) {
                        self.rename_row(ui, &mut action);
                    } else {
                        tab_row(
                            ui,
                            index,
                            id,
                            &name,
                            pinned,
                            id == active_id,
                            reveal && id == active_id,
                            &mut action,
                        );
                    }
                }
            });

        let mut footer_ui = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(footer)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        footer_ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
        self.render_settings(&mut footer_ui, p);

        // Invisible resize handle in the sidebar's right margin.
        let clip = ui.clip_rect();
        let handle = egui::Rect::from_min_max(
            egui::pos2(full.right() + 1.0, clip.top() + TOP_ROW_H),
            egui::pos2(clip.right(), clip.bottom()),
        );
        let resp = ui.interact(handle, egui::Id::new("sidebar_resize"), egui::Sense::drag());
        if resp.dragged() {
            self.tab_rail_width += resp.drag_delta().x;
            self.cache_dirty = true;
        }
        if resp.contains_pointer() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }

        ui.allocate_rect(panel, egui::Sense::hover());
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
        let editor_has_focus = ctx.memory(|m| m.has_focus(json_editor_id(tab_id)));
        let for_editor_paste = self.edit_mode || self.editor_editing || editor_has_focus;
        if needs_unescape {
            if let Some((_, unescaped)) = try_unescape_json(&text) {
                text = unescaped;
            } else if is_request_candidate(&text) {
                self.start_request_loading(text, for_editor_paste, ctx);
                ctx.input_mut(|i| {
                    i.events.retain(|e| !matches!(e, egui::Event::Paste(_)));
                });
                return;
            } else if let Some((_, requested)) = try_request_json(&text) {
                text = requested;
            }
        }

        if for_editor_paste {
            // Preserve insertion/selection semantics before formatting the document.
            if !text.is_empty() {
                self.paste_into_editor(ctx, &text);
            }
        } else if !text.trim().is_empty() {
            self.replace_document(text);
            self.editor_editing = false;
            if self.parse_error.is_some() {
                self.set_edit_mode(ctx, true);
            }
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
        self.commit_editor_view(false);

        let max = self.editor_view.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(ccursor.index.min(max)),
            )));
        state.store(ctx, editor_id);
        ctx.memory_mut(|mem| mem.request_focus(editor_id));
        self.editor_editing = true;
    }

    pub(crate) fn set_edit_mode(&mut self, ctx: &egui::Context, on: bool) {
        self.edit_mode = on;
        self.editor_editing = on;
        self.editor_card_rect = egui::Rect::NOTHING;
        let editor_id = json_editor_id(self.id);
        if on {
            ctx.memory_mut(|mem| mem.request_focus(editor_id));
        } else {
            ctx.memory_mut(|mem| mem.surrender_focus(editor_id));
        }
    }

    /// Shows the selected subtree using JSON syntax, with folding for containers.
    fn render_node_card(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect, colors: ThemeColors) {
        let p = colors.ui;
        ui.set_clip_rect(ui.clip_rect().intersect(card_rect));
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(4, 0))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let hint = |ui: &mut egui::Ui, text: &str| {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(tr(ui.ctx(), text)).color(p.faint));
                    });
                };
                let Some(parsed) = self.parsed.take() else {
                    return;
                };
                let Some(path) = self.selected_path.clone() else {
                    self.node_root_path = None;
                    self.parsed = Some(parsed);
                    hint(ui, "Select a node on the left to view it here");
                    return;
                };
                let Some(value) = value_at_path(&parsed, &path) else {
                    self.node_root_path = None;
                    self.parsed = Some(parsed);
                    hint(ui, "This node no longer exists");
                    return;
                };

                let root_changed = self.node_root_path.as_deref() != Some(&path);
                if root_changed {
                    self.node_root_path = Some(path.clone());
                    self.node_collapsed.clear();
                    collapse_beyond_depth(value, &path, 0, 2, &mut self.node_collapsed);
                    self.node_scroll_x = 0.0;
                    self.node_content_width = 0.0;
                    self.node_selected_path = None;
                }

                if !(value.is_object() || value.is_array()) {
                    let color = match value {
                        Value::String(_) => colors.string,
                        Value::Number(_) => colors.number,
                        Value::Bool(_) => colors.boolean,
                        _ => colors.null,
                    };
                    let text = format_node_primitive(value, self.node_string_escaped);
                    self.parsed = Some(parsed);
                    let mut scroll = egui::ScrollArea::vertical()
                        .id_salt(("node_value_scroll", self.id))
                        .drag_to_scroll(false)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false; 2]);
                    if root_changed {
                        scroll = scroll.vertical_scroll_offset(0.0);
                    }
                    scroll.show(ui, |ui| {
                        egui::Frame::default()
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 8,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                // Align the first line with the first row of the left tree.
                                ui.add_space(4.5);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text)
                                            .font(egui::FontId::monospace(13.0))
                                            .line_height(Some(19.5))
                                            .color(color),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                            });
                    });
                    if self.copy_notice_in_node {
                        self.draw_copy_notice(ui, card_rect, p);
                    }
                    return;
                }

                let (action, _) = show_tree_pane(
                    ui,
                    card_rect,
                    colors,
                    TreePane {
                        salt: "node_tree",
                        id: self.id,
                        root: value,
                        root_path: &path,
                        collapsed: &mut self.node_collapsed,
                        scroll_x: &mut self.node_scroll_x,
                        content_width: &mut self.node_content_width,
                        search_query: &self.search_query,
                        search_matches: &self.search_matches,
                        cur_match_path: None,
                        selected_path: self.node_selected_path.as_deref(),
                        indent_step: self.indentation.tree_indent_step(),
                        reveal_match: false,
                        plain_json: true,
                        reset_scroll: root_changed,
                    },
                );
                self.parsed = Some(parsed);
                if let Some(act) = action {
                    self.apply_tree_action(ui.ctx(), act, true);
                }
                if self.copy_notice_in_node {
                    self.draw_copy_notice(ui, card_rect, p);
                }
            });
    }

    /// Transient "Copied …" badge at the top-right of a card.
    fn draw_copy_notice(&mut self, ui: &egui::Ui, card_rect: egui::Rect, p: UiColors) {
        let now = std::time::Instant::now();
        let is_active_notice = self
            .copy_notice
            .as_ref()
            .is_some_and(|(_, t)| now.duration_since(*t).as_secs_f32() < 1.5);

        if !is_active_notice && self.copy_notice.is_some() {
            self.copy_notice = None;
        }
        if is_active_notice {
            ui.ctx().request_repaint();
            if let Some((msg, _)) = &self.copy_notice {
                let green = if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(74, 222, 128)
                } else {
                    egui::Color32::from_rgb(22, 163, 74)
                };
                let badge_text = format!("✓ {}", msg);
                let badge_font = egui::FontId::proportional(11.0);
                let galley = ui.fonts(|f| f.layout_no_wrap(badge_text, badge_font, green));
                let badge_size = egui::vec2(galley.rect.width() + 16.0, 20.0);
                let badge_pos = egui::pos2(
                    card_rect.right() - badge_size.x - 10.0,
                    card_rect.top() + 6.0,
                );
                let badge_rect = egui::Rect::from_min_size(badge_pos, badge_size);

                ui.painter()
                    .rect_filled(badge_rect, egui::CornerRadius::same(10), p.surface);
                ui.painter().rect_stroke(
                    badge_rect,
                    egui::CornerRadius::same(10),
                    egui::Stroke::new(1.0, green),
                    egui::StrokeKind::Inside,
                );
                ui.painter().galley(
                    egui::pos2(
                        badge_rect.left() + 8.0,
                        badge_rect.center().y - galley.rect.height() * 0.5,
                    ),
                    galley,
                    green,
                );
            }
        }
    }

    /// Handles a click or menu action from either tree. The right-hand tree keeps its
    /// own fold state and highlight, so it does not change the left tree's selection.
    fn apply_tree_action(&mut self, ctx: &egui::Context, act: TreeAction, node_panel: bool) {
        let copied = |tab: &mut Self, text: &str, msg: &str| {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(text);
            }
            tab.copy_notice = Some((tr(ctx, msg).to_string(), std::time::Instant::now()));
            tab.copy_notice_in_node = node_panel;
            ctx.request_repaint();
        };
        let select = |tab: &mut Self, path: String| {
            if node_panel {
                tab.node_selected_path = Some(path);
            } else {
                tab.selected_path = Some(path);
            }
        };
        match act {
            TreeAction::Select(path) => select(self, path),
            TreeAction::CopyText(txt) => {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(txt);
                }
            }
            TreeAction::CopyValue(path, val) => {
                select(self, path);
                copied(self, &val, "Copied value");
            }
            TreeAction::CopyKey(path, key) => {
                select(self, path);
                copied(self, &key, "Copied key");
            }
            TreeAction::StartEdit(path, key, val) => self.open_edit_modal(&path, &key, &val),
            TreeAction::DeleteNode(path) => self.delete_node_at(&path),
            TreeAction::ExpandSubtree(path) => {
                let set = if node_panel { &mut self.node_collapsed } else { &mut self.collapsed };
                set.retain(|p| !p.starts_with(&path));
            }
            TreeAction::CollapseSubtree(path, val) => {
                let set = if node_panel { &mut self.node_collapsed } else { &mut self.collapsed };
                collect_all_expandable_paths(&val, &path, set);
            }
            TreeAction::DownloadImage(url) => crate::image_preview::save_image_dialog(&url),
        }
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

        let dark = ctx.theme() == egui::Theme::Dark;
        egui::Area::new(egui::Id::new("edit_modal_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    egui::Color32::from_black_alpha(if dark { 90 } else { 30 }),
                );
                if resp.clicked() {
                    close_modal = true;
                }
            });

        let p = UiColors::from_ctx(ctx);
        // Long or multi-line strings get a wider dialog.
        let roomy = self.edit_modal.as_ref().is_some_and(|m| {
            m.target_type == ValueTypeTag::RawJson
                || m.input_text.contains('\n')
                || m.input_text.chars().count() > 60
        });
        let width = if roomy { 600.0 } else { 480.0 };
        let max_h = (screen.height() - 240.0).clamp(120.0, 420.0);
        // Same look as menus and tooltips (see theme.rs), with roomier padding.
        let frame = egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(20));

        egui::Window::new("edit_value_modal")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -16.0))
            .default_width(width)
            .frame(frame)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                let Some(modal) = &mut self.edit_modal else {
                    return;
                };

                // Header: icon tile, title, path, close.
                ui.horizontal(|ui| {
                    let (tile, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(tile, egui::CornerRadius::same(9), p.blue_bg);
                    draw_lucide_icon(ui.painter(), tile.shrink(9.0), LucideIcon::PencilLine, p.blue);
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.set_width(width - 34.0 - 10.0 - 34.0);
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Edit Value"))
                                .size(15.0)
                                .strong()
                                .color(p.strong),
                        );
                        ui.add_space(3.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&modal.path).monospace().size(11.5).color(p.muted),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&modal.path);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, egui::CornerRadius::same(7), p.hover);
                        }
                        let color = if resp.hovered() { p.strong } else { p.muted };
                        draw_lucide_icon(ui.painter(), rect.shrink(6.0), LucideIcon::X, color);
                        if resp.clicked() {
                            close_modal = true;
                        }
                    });
                });

                ui.add_space(18.0);
                // Key (read-only) and type, side by side.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if !modal.key_label.is_empty() {
                        let key_frame = egui::Frame::default()
                            .fill(p.subtle)
                            .stroke(egui::Stroke::new(1.0, p.border))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin::symmetric(10, 6));
                        key_frame.show(ui, |ui| {
                            ui.set_max_width(width * 0.4);
                            draw_lucide_icon(
                                ui.painter(),
                                egui::Rect::from_center_size(
                                    ui.cursor().min + egui::vec2(6.0, 9.0),
                                    egui::vec2(12.0, 12.0),
                                ),
                                LucideIcon::Key,
                                p.faint,
                            );
                            ui.add_space(18.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&modal.key_label).monospace().size(12.5).color(p.text),
                                )
                                .truncate(),
                            );
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let choices = [
                            (ValueTypeTag::String, "String"),
                            (ValueTypeTag::Number, "Number"),
                            (ValueTypeTag::Bool, "Bool"),
                            (ValueTypeTag::Null, "Null"),
                            (ValueTypeTag::RawJson, "JSON"),
                        ];
                        let labels: Vec<&str> = choices.iter().map(|(_, l)| *l).collect();
                        let selected = choices
                            .iter()
                            .position(|(t, _)| *t == modal.target_type)
                            .unwrap_or(0);
                        // Right-to-left layout: allocate the whole control as one widget.
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if let Some(i) = segmented(ui, &labels, selected) {
                                let tag = choices[i].0;
                                if tag != modal.target_type {
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
                    });
                });

                ui.add_space(14.0);
                let input_id = egui::Id::new("edit_modal_input");
                let focused = ui.memory(|m| m.has_focus(input_id));
                let stroke = if modal.error.is_some() {
                    egui::Stroke::new(1.5, p.danger)
                } else if focused {
                    egui::Stroke::new(1.5, p.blue)
                } else {
                    egui::Stroke::new(1.0, p.border)
                };
                let field = egui::Frame::default()
                    .fill(if focused { p.surface } else { p.subtle })
                    .stroke(stroke)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(12, 10));
                field.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match modal.target_type {
                        ValueTypeTag::Bool => {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if type_chip(ui, "true", modal.input_text == "true").clicked() {
                                    modal.input_text = "true".to_string();
                                    modal.error = None;
                                }
                                if type_chip(ui, "false", modal.input_text == "false").clicked() {
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
                        ValueTypeTag::Number => {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut modal.input_text)
                                    .id(input_id)
                                    .font(egui::FontId::monospace(13.0))
                                    .desired_width(f32::INFINITY)
                                    .frame(false),
                            );
                            if !resp.has_focus() {
                                resp.request_focus();
                            }
                            if resp.changed() {
                                modal.error = None;
                            }
                            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                save_modal = true;
                            }
                        }
                        ValueTypeTag::String | ValueTypeTag::RawJson => {
                            // Multi-line text area: wraps long text, shows real line breaks
                            // and tabs, and scrolls once it gets tall.
                            let lines = modal.input_text.lines().count().max(1);
                            let rows = if modal.target_type == ValueTypeTag::RawJson {
                                lines.clamp(6, 18)
                            } else {
                                lines.clamp(3, 14)
                            };
                            egui::ScrollArea::vertical()
                                .id_salt("edit_modal_scroll")
                                .max_height(max_h)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    let resp = ui.add(
                                        egui::TextEdit::multiline(&mut modal.input_text)
                                            .id(input_id)
                                            .font(egui::FontId::monospace(13.0))
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(rows)
                                            .lock_focus(true)
                                            .frame(false),
                                    );
                                    if !resp.has_focus() && !resp.lost_focus() {
                                        resp.request_focus();
                                    }
                                    if resp.changed() {
                                        modal.error = None;
                                    }
                                });
                        }
                    }
                });

                if let Some(err) = &modal.error {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(err).size(12.0).color(p.danger));
                }

                ui.add_space(16.0);
                // Footer: size and shortcuts on the left, actions on the right.
                ui.horizontal(|ui| {
                    let multiline = matches!(modal.target_type, ValueTypeTag::String | ValueTypeTag::RawJson);
                    let mut info = String::new();
                    if multiline {
                        let chars = modal.input_text.chars().count();
                        let lines = modal.input_text.lines().count().max(1);
                        info = format!("{} {} · {} {}   ", chars, tr(ui.ctx(), "chars"), lines, tr(ui.ctx(), "lines"));
                    }
                    let keys = if multiline { "⌘↩ save · esc cancel" } else { "↩ save · esc cancel" };
                    ui.label(
                        egui::RichText::new(format!("{}{}", info, tr(ui.ctx(), keys)))
                            .size(11.0)
                            .color(p.faint),
                    );
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
    /// Compare mode's per-editor toolbar.
    fn render_editor_toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        self.toolbar_reshape(ui);
        toolbar_separator(ui);
        self.toolbar_history(ui);
        toolbar_separator(ui);
        self.toolbar_output(ui);
        let compare = btn_compare(ui).clicked();
        toolbar_separator(ui);
        self.toolbar_clear(ui);
        compare
    }

    /// Buttons that rewrite the document's shape.
    /// Focuses the search box and selects its text.
    fn focus_search(&mut self, ctx: &egui::Context) {
        let search_id = search_input_id(self.id);
        let editor_id = json_editor_id(self.id);
        ctx.memory_mut(|mem| {
            mem.surrender_focus(editor_id);
            mem.request_focus(search_id);
        });
        self.editor_editing = false;
        let mut state = egui::text_edit::TextEditState::load(ctx, search_id).unwrap_or_default();
        let len = self.search_query.chars().count();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(len),
        )));
        state.store(ctx, search_id);
    }

    /// Original -> A→Z -> Z→A -> original.
    fn cycle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::None => SortMode::Asc,
            SortMode::Asc => SortMode::Desc,
            SortMode::Desc => SortMode::None,
        };
        self.apply_sort();
    }

    fn copy_json(&mut self, ctx: &egui::Context) {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if cb.set_text(self.input.clone()).is_ok() {
                self.copy_notice = Some((tr(ctx, "Copied JSON").to_string(), std::time::Instant::now()));
                self.copied_full_at = Some(std::time::Instant::now());
                self.copy_notice_in_node = false;
            }
        }
    }

    fn copy_minified_with_notice(&mut self, ctx: &egui::Context) {
        if self.copy_minified() {
            self.copy_notice = Some((
                tr(ctx, "Copied minified JSON").to_string(),
                std::time::Instant::now(),
            ));
            self.copied_minified_at = Some(std::time::Instant::now());
            self.copy_notice_in_node = false;
        }
    }

    fn toolbar_reshape(&mut self, ui: &mut egui::Ui) {
        if btn_format(ui).clicked() {
            self.format_json(ui.ctx());
        }
        if btn_sort(ui, self.sort_mode).clicked() {
            self.cycle_sort();
        }
        if btn_simplify(ui, self.parsed.is_some()).clicked() {
            self.simplify_json();
        }
    }

    fn toolbar_history(&mut self, ui: &mut egui::Ui) {
        if btn_undo(ui, self.can_undo()).clicked() {
            self.undo_edit();
        }
        if btn_redo(ui, self.can_redo()).clicked() {
            self.redo_edit();
        }
    }

    /// Buttons that copy the document out.
    fn toolbar_output(&mut self, ui: &mut egui::Ui) {
        let now = std::time::Instant::now();
        let copied_full = self
            .copied_full_at
            .as_ref()
            .is_some_and(|t| now.duration_since(*t).as_secs_f32() < 1.5);
        let copied_min = self
            .copied_minified_at
            .as_ref()
            .is_some_and(|t| now.duration_since(*t).as_secs_f32() < 1.5);
        if copied_full || copied_min {
            ui.ctx().request_repaint();
        }
        if btn_copy(ui, copied_full).clicked() {
            self.copy_json(ui.ctx());
        }
        if btn_compress(ui, copied_min).clicked() {
            self.copy_minified_with_notice(ui.ctx());
        }
    }

    fn toolbar_clear(&mut self, ui: &mut egui::Ui) {
        if btn_clear(ui).clicked() {
            self.clear_document();
        }
    }

    /// Search pill: icon, query, match count with previous/next, and clear.
    fn render_search_box(&mut self, ui: &mut egui::Ui, pill_w: f32, p: UiColors) {
        let search_id = search_input_id(self.id);
        let focused = ui.memory(|mem| mem.has_focus(search_id));
        let (rect, bg_resp) =
            ui.allocate_exact_size(egui::vec2(pill_w, 26.0), egui::Sense::click());
        if bg_resp.clicked() {
            ui.memory_mut(|mem| mem.request_focus(search_id));
        }
        let hovered = ui.rect_contains_pointer(rect);
        let (fill, stroke) = if focused {
            (p.surface, egui::Stroke::new(1.5, p.blue))
        } else if hovered {
            (p.surface, egui::Stroke::new(1.0, p.blue_border))
        } else {
            (p.subtle, egui::Stroke::new(1.0, p.border))
        };
        ui.painter().rect_filled(rect, egui::CornerRadius::same(13), fill);
        ui.painter()
            .rect_stroke(rect, egui::CornerRadius::same(13), stroke, egui::StrokeKind::Inside);

        let mut row = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(rect.shrink2(egui::vec2(10.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing = egui::vec2(5.0, 0.0);

        let (icon_rect, _) = row.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        draw_lucide_icon(
            row.painter(),
            icon_rect,
            LucideIcon::Search,
            if focused { p.blue } else { p.muted },
        );

        // Consume navigation before TextEdit handles Enter or moves its cursor.
        let (mut prev, mut next) = if focused {
            row.input_mut(|i| {
                let prev = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                    || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter);
                let next = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                (prev, next)
            })
        } else {
            (false, false)
        };

        // Size the text field so the trailing controls always fit inside the pill.
        if self.edit_mode {
            self.refresh_editor_hits();
        }
        let (match_count, match_index) = self.search_status();
        let has_query = !self.search_query.is_empty();
        let has_matches = match_count > 0;
        let count_font = egui::FontId::proportional(11.0);
        let count_text = has_query.then(|| {
            if has_matches {
                format!("{}/{}", match_index + 1, match_count)
            } else {
                tr(row.ctx(), "No matches").to_string()
            }
        });
        let gap = row.spacing().item_spacing.x;
        let count_w = count_text.as_ref().map_or(0.0, |t| {
            row.fonts(|f| f.layout_no_wrap(t.clone(), count_font.clone(), p.muted).size().x) + gap
        });
        let nav_w = if has_matches { 2.0 * (18.0 + gap) } else { 0.0 };
        let clear_w = if has_query { 18.0 + gap } else { 0.0 };
        let text_w = (row.available_width() - count_w - nav_w - clear_w - gap).max(30.0);

        let search_resp = row.add(
            egui::TextEdit::singleline(&mut self.search_query)
                .id(search_id)
                .hint_text(
                    egui::RichText::new(tr(row.ctx(), "Search keys and values")).color(p.faint),
                )
                .desired_width(text_w)
                .margin(egui::Margin::ZERO)
                .frame(false),
        );
        if search_resp.changed() {
            self.update_search();
        }

        if let Some(text) = count_text {
            row.label(
                egui::RichText::new(text)
                    .font(count_font)
                    .color(if has_matches { p.muted } else { p.danger }),
            );
        }
        if has_matches {
            prev |= mini_icon_button(&mut row, LucideIcon::ChevronUp, "Previous match").clicked();
            next |= mini_icon_button(&mut row, LucideIcon::ChevronDown, "Next match").clicked();
        }
        if has_query && mini_icon_button(&mut row, LucideIcon::X, "Clear search").clicked() {
            self.search_query.clear();
            self.update_search();
            row.memory_mut(|mem| mem.request_focus(search_id));
        }

        if prev {
            self.search_prev();
        } else if next {
            self.search_next();
        }
    }

    /// Edit mode: tints every search match in the editor, marks the current one more
    /// strongly, and returns the current match's rect when it should be scrolled to.
    fn paint_editor_hits(
        &mut self,
        ui: &mut egui::Ui,
        galley: &std::sync::Arc<egui::Galley>,
        text_rect: egui::Rect,
        visible_w: f32,
        max_scroll_x: f32,
    ) -> Option<egui::Rect> {
        self.refresh_editor_hits();
        if self.editor_hits.is_empty() {
            self.reveal_editor_hit = false;
            return None;
        }
        let dark = ui.visuals().dark_mode;
        let (fill, current_fill) = if dark {
            (egui::Color32::from_rgba_unmultiplied(250, 204, 21, 50), egui::Color32::from_rgba_unmultiplied(249, 115, 22, 120))
        } else {
            (egui::Color32::from_rgba_unmultiplied(250, 204, 21, 90), egui::Color32::from_rgba_unmultiplied(249, 115, 22, 110))
        };
        let clip = ui.clip_rect();
        let hit_rect = |start: usize, end: usize| {
            let a = galley.pos_from_cursor(&galley.from_ccursor(egui::text::CCursor::new(start)));
            let b = galley.pos_from_cursor(&galley.from_ccursor(egui::text::CCursor::new(end)));
            egui::Rect::from_min_max(
                text_rect.min + a.min.to_vec2(),
                text_rect.min + egui::vec2(b.max.x.max(a.min.x + 2.0), a.max.y),
            )
        };
        // Matches are sorted by position, so skip straight to the visible rows.
        let top_char = galley
            .cursor_from_pos(egui::vec2(0.0, clip.top() - text_rect.top()))
            .ccursor
            .index;
        let first = self.editor_hits.partition_point(|&(_, end)| end < top_char);
        for (i, &(start, end)) in self.editor_hits.iter().enumerate().skip(first) {
            let rect = hit_rect(start, end);
            if rect.top() > clip.bottom() {
                break;
            }
            let color = if i == self.editor_hit_index { current_fill } else { fill };
            ui.painter().rect_filled(rect.expand2(egui::vec2(1.0, 0.0)), 3.0, color);
        }
        if std::mem::take(&mut self.reveal_editor_hit) {
            let (start, end) = self.editor_hits[self.editor_hit_index];
            let rect = hit_rect(start, end);
            // Horizontal offset is ours to drive; the vertical area scrolls itself.
            let x = rect.left() - text_rect.left();
            if x < self.editor_scroll_x || x + rect.width() > self.editor_scroll_x + visible_w {
                self.editor_scroll_x = (x - visible_w * 0.3).clamp(0.0, max_scroll_x.max(0.0));
            }
            ui.ctx().request_repaint();
            return Some(rect);
        }
        None
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
            .inner_margin(egui::Margin::symmetric(4, 0));

        card_frame.show(ui, |ui| {
            // Digits follow the source, so folding does not make the gutter jump.
            let source_line_count = self.editor_map.last().map_or(1, |l| l.source_end + 1);
            let digits = source_line_count.to_string().len().max(2);
            let fold_w = 16.0;

            // Relaxed line height (19.5pt) and letter spacing (0.15pt).
            // Use the galley's pixel-rounded row step so the gutter does not drift
            // away from the text as the document gets long.
            let requested_line_h = 19.5;
            let font_id = egui::FontId::monospace(13.0);
            let line_height_val = editor_row_height(ui, &font_id, requested_line_h);

            let digit_w = ui.fonts(|f| f.glyph_width(&font_id, '0')).max(7.2);

            let measured_content_w = std::rc::Rc::new(std::cell::Cell::new(None));
            let last_galley = std::rc::Rc::new(std::cell::RefCell::new(None));
            let galley_cache =
                std::rc::Rc::new(std::cell::RefCell::new(self.editor_galley_cache.take()));
            let mcw = measured_content_w.clone();
            let lg = last_galley.clone();
            let gc = galley_cache.clone();
            let layout_font_id = font_id.clone();
            let glyph_width = digit_w;
            let mut code_layouter = move |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                // TextEdit calls this every frame; re-highlighting a large document each
                // time is what made big pastes freeze, so reuse the last layout.
                let pixels_per_point = ui.ctx().pixels_per_point();
                let mut cache = gc.borrow_mut();
                let hit = cache.as_ref().is_some_and(|c: &EditorGalleyCache| {
                    c.colors == colors
                        && c.line_height == line_height_val
                        && c.glyph_width == glyph_width
                        && c.pixels_per_point == pixels_per_point
                        && c.text == text
                });
                if !hit {
                    let job = highlight_json(text, layout_font_id.clone(), line_height_val, colors);
                    *cache = Some(EditorGalleyCache {
                        text: text.to_owned(),
                        colors,
                        line_height: line_height_val,
                        glyph_width,
                        pixels_per_point,
                        galley: ui.fonts(|f| f.layout_job(job)),
                    });
                }
                let galley = cache.as_ref().unwrap().galley.clone();
                mcw.set(Some(galley.rect.width()));
                *lg.borrow_mut() = Some(galley.clone());
                galley
            };

            let gutter_width = (digits as f32 * digit_w + 12.0).max(28.0);
            let visible_editor_w = (card_rect.width() - gutter_width - fold_w - 22.0).max(100.0);
            let folded_now = self.editor_folds.clone();

            // Content width: use the measured width once known; estimate only on the first
            // frame, since scanning a large document every frame is slow.
            let approx_char_w = digit_w;
            let approx_w = if self.editor_content_width > 0.0 {
                0.0
            } else {
                self.editor_view
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
                .unwrap_or(0.0)
            };
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
            let error_bar_h = if self.parse_error.is_some() && !self.input.trim().is_empty() {
                36.0
            } else {
                0.0
            };
            let available_h = ui.available_height();
            let sa_h = (available_h - scrollbar_h - error_bar_h).max(30.0);

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

            // A search match to center. The request must be made outside the inner
            // horizontal area, which would otherwise swallow the vertical part of it.
            let mut reveal_rect: Option<egui::Rect> = None;
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
                if let Some(err) = &self.parse_error {
                    if err.line > 0 && !self.input.trim().is_empty() {
                        let err_line_idx = err.line - 1;
                        let font_h = ui.fonts(|f| f.row_height(&font_id));
                        let top = ui.cursor().top() - ((line_height_val - font_h) * 0.5).max(0.0);
                        if let Some(row_idx) = self.editor_map.iter().position(|l| l.source_start <= err_line_idx && err_line_idx <= l.source_end) {
                            let err_rect = egui::Rect::from_min_size(
                                egui::pos2(card_rect.left() + 4.0, top + row_idx as f32 * line_height_val),
                                egui::vec2(card_rect.width() - 8.0, line_height_val),
                            );
                            let err_bg = if ui.visuals().dark_mode {
                                egui::Color32::from_rgba_unmultiplied(239, 68, 68, 35)
                            } else {
                                egui::Color32::from_rgba_unmultiplied(239, 68, 68, 25)
                            };
                            ui.painter().rect_filled(err_rect, egui::CornerRadius::same(3), err_bg);
                        }
                    }
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    // Only rows inside the viewport get fold arrows and line numbers;
                    // building widgets for every row froze the app on large documents.
                    let visible_lines = self.editor_map.len().max(1);
                    let clip = ui.clip_rect();
                    let row_count = self.editor_map.len();
                    let visible_rows = move |top: f32| {
                        let first = ((clip.top() - top) / line_height_val).floor().max(0.0) as usize;
                        let end = (((clip.bottom() - top) / line_height_val).ceil().max(0.0)
                            as usize)
                            .min(row_count);
                        first..end.max(first)
                    };

                    let number_galley = |ui: &egui::Ui, n: usize| {
                        let mut job = egui::text::LayoutJob::single_section(
                            format!("{:>width$}", n, width = digits),
                            egui::TextFormat {
                                font_id: font_id.clone(),
                                color: p.muted,
                                line_height: Some(line_height_val),
                                ..Default::default()
                            },
                        );
                        job.wrap.max_width = f32::INFINITY;
                        ui.painter().layout_job(job)
                    };
                    // Every number is padded to the same width, so one sample sizes the column.
                    let numbers_w = number_galley(ui, 1).size().x;
                    let gutter_h = visible_lines as f32 * line_height_val;
                    ui.allocate_ui_with_layout(
                        egui::vec2(gutter_width, gutter_h),
                        egui::Layout::top_down(egui::Align::RIGHT),
                        |ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(numbers_w, gutter_h),
                                egui::Sense::hover(),
                            );
                            for row in visible_rows(rect.top()) {
                                let line = self.editor_map[row];
                                let is_err_line = self.parse_error.as_ref().map_or(false, |e| e.line == line.source_start + 1 && !self.input.trim().is_empty());
                                let galley = if is_err_line {
                                    let mut job = egui::text::LayoutJob::single_section(
                                        format!("{:>width$}", line.source_start + 1, width = digits),
                                        egui::TextFormat {
                                            font_id: font_id.clone(),
                                            color: p.danger,
                                            line_height: Some(line_height_val),
                                            ..Default::default()
                                        },
                                    );
                                    job.wrap.max_width = f32::INFINITY;
                                    ui.painter().layout_job(job)
                                } else {
                                    number_galley(ui, line.source_start + 1)
                                };
                                let pos = egui::pos2(
                                    rect.right() - galley.size().x,
                                    rect.top() + row as f32 * line_height_val,
                                );
                                let color = if is_err_line { p.danger } else { p.muted };
                                ui.painter().galley(pos, galley, color);
                            }
                        },
                    );

                    let mut clicked_fold = None;
                    let (fold_rect, _) =
                        ui.allocate_exact_size(egui::vec2(fold_w, gutter_h), egui::Sense::hover());
                    for row in visible_rows(fold_rect.top()) {
                        let start = self.editor_map[row].source_start;
                        if !self.editor_fold_starts.contains(&start) {
                            continue;
                        }
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(fold_rect.left(), fold_rect.top() + row as f32 * line_height_val),
                            egui::vec2(fold_w, line_height_val),
                        );
                        let resp = ui.interact(
                            rect,
                            egui::Id::new(("editor_fold", editor_scroll_id, start)),
                            egui::Sense::click(),
                        );
                        let collapsed = folded_now.contains(&start);
                        let color = if resp.hovered() {
                            p.strong
                        } else {
                            p.faint
                        };
                        draw_fold_chevron(ui.painter(), rect.center(), collapsed, color);
                        let tip = if collapsed { tr(ui.ctx(), "Expand") } else { tr(ui.ctx(), "Collapse") };
                        if resp.clicked() {
                            clicked_fold = Some(start);
                        }
                        resp.on_hover_text(tip)
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                    if let Some(start) = clicked_fold {
                        self.toggle_editor_fold(start);
                    }

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
                            if self.edit_mode {
                                if let Some(galley) = last_galley.borrow().clone() {
                                    reveal_rect = self.paint_editor_hits(
                                        ui,
                                        &galley,
                                        resp.rect,
                                        visible_editor_w,
                                        max_scroll_x,
                                    );
                                }
                            }
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
                if let Some(rect) = reveal_rect {
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
            });
            self.editor_scroll_y = vert_out.state.offset.y;
            self.editor_scroll_max_y =
                (vert_out.content_size.y - vert_out.inner_rect.height()).max(0.0);

            if let Some(w) = measured_content_w.get() {
                self.editor_content_width = w;
            }
            self.editor_galley_cache = galley_cache.borrow_mut().take();

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

            if let Some(err) = &self.parse_error {
                if !self.input.trim().is_empty() {
                    ui.add_space(3.0);
                    let bar_h = 28.0;
                    let (bar_rect, bar_resp) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), bar_h),
                        egui::Sense::click(),
                    );

                    let bar_bg = if ui.visuals().dark_mode {
                        egui::Color32::from_rgba_unmultiplied(239, 68, 68, 24)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(254, 242, 242, 255)
                    };
                    let border_stroke = egui::Stroke::new(1.0, p.danger_border);
                    ui.painter().rect_filled(bar_rect, egui::CornerRadius::same(6), bar_bg);
                    ui.painter().rect_stroke(bar_rect, egui::CornerRadius::same(6), border_stroke, egui::StrokeKind::Inside);

                    let mut bar_ui = ui.new_child(
                        egui::UiBuilder::default()
                            .max_rect(bar_rect.shrink2(egui::vec2(8.0, 2.0)))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    bar_ui.spacing_mut().item_spacing.x = 6.0;

                    bar_ui.label(
                        egui::RichText::new("⚠")
                            .size(13.0)
                            .strong()
                            .color(p.danger),
                    );

                    bar_ui.label(
                        egui::RichText::new(tr(bar_ui.ctx(), "Syntax Error"))
                            .size(12.0)
                            .strong()
                            .color(p.danger),
                    );

                    let loc_text = format!("Ln {}, Col {}", err.line, err.column);
                    let pill_resp = bar_ui.add(
                        egui::Button::new(
                            egui::RichText::new(loc_text)
                                .size(11.0)
                                .monospace()
                                .color(p.danger),
                        )
                        .fill(p.subtle)
                        .stroke(egui::Stroke::new(1.0, p.danger_border))
                        .corner_radius(egui::CornerRadius::same(4))
                    ).on_hover_cursor(egui::CursorIcon::PointingHand)
                     .on_hover_text(tr(bar_ui.ctx(), "Click to jump to error"));

                    let err_msg = err.message.clone();
                    let err_line = err.line;
                    let err_col = err.column;

                    if pill_resp.clicked() || bar_resp.clicked() {
                        let target_line = err_line.saturating_sub(1);
                        let mut char_idx = 0;
                        for (i, line_str) in self.editor_view.lines().enumerate() {
                            if i == target_line {
                                let col = err_col.saturating_sub(1);
                                char_idx += col.min(line_str.chars().count());
                                break;
                            }
                            char_idx += line_str.chars().count() + 1;
                        }
                        let editor_id = json_editor_id(self.id);
                        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), editor_id).unwrap_or_default();
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(char_idx),
                        )));
                        state.store(ui.ctx(), editor_id);
                        ui.ctx().memory_mut(|mem| mem.request_focus(editor_id));
                        self.editor_editing = true;

                        let row_idx = self.editor_map.iter().position(|l| l.source_start <= target_line && target_line <= l.source_end).unwrap_or(target_line);
                        let target_y = row_idx as f32 * line_height_val;
                        let desired_scroll_y = (target_y - sa_h * 0.4).max(0.0);
                        self.editor_sync_scroll = self.editor_scroll_y - desired_scroll_y;
                        ui.ctx().request_repaint();
                    }

                    bar_ui.label(
                        egui::RichText::new(&err_msg)
                            .size(11.5)
                            .monospace()
                            .color(p.strong),
                    );

                    let snippet_opt = extract_snippet(&self.input, err_line, err_col);
                    bar_resp.on_hover_ui(|ui| {
                        ui.set_max_width(450.0);
                        ui.label(egui::RichText::new(&err_msg).size(12.0).strong().color(p.danger));
                        if let Some(snippet) = &snippet_opt {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(snippet).size(11.0).monospace().color(p.text));
                        }
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(tr(ui.ctx(), "Click to jump to error")).size(11.0).color(p.faint));
                    });
                }
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
                            let target = if loading.target_url.chars().count() > 34 {
                                format!("{}...", loading.target_url.chars().take(31).collect::<String>())
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
                    if self.parse_error.is_some() {
                        self.set_edit_mode(ctx, true);
                    }
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
                    if self.parse_error.is_some() {
                        self.set_edit_mode(ctx, true);
                    }
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
    fn render_tree_syntax_error(&self, ui: &mut egui::Ui, err: &ParseError, colors: ThemeColors) -> bool {
        let p = colors.ui;
        ui.add_space(10.0);
        let mut switch_clicked = false;

        ui.vertical(|ui| {
            ui.set_width(ui.available_width());

            // 1. Header: Left-aligned Icon + Title + Location Badge
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                // Subtle Warning Icon
                ui.label(
                    egui::RichText::new("⚠")
                        .size(14.0)
                        .strong()
                        .color(p.danger),
                );

                // Title
                ui.label(
                    egui::RichText::new(tr(ui.ctx(), "Syntax Error"))
                        .size(13.0)
                        .strong()
                        .color(p.strong),
                );

                // Location Pill Badge: Ln X, Col Y
                let pill_frame = egui::Frame::default()
                    .fill(p.subtle)
                    .stroke(egui::Stroke::new(1.0, p.danger_border))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(7, 2));

                pill_frame.show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Ln {}, Col {}", err.line, err.column))
                            .size(11.0)
                            .monospace()
                            .color(p.danger),
                    );
                });
            });

            ui.add_space(8.0);

            // 2. Error Message Box (subtle background, clean border, no red fill)
            let msg_frame = egui::Frame::default()
                .fill(p.subtle)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8));

            msg_frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(&err.message)
                            .size(12.0)
                            .monospace()
                            .color(p.strong),
                    );
                });
            });

            // 3. Code context snippet preview
            if let Some(snippet) = extract_snippet(&self.input, err.line, err.column) {
                ui.add_space(8.0);
                let snippet_frame = egui::Frame::default()
                    .fill(p.subtle)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(10, 8));

                snippet_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(snippet)
                            .size(11.5)
                            .monospace()
                            .line_height(Some(16.0))
                            .color(p.text),
                    );
                });
            }

            ui.add_space(8.0);

            // 4. Subtle Hint Footer & Action Button
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "💡 {}",
                        tr(
                            ui.ctx(),
                            "Fix the syntax error in edit mode to restore the tree view."
                        )
                    ))
                    .size(11.0)
                    .color(p.faint),
                );
            });
            ui.add_space(8.0);
            if ui.button(
                egui::RichText::new(format!("✏ {}", tr(ui.ctx(), "Switch to Edit Mode")))
                    .size(12.0)
                    .color(p.blue),
            ).clicked() {
                switch_clicked = true;
            }
        });
        switch_clicked
    }

    fn render_tree_card(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect, colors: ThemeColors) {
        let p = colors.ui;
        ui.set_clip_rect(ui.clip_rect().intersect(card_rect));
        let card_frame = egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(4, 0));

        // Remember how many tree rows fit, to decide how far a new paste expands.
        self.tree_fit_rows = ((card_rect.height() / TREE_ROW_HEIGHT).floor() as usize).max(1);

        card_frame.show(ui, |ui| {
            if let Some(err) = &self.parse_error {
                if !self.input.trim().is_empty() {
                    if self.render_tree_syntax_error(ui, err, colors) {
                        self.set_edit_mode(ui.ctx(), true);
                    }
                    ui.allocate_space(ui.available_size());
                    return;
                }
            }

            if self.parsed.is_none() {
                if self.request_loading.is_some() {
                    // The response replaces this card as soon as it arrives.
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(tr(ui.ctx(), "Loading JSON from request..."))
                                .color(p.faint),
                        );
                    });
                    return;
                }
                // Nothing loaded yet: show a minimal, box-free welcome view.
                let rows = [
                    IntroRow::tip(LucideIcon::Braces, "Paste JSON from the clipboard"),
                    IntroRow::tip(LucideIcon::Download, "Drop a JSON file onto the window"),
                    IntroRow::tip(
                        LucideIcon::ExternalLink,
                        "Paste a URL or cURL command to load its response",
                    ),
                ];
                intro_panel(
                    ui,
                    p,
                    "welcome",
                    "IJson",
                    "A fast, native JSON viewer and editor",
                    &rows,
                );
                return;
            }

            // Borrow the document for this frame instead of deep-cloning it.
            let parsed = self.parsed.take().unwrap();
            let search_query = self.search_query.clone();
            let search_matches = self.search_matches.clone();
            let cur_match_path = self.search_matches.get(self.search_index).cloned();
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

            let tab: &mut JsonTab = self;
            let (action, revealed) = show_tree_pane(
                ui,
                card_rect,
                colors,
                TreePane {
                    salt: "tree",
                    id: tab.id,
                    root: &parsed,
                    root_path: "$",
                    collapsed: &mut tab.collapsed,
                    scroll_x: &mut tab.tree_scroll_x,
                    content_width: &mut tab.tree_content_width,
                    search_query: &search_query,
                    search_matches: &search_matches,
                    cur_match_path: cur_match_path.as_deref(),
                    selected_path: selected_path.as_deref(),
                    indent_step: tab.indentation.tree_indent_step(),
                    reveal_match,
                    plain_json: false,
                    reset_scroll: false,
                },
            );
            if revealed {
                tab.reveal_search_match = false;
            }
            self.parsed = Some(parsed);

            if let Some(act) = action {
                self.apply_tree_action(ui.ctx(), act, false);
            }

            if !self.copy_notice_in_node {
                self.draw_copy_notice(ui, card_rect, p);
            }
        });
    }

}

/// One line of an intro panel: an icon and a short description.
struct IntroRow {
    icon: LucideIcon,
    text: &'static str,
}

impl IntroRow {
    fn tip(icon: LucideIcon, text: &'static str) -> Self {
        Self { icon, text }
    }
}

/// Fills an empty tab with a left-aligned, box-free hero header and borderless hint list.
fn intro_panel(
    ui: &mut egui::Ui,
    p: UiColors,
    salt: &'static str,
    title: &str,
    subtitle: &str,
    rows: &[IntroRow],
) {
    let avail = ui.available_size();
    let width = (avail.x - 48.0).clamp(200.0, 400.0);
    let height_id = egui::Id::new(("intro_height", salt));
    let last_h = ui.ctx().data(|d| d.get_temp::<f32>(height_id)).unwrap_or(0.0);

    egui::ScrollArea::vertical()
        .id_salt(("intro_scroll", salt))
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(((avail.y - last_h) * 0.38).max(28.0));
            let side = ((ui.available_width() - width) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                let block = ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;

                    // Left-aligned hero: app icon on the left, title and subtitle on the right.
                    egui::Frame::default()
                        .inner_margin(egui::Margin::symmetric(4, 0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 12.0;
                                let logo = JsonViewerApp::app_logo_texture(ui.ctx());
                                let (icon_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(48.0, 48.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().add(
                                    egui::epaint::RectShape::filled(
                                        icon_rect,
                                        egui::CornerRadius::ZERO,
                                        egui::Color32::WHITE,
                                    )
                                    .with_texture(
                                        logo.id(),
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                    ),
                                );
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.add_space(2.0);
                                    ui.label(
                                        egui::RichText::new(tr(ui.ctx(), title))
                                            .size(22.0)
                                            .strong()
                                            .color(p.strong),
                                    );
                                    ui.add_space(3.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(tr(ui.ctx(), subtitle))
                                                .size(13.0)
                                                .color(p.muted),
                                        )
                                        .wrap(),
                                    );
                                });
                            });
                        });

                    ui.add_space(22.0);

                    // Open, borderless hint list.
                    for row in rows {
                        intro_row(ui, p, row);
                    }
                });
                ui.ctx()
                    .data_mut(|d| d.insert_temp(height_id, block.response.rect.height()));
            });
            ui.add_space(28.0);
        });
}

fn intro_row(ui: &mut egui::Ui, p: UiColors, row: &IntroRow) {
    egui::Frame::default()
        .inner_margin(egui::Margin::symmetric(4, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(22.0);
                ui.spacing_mut().item_spacing.x = 10.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
                draw_lucide_icon(ui.painter(), icon_rect, row.icon, p.muted);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr(ui.ctx(), row.text))
                            .size(13.0)
                            .color(p.text),
                    )
                    .wrap(),
                );
            });
        });
}

/// Per-panel state for a scrollable JSON tree. Both View-mode panels use this so
/// they scroll and respond to gestures the same way.
struct TreePane<'a> {
    salt: &'static str,
    id: u64,
    root: &'a Value,
    root_path: &'a str,
    collapsed: &'a mut HashSet<String>,
    scroll_x: &'a mut f32,
    content_width: &'a mut f32,
    search_query: &'a str,
    search_matches: &'a [String],
    cur_match_path: Option<&'a str>,
    selected_path: Option<&'a str>,
    indent_step: f32,
    reveal_match: bool,
    plain_json: bool,
    reset_scroll: bool,
}

/// Draws a tree with vertical scrolling and a custom horizontal scrollbar.
/// Returns the row action, and whether the current search match was scrolled into view.
fn show_tree_pane(
    ui: &mut egui::Ui,
    card_rect: egui::Rect,
    colors: ThemeColors,
    pane: TreePane,
) -> (Option<TreeAction>, bool) {
    let p = colors.ui;
    let mut action: Option<TreeAction> = None;
    let mut revealed = false;
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
        let text_exceeds_width = *pane.content_width > visible_w + 2.0;
        let scroll_content_w = if text_exceeds_width {
            *pane.content_width + 16.0
        } else {
            visible_w
        };
        let max_scroll_x = if text_exceeds_width {
            scroll_content_w - visible_w
        } else {
            0.0
        };

        if !text_exceeds_width {
            *pane.scroll_x = 0.0;
        } else if is_over_tree && manual_scroll_x != 0.0 {
            *pane.scroll_x =
                (*pane.scroll_x - manual_scroll_x).clamp(0.0, max_scroll_x);
        } else {
            *pane.scroll_x = (*pane.scroll_x).clamp(0.0, max_scroll_x);
        }

        // Keep the full horizontal track inside the clipped pane.
        let scrollbar_h = if text_exceeds_width {
            10.0 + ui.spacing().item_spacing.y
        } else {
            0.0
        };
        let available_h = ui.available_height();
        let sa_h = (available_h - scrollbar_h).max(30.0);

        let mut max_row_w = 0.0f32;
        let mut current_match_rect = None;

        // IDEA-style fold gutter: reveal expanded fold arrows only while hovering
        // the panel's left edge; collapsed nodes always keep their arrow visible.
        let gutter_rect = egui::Rect::from_min_max(
            card_rect.min,
            egui::pos2(
                (card_rect.left() + 4.0 + crate::tree::GUTTER_W + 4.0).min(card_rect.right()),
                card_rect.bottom(),
            ),
        );
        let gutter_hovered = ui.rect_contains_pointer(gutter_rect);

        let mut scroll = egui::ScrollArea::vertical()
            .id_salt((pane.salt, "vert", pane.id))
            .drag_to_scroll(false)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .auto_shrink([false; 2])
            .max_height(sa_h);
        if pane.reset_scroll {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show(ui, |ui| {
            let horiz_offset = *pane.scroll_x;
            egui::ScrollArea::horizontal()
                .id_salt((pane.salt, "horiz", pane.id))
                .horizontal_scroll_offset(horiz_offset)
                .drag_to_scroll(false)
                .enable_scrolling(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if text_exceeds_width {
                        ui.set_min_width(scroll_content_w);
                    }
                    render_tree_node(
                        ui,
                        pane.root,
                        pane.root_path,
                        TreeLabel::Root,
                        0,
                        pane.collapsed,
                        pane.search_query,
                        pane.search_matches,
                        pane.cur_match_path,
                        pane.selected_path,
                        colors,
                        pane.indent_step,
                        pane.plain_json,
                        gutter_hovered,
                        false,
                        &mut action,
                        &mut max_row_w,
                        &mut current_match_rect,
                    );
                });
            // Issue the request outside the horizontal area so the vertical area handles it.
            if pane.reveal_match {
                if let Some(rect) = current_match_rect {
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    revealed = true;
                }
            }
        });

        *pane.content_width = max_row_w;

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
                    *pane.scroll_x =
                        (*pane.scroll_x + delta_scroll).clamp(0.0, max_scroll_x);
                } else if scroll_resp.clicked() {
                    if let Some(pos) = scroll_resp.interact_pointer_pos() {
                        let target_ratio = ((pos.x - track_rect.min.x - thumb_w * 0.5)
                            / scrollable_track)
                            .clamp(0.0, 1.0);
                        *pane.scroll_x =
                            (target_ratio * max_scroll_x).clamp(0.0, max_scroll_x);
                    }
                }

                let scroll_ratio = (*pane.scroll_x / max_scroll_x).clamp(0.0, 1.0);
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

    (action, revealed)
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
        // A cursor at the newline still belongs to the line before it.
        if current_char_idx < click_char_idx && c == '\n' {
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

fn tab_row(
    ui: &mut egui::Ui,
    index: usize,
    id: u64,
    name: &str,
    pinned: bool,
    active: bool,
    reveal: bool,
    action: &mut RailAction,
) {
    let p = UiColors::current(ui);
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::click());
    if reveal {
        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
    }

    // Active tab floats like a card; hovered rows get a soft tint.
    let radius = egui::CornerRadius::same(12);
    if active {
        let shadow = egui::epaint::Shadow {
            offset: [0, 1],
            blur: 6,
            spread: 0,
            color: egui::Color32::from_black_alpha(if ui.visuals().dark_mode { 60 } else { 14 }),
        };
        ui.painter().add(shadow.as_shape(rect, radius));
        let fill = p.surface;
        ui.painter().rect(rect, radius, fill, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.hover.gamma_multiply(0.6));
    }

    let show_actions = active || resp.hovered();
    let icon = 14.0;
    let text_left = rect.left() + 12.0;
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
    let name_color = if pinned {
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
    let name_text = egui::RichText::new(name).size(13.0).color(name_color);
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

fn extract_snippet(input: &str, err_line: usize, err_col: usize) -> Option<String> {
    if err_line == 0 {
        return None;
    }
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let target_idx = err_line.saturating_sub(1);
    if target_idx >= lines.len() {
        return None;
    }

    let start_idx = target_idx.saturating_sub(1);
    let end_idx = (target_idx + 1).min(lines.len() - 1);
    let width = (end_idx + 1).to_string().len().max(2);

    let mut out = String::new();
    for i in start_idx..=end_idx {
        let is_err = i == target_idx;
        let mark = if is_err { ">" } else { " " };
        let line_content = lines[i];
        out.push_str(&format!("{mark} {:>width$} | {line_content}\n", i + 1));
        if is_err {
            let col_offset = err_col.saturating_sub(1);
            let leading_chars = line_content.chars().take(col_offset);
            let mut indent_len = 0;
            for ch in leading_chars {
                if ch == '\t' {
                    indent_len += 4;
                } else if ch.is_ascii() {
                    indent_len += 1;
                } else {
                    indent_len += 2;
                }
            }
            let pointer_indent = " ".repeat(width + 4 + indent_len);
            out.push_str(&format!("{pointer_indent}^\n"));
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

/// Height of the toolbar row, level with the macOS window controls.
const TOP_ROW_H: f32 = 32.0;
/// Space between the two cards; the split handle sits in it.
const CARD_GAP: f32 = 8.0;
const CARD_RADIUS: u8 = 10;

/// Splits the card area at `left_w`, leaving `CARD_GAP` between the two cards.
fn split_cards(rect: egui::Rect, left_w: f32) -> (egui::Rect, egui::Rect) {
    let x = (rect.left() + left_w).round();
    let left = egui::Rect::from_min_max(rect.min, egui::pos2(x - CARD_GAP * 0.5, rect.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(x + CARD_GAP * 0.5, rect.top()), rect.max);
    (left, right)
}

/// Paints a clean solid card and returns its body rect.
fn draw_card(ui: &egui::Ui, rect: egui::Rect, focused: bool) -> egui::Rect {
    let radius = egui::CornerRadius::same(CARD_RADIUS);
    let painter = ui.painter();
    let dark = ui.visuals().dark_mode;
    let p = UiColors::new(dark);

    // Subtle elevation shadow painted on the layer so it isn't clipped at panel edges.
    let shadow_painter = ui.ctx().layer_painter(ui.layer_id());
    let shadow_alpha = match (dark, focused) {
        (false, true) => 12,
        (false, false) => 6,
        (true, true) => 48,
        (true, false) => 28,
    };
    shadow_painter.add(
        egui::epaint::Shadow {
            offset: [0, 1],
            blur: 5,
            spread: 0,
            color: egui::Color32::from_black_alpha(shadow_alpha),
        }
        .as_shape(rect, radius),
    );

    let fill = match (dark, focused) {
        (false, true) => p.surface,
        (false, false) => egui::Color32::from_rgb(250, 251, 252),
        (true, true) => p.surface,
        (true, false) => egui::Color32::from_rgb(26, 27, 30),
    };
    let border = match (dark, focused) {
        (false, true) => egui::Color32::from_rgb(212, 216, 224),
        (false, false) => p.border,
        (true, true) => egui::Color32::from_rgb(60, 63, 72),
        (true, false) => p.border,
    };
    painter.rect(
        rect,
        radius,
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );

    // Inset the body so square content backgrounds stay clear of the rounded border.
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + 2.0, rect.top() + 8.0),
        egui::pos2(rect.right() - 2.0, rect.bottom() - 6.0),
    )
}

/// Animated pill toggle switch for settings rows. Returns `true` when toggled.
fn settings_toggle_switch(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut bool,
    p: &UiColors,
) -> bool {
    let track_w = 34.0;
    let track_h = 18.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(track_w, track_h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut changed = false;
    if resp.clicked() {
        *value = !*value;
        changed = true;
    }

    let anim = ui.ctx().animate_bool_responsive(id, *value);
    let track_bg = if *value { p.primary } else { p.border };
    let radius = egui::CornerRadius::same((track_h * 0.5) as u8);
    ui.painter().rect_filled(rect, radius, track_bg);

    let thumb_r = track_h * 0.5 - 2.0;
    let thumb_x = egui::lerp(
        (rect.left() + track_h * 0.5)..=(rect.right() - track_h * 0.5),
        anim,
    );
    ui.painter().circle_filled(
        egui::pos2(thumb_x, rect.center().y),
        thumb_r,
        egui::Color32::WHITE,
    );
    changed
}

/// Compact settings row action button with an optional leading Lucide icon.
fn settings_action_button(
    ui: &mut egui::Ui,
    icon: Option<LucideIcon>,
    label: &str,
    primary: bool,
    p: &UiColors,
) -> egui::Response {
    let text = tr(ui.ctx(), label);
    let font = egui::FontId::proportional(12.0);
    let text_w = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), p.text)
        .size()
        .x;
    let icon_size = 12.5;
    let icon_gap = if icon.is_some() { 5.0 } else { 0.0 };
    let pad_x = 11.0;
    let w = pad_x * 2.0 + if icon.is_some() { icon_size } else { 0.0 } + icon_gap + text_w;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    let (bg, fg, stroke) = if primary {
        let bg = if resp.is_pointer_button_down_on() {
            p.primary.gamma_multiply(0.85)
        } else {
            p.primary
        };
        (bg, egui::Color32::WHITE, egui::Stroke::NONE)
    } else if resp.is_pointer_button_down_on() {
        (p.pressed, p.strong, egui::Stroke::new(1.0, p.border))
    } else if resp.hovered() {
        (p.hover, p.strong, egui::Stroke::new(1.0, p.border))
    } else {
        (p.subtle, p.text, egui::Stroke::new(1.0, p.border))
    };

    ui.painter()
        .rect(rect, egui::CornerRadius::same(6), bg, stroke, egui::StrokeKind::Inside);

    let mut x = rect.left() + pad_x;
    if let Some(ic) = icon {
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(x + icon_size * 0.5, rect.center().y),
            egui::vec2(icon_size, icon_size),
        );
        draw_lucide_icon(ui.painter(), icon_rect, ic, fg);
        x += icon_size + icon_gap;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        fg,
    );
    resp
}

const CARD_HEADER_H: f32 = 32.0;

/// Formats a primitive JSON value for the right-hand node panel.
/// Strings can be shown either JSON-escaped (with quotes and escape sequences) or raw.
fn format_node_primitive(value: &Value, string_escaped: bool) -> String {
    match value {
        Value::String(s) if !string_escaped => s.clone(),
        _ => value.to_string(),
    }
}

const ESCAPE_SWITCH_FONT_SIZE: f32 = 11.5;
const ESCAPE_SWITCH_TRACK_W: f32 = 26.0;
const ESCAPE_SWITCH_TRACK_H: f32 = 14.0;
const ESCAPE_SWITCH_GAP: f32 = 6.0;
const ESCAPE_SWITCH_PAD_X: f32 = 6.0;
const ESCAPE_SWITCH_HEIGHT: f32 = 22.0;

fn escape_switch_width(ui: &egui::Ui) -> f32 {
    let label = tr(ui.ctx(), "Escape");
    let font = egui::FontId::proportional(ESCAPE_SWITCH_FONT_SIZE);
    let label_w = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font, egui::Color32::WHITE).size().x);
    ESCAPE_SWITCH_PAD_X * 2.0 + label_w + ESCAPE_SWITCH_GAP + ESCAPE_SWITCH_TRACK_W
}

/// Draws a compact "Escape" toggle switch at the top-right corner of the node panel header.
fn draw_escape_switch(
    ui: &mut egui::Ui,
    header: egui::Rect,
    tab_id: u64,
    escaped: &mut bool,
    p: UiColors,
) {
    let w = escape_switch_width(ui);
    let rect = egui::Rect::from_center_size(
        egui::pos2(header.right() - 10.0 - w * 0.5, header.center().y),
        egui::vec2(w, ESCAPE_SWITCH_HEIGHT),
    );
    let id = egui::Id::new(("node_escape_switch", tab_id));
    let tip = if *escaped {
        "Showing escaped string · click to unescape"
    } else {
        "Showing unescaped string · click to escape"
    };
    let resp = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tr(ui.ctx(), tip));
    if resp.clicked() {
        *escaped = !*escaped;
    }

    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(header));

    let label_color = if *escaped || resp.hovered() { p.text } else { p.muted };
    let label = tr(ui.ctx(), "Escape");
    let font = egui::FontId::proportional(ESCAPE_SWITCH_FONT_SIZE);
    let galley = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font, label_color));
    painter.galley(
        egui::pos2(
            rect.left() + ESCAPE_SWITCH_PAD_X,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        label_color,
    );

    let track_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.right() - ESCAPE_SWITCH_PAD_X - ESCAPE_SWITCH_TRACK_W * 0.5,
            rect.center().y,
        ),
        egui::vec2(ESCAPE_SWITCH_TRACK_W, ESCAPE_SWITCH_TRACK_H),
    );
    let anim = ui.ctx().animate_bool_responsive(id, *escaped);
    let track_bg = if *escaped { p.primary } else { p.border };
    let track_radius = egui::CornerRadius::same((ESCAPE_SWITCH_TRACK_H * 0.5) as u8);
    painter.rect_filled(track_rect, track_radius, track_bg);

    let thumb_r = ESCAPE_SWITCH_TRACK_H * 0.5 - 2.0;
    let thumb_x = egui::lerp(
        (track_rect.left() + ESCAPE_SWITCH_TRACK_H * 0.5)
            ..=(track_rect.right() - ESCAPE_SWITCH_TRACK_H * 0.5),
        anim,
    );
    painter.circle_filled(
        egui::pos2(thumb_x, track_rect.center().y),
        thumb_r,
        egui::Color32::WHITE,
    );
}

/// Paints an icon + title header across the top of a card drawn by `draw_card`,
/// and returns the body rect below it.
fn draw_card_header(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: LucideIcon,
    icon_color: egui::Color32,
    title: &str,
    focused: bool,
    right_reserve: f32,
    p: UiColors,
) -> egui::Rect {
    let painter = ui.painter();
    let title_color = if focused { p.text } else { p.muted };
    let icon_color = if focused { icon_color } else { icon_color.gamma_multiply(0.7) };

    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), CARD_HEADER_H));
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(header.left() + 18.0, header.center().y),
        egui::vec2(14.0, 14.0),
    );
    draw_lucide_icon(painter, icon_rect, icon, icon_color);
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(icon_rect.right() + 8.0, header.top()),
        egui::pos2(
            (header.right() - 12.0 - right_reserve).max(icon_rect.right() + 8.0),
            header.bottom(),
        ),
    );
    let galley = ui.fonts(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(
            title.to_owned(),
            egui::FontId::proportional(13.0),
            title_color,
        );
        job.wrap.max_width = text_rect.width().max(0.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        f.layout_job(job)
    });
    painter.with_clip_rect(text_rect).galley(
        egui::pos2(text_rect.left(), text_rect.center().y - galley.size().y * 0.5),
        galley,
        title_color,
    );

    // Inset the body so square content backgrounds stay clear of the rounded border.
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + 2.0, header.bottom()),
        egui::pos2(rect.right() - 2.0, rect.bottom() - 6.0),
    )
}

/// Lets empty chrome move the window; double-click toggles maximize.
fn window_drag_area(ui: &egui::Ui, rect: egui::Rect, salt: &str) {
    let resp = ui.interact(rect, egui::Id::new(salt), egui::Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        let is_max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
    }
}

/// Rounds to whole physical pixels, so edges stay crisp while animating.
fn snap_to_pixels(ctx: &egui::Context, v: f32) -> f32 {
    let ppp = ctx.pixels_per_point();
    (v * ppp).round() / ppp
}

#[cfg(test)]
mod intro_tests {
    use super::*;

    /// Lays the intro out in a card of the given size and returns the block's height.
    fn intro_height(card: egui::Vec2) -> f32 {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, card)),
            ..Default::default()
        };
        // Two frames: the second one uses the height measured by the first.
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let p = UiColors::current(ui);
                    let rows = [
                        IntroRow::tip(LucideIcon::Braces, "Paste JSON from the clipboard"),
                        IntroRow::tip(LucideIcon::Download, "Drop a JSON file onto the window"),
                        IntroRow::tip(
                            LucideIcon::ExternalLink,
                            "Paste a URL or cURL command to load its response",
                        ),
                    ];
                    intro_panel(
                        ui,
                        p,
                        "test",
                        "IJson",
                        "A fast, native JSON viewer and editor",
                        &rows,
                    );
                });
            });
        }
        ctx.data(|d| d.get_temp::<f32>(egui::Id::new(("intro_height", "test")))).unwrap()
    }

    #[test]
    fn intro_fits_wide_and_narrow_cards() {
        // Horizontal logo+title header + 3 borderless rows; narrow cards wrap onto extra lines.
        let wide = intro_height(egui::vec2(700.0, 500.0));
        assert!((160.0..260.0).contains(&wide), "wide: {wide}");
        let narrow = intro_height(egui::vec2(260.0, 500.0));
        assert!(narrow >= wide && narrow < 380.0, "narrow: {narrow}, wide: {wide}");
    }

    #[test]
    fn format_node_primitive_toggles_string_escaping() {
        let s = serde_json::json!("line1\nline2\t\"quoted\"\\end");
        assert_eq!(
            format_node_primitive(&s, true),
            "\"line1\\nline2\\t\\\"quoted\\\"\\\\end\""
        );
        assert_eq!(
            format_node_primitive(&s, false),
            "line1\nline2\t\"quoted\"\\end"
        );

        assert_eq!(format_node_primitive(&serde_json::json!(42), false), "42");
        assert_eq!(format_node_primitive(&serde_json::json!(true), false), "true");
        assert_eq!(format_node_primitive(&serde_json::json!(null), false), "null");
    }
}
