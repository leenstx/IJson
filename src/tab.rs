use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;

use crate::compare::Comparison;
use crate::fold::{apply_view_edit, build_view, shift_folds, FoldSpan, ViewCommit, ViewLine};
use crate::model::{
    collapse_beyond_depth, collect_all_expandable_paths, delete_value_at_path,
    is_minimal_json_value, is_request_candidate, search_recursive, simplify_json_value,
    sort_json_value, to_minimal_json_value, try_request_json, try_unescape_json,
    update_value_at_path, EditModalState, ParseError, SortMode, ValueTypeTag,
};

pub(crate) const TAB_RAIL_WIDTH: f32 = 148.0;

pub(crate) struct RequestLoadingState {
    pub(crate) target_url: String,
    pub(crate) raw_text: String,
    pub(crate) for_editor_paste: bool,
    pub(crate) receiver: std::sync::mpsc::Receiver<Result<String, String>>,
}

pub(crate) struct JsonTab {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) auto_name: String,
    pub(crate) pinned: bool,
    /// Set by document edits. The app folds this into the session dirty flag.
    pub(crate) dirty: bool,
    pub(crate) comparison: Option<Box<Comparison>>,
    pub(crate) revision: u64,

    pub(crate) input: String,
    // User text before key sorting, so "Original" can restore editor and tree together.
    pub(crate) source_input: String,
    pub(crate) applying_sort: bool,
    pub(crate) parsed: Option<Value>,
    pub(crate) parse_error: Option<ParseError>,

    pub(crate) collapsed: HashSet<String>,
    pub(crate) current_depth: usize,
    pub(crate) sort_mode: SortMode,

    pub(crate) search_query: String,
    pub(crate) search_matches: Vec<String>,
    pub(crate) search_index: usize,
    pub(crate) reveal_search_match: bool,

    pub(crate) selected_path: Option<String>,
    pub(crate) edit_modal: Option<EditModalState>,
    pub(crate) editor_scroll_x: f32,
    /// Vertical editor offset from the last frame.
    pub(crate) editor_scroll_y: f32,
    /// Largest vertical editor offset from the last frame.
    pub(crate) editor_scroll_max_y: f32,
    /// Vertical scroll requested by compare mode's Cmd+scroll sync.
    pub(crate) editor_sync_scroll: f32,
    pub(crate) reset_editor_scroll: bool,
    pub(crate) editor_content_width: f32,
    pub(crate) tree_scroll_x: f32,
    pub(crate) tree_content_width: f32,
    pub(crate) editor_last_interaction: f64,
    // True after the user clicks into the editor, until they click outside it.
    pub(crate) editor_editing: bool,
    pub(crate) editor_card_rect: egui::Rect,
    // Folded projection of `input`. TextEdit edits this, then the change is mapped back.
    pub(crate) editor_view: String,
    pub(crate) editor_built: String,
    pub(crate) editor_map: Vec<ViewLine>,
    pub(crate) editor_spans: Vec<FoldSpan>,
    pub(crate) editor_folds: HashSet<usize>,
    pub(crate) request_loading: Option<RequestLoadingState>,

    undo_past: Vec<String>,
    undo_future: Vec<String>,
    undo_pending: Option<String>,
    undo_coalesce: bool,
    undo_at: Option<std::time::Instant>,
}

impl JsonTab {
    pub(crate) fn empty(id: u64, name: String) -> Self {
        let mut tab = Self {
            id,
            auto_name: name.clone(),
            name,
            pinned: false,
            dirty: false,
            comparison: None,
            revision: 0,
            input: String::new(),
            source_input: String::new(),
            applying_sort: false,
            parsed: None,
            parse_error: None,
            collapsed: HashSet::new(),
            current_depth: 1,
            sort_mode: SortMode::None,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
            reveal_search_match: false,
            selected_path: None,
            edit_modal: None,
            editor_scroll_x: 0.0,
            editor_scroll_y: 0.0,
            editor_scroll_max_y: 0.0,
            editor_sync_scroll: 0.0,
            reset_editor_scroll: false,
            editor_content_width: 0.0,
            tree_scroll_x: 0.0,
            tree_content_width: 0.0,
            editor_last_interaction: 0.0,
            editor_editing: false,
            editor_card_rect: egui::Rect::NOTHING,
            editor_view: String::new(),
            editor_built: String::new(),
            editor_map: Vec::new(),
            editor_spans: Vec::new(),
            editor_folds: HashSet::new(),
            request_loading: None,
            undo_past: Vec::new(),
            undo_future: Vec::new(),
            undo_pending: None,
            undo_coalesce: false,
            undo_at: None,
        };
        tab.parse_input(true);
        tab.dirty = false;
        tab
    }

    pub(crate) fn reparse(&mut self) {
        self.parse_input(true);
        self.note_source_input();
        self.dirty = true;
    }

    pub(crate) fn note_source_input(&mut self) {
        if !self.applying_sort {
            self.source_input = self.input.clone();
        }
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo_past.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.undo_future.is_empty()
    }

    /// Snapshot the current text before a change. Typing within 800ms shares one step.
    fn remember_before_edit(&mut self, coalesce_typing: bool) {
        let now = std::time::Instant::now();
        let burst = coalesce_typing
            && self.undo_coalesce
            && self
                .undo_at
                .is_some_and(|at| now.duration_since(at) < std::time::Duration::from_millis(800));
        self.undo_at = Some(now);
        if burst {
            return;
        }
        self.undo_pending = Some(self.input.clone());
        self.undo_coalesce = coalesce_typing;
    }

    fn note_edit_applied(&mut self) {
        let Some(prev) = self.undo_pending.take() else {
            return;
        };
        if prev == self.input {
            return;
        }
        self.undo_past.push(prev);
        if self.undo_past.len() > 40 {
            let extra = self.undo_past.len() - 40;
            self.undo_past.drain(0..extra);
        }
        self.undo_future.clear();
    }

    pub(crate) fn undo_edit(&mut self) {
        self.undo_coalesce = false;
        self.undo_pending = None;
        let Some(prev) = self.undo_past.pop() else {
            return;
        };
        self.undo_future.push(self.input.clone());
        self.restore_input(prev);
    }

    pub(crate) fn redo_edit(&mut self) {
        self.undo_coalesce = false;
        self.undo_pending = None;
        let Some(next) = self.undo_future.pop() else {
            return;
        };
        self.undo_past.push(self.input.clone());
        self.restore_input(next);
    }

    fn restore_input(&mut self, text: String) {
        self.input = text;
        self.applying_sort = false;
        self.parse_input(false);
        self.note_source_input();
        self.dirty = true;
    }

    pub(crate) fn clear_document(&mut self) {
        self.remember_before_edit(false);
        self.editor_folds.clear();
        self.input.clear();
        self.reparse();
        self.editor_scroll_x = 0.0;
        self.reset_editor_scroll = true;
        self.note_edit_applied();
    }

    pub(crate) fn replace_document(&mut self, mut text: String) {
        self.remember_before_edit(false);
        let parsed = match serde_json::from_str::<Value>(&text) {
            Ok(Value::String(ref s)) => {
                let trimmed = s.trim();
                if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                    || (trimmed.starts_with('[') && trimmed.ends_with(']'))
                {
                    try_unescape_json(&text)
                        .or_else(|| {
                            serde_json::from_str::<Value>(trimmed)
                                .ok()
                                .map(|v| (v, trimmed.to_string()))
                        })
                        .or_else(|| try_request_json(&text))
                } else {
                    Some((Value::String(s.clone()), text.clone()))
                }
            }
            Ok(v) => Some((v, text.clone())),
            Err(_) => try_unescape_json(&text).or_else(|| try_request_json(&text)),
        };

        if let Some((v, unescaped)) = parsed {
            if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                text = pretty;
            } else {
                text = unescaped;
            }
        }
        self.editor_folds.clear();
        self.input = text;
        self.reparse();
        self.editor_scroll_x = 0.0;
        self.reset_editor_scroll = true;
        self.note_edit_applied();
    }

    pub(crate) fn parse_input(&mut self, reset_collapse: bool) {
        self.revision = self.revision.wrapping_add(1);
        // Do not report error when input is empty
        if self.input.trim().is_empty() {
            self.parsed = None;
            self.parse_error = None;
            if reset_collapse {
                self.collapsed.clear();
            }
            self.rebuild_editor_view();
            return;
        }

        match serde_json::from_str::<Value>(&self.input) {
            Ok(v) => {
                self.parsed = Some(v);
                self.parse_error = None;
                if reset_collapse {
                    // Default display Level 1, unless a saved depth was restored.
                    self.expand_to_depth(self.current_depth);
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                self.parse_error = Some(ParseError {
                    message: err_str,
                    line: e.line(),
                    column: e.column(),
                });
                self.parsed = None;
            }
        }
        self.rebuild_editor_view();
    }

    pub(crate) fn rebuild_editor_view(&mut self) {
        let (view, map, spans) = build_view(&self.input, &self.editor_folds);
        self.editor_spans = spans;
        self.editor_folds.retain(|start| {
            self.editor_spans
                .iter()
                .any(|span| span.start_line == *start)
        });
        self.editor_map = map;
        self.editor_view = view.clone();
        self.editor_built = view;
    }

    pub(crate) fn toggle_editor_fold(&mut self, start_line: usize) {
        if !self.editor_folds.remove(&start_line) {
            self.editor_folds.insert(start_line);
        }
        self.rebuild_editor_view();
        self.dirty = true;
    }

    pub(crate) fn commit_editor_view(&mut self, format_after_paste: bool) {
        if self.editor_view == self.editor_built {
            return;
        }
        self.remember_before_edit(!format_after_paste);
        match apply_view_edit(
            &self.input,
            &self.editor_map,
            &self.editor_built,
            &self.editor_view,
        ) {
            ViewCommit::Unchanged => {}
            ViewCommit::Restore => {
                self.editor_view = self.editor_built.clone();
            }
            ViewCommit::Applied {
                source,
                src_lo,
                src_hi,
                new_line_count,
                expand,
            } => {
                shift_folds(
                    &mut self.editor_folds,
                    src_lo,
                    src_hi,
                    new_line_count,
                    &expand,
                );
                if source != self.input {
                    self.input = source;
                    self.reparse();
                    // Keep insertion and automatic formatting in one undo step.
                    if format_after_paste {
                        if self.parsed.is_none() {
                            if let Some((_, unescaped)) = try_unescape_json(&self.input) {
                                self.input = unescaped;
                                self.reparse();
                            } else if let Some((_, requested)) = try_request_json(&self.input) {
                                self.input = requested;
                                self.reparse();
                            }
                        }
                        self.format_current();
                    }
                } else {
                    self.rebuild_editor_view();
                }
            }
        }
        self.note_edit_applied();
    }

    pub(crate) fn format_json(&mut self, ctx: &egui::Context) {
        if self.input.trim().is_empty() {
            return;
        }
        self.remember_before_edit(false);
        if self.parsed.is_none() {
            if let Some((_, unescaped)) = try_unescape_json(&self.input) {
                self.input = unescaped;
                self.reparse();
            } else if is_request_candidate(&self.input) {
                self.start_request_loading(self.input.clone(), false, ctx);
                return;
            } else if let Some((_, requested)) = try_request_json(&self.input) {
                self.input = requested;
                self.reparse();
            }
        }
        self.format_current();
        self.note_edit_applied();
    }

    fn format_current(&mut self) {
        if let Some(v) = &self.parsed {
            if let Ok(formatted) = serde_json::to_string_pretty(v) {
                if formatted != self.input {
                    self.editor_folds.clear();
                    self.input = formatted;
                    self.reparse();
                    self.editor_scroll_x = 0.0;
                    self.reset_editor_scroll = true;
                }
            }
        }
    }

    pub(crate) fn compress_json(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }
        self.remember_before_edit(false);
        if let Some(v) = &self.parsed {
            if let Ok(compressed) = serde_json::to_string(v) {
                if compressed != self.input {
                    self.editor_folds.clear();
                    self.input = compressed;
                    self.reparse();
                    self.editor_scroll_x = 0.0;
                    self.reset_editor_scroll = true;
                }
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(self.input.clone());
                }
            }
        }
        self.note_edit_applied();
    }

    pub(crate) fn simplify_json(&mut self) {
        let Some(value) = &self.parsed else {
            return;
        };
        let target = if is_minimal_json_value(value) {
            simplify_json_value(value)
        } else {
            to_minimal_json_value(value)
        };
        if let Ok(text) = serde_json::to_string_pretty(&target) {
            if text != self.input {
                // Reuse document replacement so extraction is a single undoable edit.
                self.replace_document(text);
                self.selected_path = None;
                self.update_search();
            }
        }
    }

    pub(crate) fn apply_sort(&mut self) {
        let source = if self.source_input.trim().is_empty() {
            self.input.clone()
        } else {
            self.source_input.clone()
        };
        if source.trim().is_empty() {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(&source) else {
            return;
        };

        self.remember_before_edit(false);
        self.applying_sort = true;
        self.editor_folds.clear();
        self.input = if self.sort_mode == SortMode::None {
            serde_json::to_string_pretty(&v).unwrap_or(source)
        } else {
            let sorted = sort_json_value(&v, self.sort_mode);
            serde_json::to_string_pretty(&sorted).unwrap_or_default()
        };
        // Rewrite the editor, but keep the tree's current expand/collapse set.
        // Paths are keyed by name, so reordering keys does not change which nodes are open.
        self.parse_input(false);
        self.editor_scroll_x = 0.0;
        self.reset_editor_scroll = true;
        self.dirty = true;
        self.applying_sort = false;
        self.note_edit_applied();
    }

    pub(crate) fn expand_all(&mut self) {
        self.collapsed.clear();
        self.dirty = true;
    }

    pub(crate) fn collapse_all(&mut self) {
        if let Some(v) = &self.parsed {
            self.collapsed.clear();
            collect_all_expandable_paths(v, "$", &mut self.collapsed);
            self.dirty = true;
        }
    }

    pub(crate) fn expand_to_depth(&mut self, target_depth: usize) {
        self.current_depth = target_depth;
        if let Some(v) = &self.parsed {
            self.collapsed.clear();
            collapse_beyond_depth(v, "$", 0, target_depth, &mut self.collapsed);
            self.dirty = true;
        }
    }

    pub(crate) fn update_search(&mut self) {
        self.search_matches.clear();
        self.search_index = 0;
        self.reveal_search_match = false;
        self.dirty = true;
        let q = self.search_query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }

        if let Some(v) = &self.parsed {
            search_recursive(v, "$", &q, &mut self.search_matches);

            for path in &self.search_matches {
                let mut p = path.clone();
                while let Some(pos) = p.rfind(|c| c == '.' || c == '[') {
                    p.truncate(pos);
                    self.collapsed.remove(&p);
                }
            }

            if !self.search_matches.is_empty() {
                self.selected_path = Some(self.search_matches[0].clone());
                self.reveal_search_match = true;
            }
        }
    }

    pub(crate) fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_matches.len();
        self.selected_path = Some(self.search_matches[self.search_index].clone());
        self.reveal_search_match = true;
        self.dirty = true;
    }

    pub(crate) fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_index == 0 {
            self.search_index = self.search_matches.len() - 1;
        } else {
            self.search_index -= 1;
        }
        self.selected_path = Some(self.search_matches[self.search_index].clone());
        self.reveal_search_match = true;
        self.dirty = true;
    }

    pub(crate) fn open_edit_modal(&mut self, path: &str, key_label: &str, val: &Value) {
        let (input_text, target_type) = match val {
            Value::String(s) => (s.clone(), ValueTypeTag::String),
            Value::Number(n) => (n.to_string(), ValueTypeTag::Number),
            Value::Bool(b) => (b.to_string(), ValueTypeTag::Bool),
            Value::Null => ("null".to_string(), ValueTypeTag::Null),
            _ => (
                serde_json::to_string_pretty(val).unwrap_or_default(),
                ValueTypeTag::RawJson,
            ),
        };

        self.edit_modal = Some(EditModalState {
            path: path.to_string(),
            key_label: key_label.to_string(),
            input_text,
            target_type,
            error: None,
        });
    }

    pub(crate) fn commit_edit_modal(&mut self) -> bool {
        let Some(modal) = self.edit_modal.as_ref() else {
            return false;
        };
        let path = modal.path.clone();
        let parsed_val: Result<Value, String> = match modal.target_type {
            ValueTypeTag::String => Ok(Value::String(modal.input_text.clone())),
            ValueTypeTag::Number => {
                if let Ok(n) = modal.input_text.parse::<i64>() {
                    Ok(Value::Number(n.into()))
                } else if let Ok(f) = modal.input_text.parse::<f64>() {
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .ok_or_else(|| "Invalid float number".to_string())
                } else {
                    Err("Enter a valid number".to_string())
                }
            }
            ValueTypeTag::Bool => match modal.input_text.trim().to_lowercase().as_str() {
                "true" | "1" => Ok(Value::Bool(true)),
                "false" | "0" => Ok(Value::Bool(false)),
                _ => Err("Choose true or false".to_string()),
            },
            ValueTypeTag::Null => Ok(Value::Null),
            ValueTypeTag::RawJson => {
                serde_json::from_str::<Value>(&modal.input_text)
                    .or_else(|_| {
                        try_unescape_json(&modal.input_text)
                            .map(|(v, _)| v)
                            .ok_or_else(|| "Invalid JSON".to_string())
                    })
                    .map_err(|e| e.to_string())
            }
        };

        match parsed_val {
            Ok(new_val) => {
                if let Some(mut root) = self.parsed.clone() {
                    if update_value_at_path(&mut root, &path, new_val) {
                        self.remember_before_edit(false);
                        self.editor_folds.clear();
                        self.input = serde_json::to_string_pretty(&root).unwrap_or_default();
                        self.parsed = Some(root);
                        self.note_source_input();
                        self.rebuild_editor_view();
                        self.dirty = true;
                        self.edit_modal = None;
                        self.note_edit_applied();
                        return true;
                    }
                }
                if let Some(modal) = &mut self.edit_modal {
                    modal.error = Some("Could not update this node".to_string());
                }
                false
            }
            Err(err) => {
                if let Some(modal) = &mut self.edit_modal {
                    modal.error = Some(err);
                }
                false
            }
        }
    }

    pub(crate) fn delete_node_at(&mut self, path: &str) {
        if let Some(mut root) = self.parsed.clone() {
            if delete_value_at_path(&mut root, path) {
                self.remember_before_edit(false);
                self.editor_folds.clear();
                self.input = serde_json::to_string_pretty(&root).unwrap_or_default();
                self.parsed = Some(root);
                self.note_source_input();
                self.rebuild_editor_view();
                self.dirty = true;
                self.note_edit_applied();
            }
        }
    }

    pub(crate) fn start_request_loading(
        &mut self,
        text: String,
        for_editor_paste: bool,
        ctx: &egui::Context,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let trimmed = text.trim();
        let target_url = if trimmed.starts_with("curl") {
            shlex::split(trimmed)
                .and_then(|args| {
                    args.into_iter().find(|a| a.starts_with("http://") || a.starts_with("https://"))
                })
                .unwrap_or_else(|| "curl request".to_string())
        } else {
            trimmed.to_string()
        };

        let text_clone = text.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let res = try_request_json(&text_clone)
                .map(|(_, s)| s)
                .ok_or_else(|| "Failed".to_string());
            let _ = tx.send(res);
            ctx_clone.request_repaint();
        });

        self.request_loading = Some(RequestLoadingState {
            target_url,
            raw_text: text,
            for_editor_paste,
            receiver: rx,
        });
    }

    pub(crate) fn load_text(&mut self, mut content: String) {
        self.remember_before_edit(false);
        if serde_json::from_str::<Value>(&content).is_err() {
            if let Some((_, unescaped)) = try_unescape_json(&content) {
                content = unescaped;
            } else if let Some((_, requested)) = try_request_json(&content) {
                content = requested;
            }
        }
        self.editor_folds.clear();
        self.input = content;
        self.reparse();
        self.format_current();
        self.editor_scroll_x = 0.0;
        self.reset_editor_scroll = true;
        self.note_edit_applied();
    }

    /// Rebuild derived state after fields were restored from session.
    pub(crate) fn finish_load(&mut self, collapsed: HashSet<String>, search_index: usize) {
        // Keep the saved fold state. A normal reparse would reset it to the depth preset.
        self.parse_input(false);
        self.collapsed = collapsed;
        self.update_search();
        if !self.search_matches.is_empty() {
            self.search_index = search_index.min(self.search_matches.len() - 1);
            self.selected_path = Some(self.search_matches[self.search_index].clone());
        }
        self.dirty = false;
    }

    pub(crate) fn to_json(&self) -> Value {
        let collapsed: Vec<&String> = self.collapsed.iter().collect();
        let mut editor_folds: Vec<usize> = self.editor_folds.iter().copied().collect();
        editor_folds.sort_unstable();
        let sort_mode = match self.sort_mode {
            SortMode::None => "none",
            SortMode::Asc => "asc",
            SortMode::Desc => "desc",
        };
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "auto_name": self.auto_name,
            "pinned": self.pinned,
            "input": self.input,
            "source_input": self.source_input,
            "collapsed": collapsed,
            "current_depth": self.current_depth,
            "sort_mode": sort_mode,
            "search_query": self.search_query,
            "search_index": self.search_index,
            "selected_path": self.selected_path,
            "editor_folds": editor_folds,
            "comparison_right": self.comparison.as_ref().map(|comparison| comparison.right.to_json()),
        })
    }
}

pub(crate) struct LoadedTab {
    pub(crate) tab: JsonTab,
    pub(crate) collapsed: HashSet<String>,
    pub(crate) search_index: usize,
}

pub(crate) fn tab_from_value(v: &Value, fallback_id: u64) -> Option<LoadedTab> {
    let input = v.get("input").and_then(|x| x.as_str())?.to_string();
    let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(fallback_id);
    let auto_name = v
        .get("auto_name")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| auto_name.clone())
        .unwrap_or_else(local_stamp_now);
    let auto_name = auto_name.unwrap_or_else(|| name.clone());
    let mut tab = JsonTab::empty(id, name);
    tab.auto_name = auto_name;
    tab.pinned = v.get("pinned").and_then(|x| x.as_bool()).unwrap_or(false);
    tab.input = input;
    if let Some(lines) = v.get("editor_folds").and_then(|x| x.as_array()) {
        tab.editor_folds = lines
            .iter()
            .filter_map(|n| n.as_u64().map(|n| n as usize))
            .collect();
    }
    tab.source_input = v
        .get("source_input")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| tab.input.clone());
    if let Some(depth) = v.get("current_depth").and_then(|x| x.as_u64()) {
        tab.current_depth = (depth as usize).clamp(1, 64);
    }
    tab.sort_mode = match v.get("sort_mode").and_then(|x| x.as_str()) {
        Some("asc") => SortMode::Asc,
        Some("desc") => SortMode::Desc,
        _ => SortMode::None,
    };
    tab.search_query = v
        .get("search_query")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    tab.selected_path = v
        .get("selected_path")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let collapsed = v
        .get("collapsed")
        .and_then(|x| x.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let search_index = v.get("search_index").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    if let Some(right) = v.get("comparison_right").filter(|right| {
        right.is_object() && right.get("comparison_right").is_none_or(Value::is_null)
    }) {
        if let Some(mut loaded) = tab_from_value(right, id.saturating_add(1)) {
            loaded
                .tab
                .finish_load(loaded.collapsed, loaded.search_index);
            tab.comparison = Some(Box::new(Comparison::new(loaded.tab)));
        }
    }
    Some(LoadedTab {
        tab,
        collapsed,
        search_index,
    })
}

pub(crate) fn unique_tab_name(existing: &[JsonTab], base: &str) -> String {
    if !existing.iter().any(|tab| tab.name == base) {
        return base.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base} {n}");
        if !existing.iter().any(|tab| tab.name == candidate) {
            return candidate;
        }
        n += 1;
    }
}

pub(crate) fn local_stamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_local_stamp(secs)
}

pub(crate) fn stamp_from_system_time(time: std::time::SystemTime) -> Option<String> {
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(format_local_stamp(secs))
}

fn format_local_stamp(secs: i64) -> String {
    unsafe {
        let mut tm = std::mem::zeroed();
        if localtime_r(&secs, &mut tm).is_null() {
            return "00.00:00:00".to_string();
        }
        format!(
            "{:02}.{:02}:{:02}:{:02}",
            tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec
        )
    }
}

#[repr(C)]
struct LibcTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const i8,
}

extern "C" {
    fn localtime_r(time: *const i64, result: *mut LibcTm) -> *mut LibcTm;
}
