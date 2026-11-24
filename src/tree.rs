use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;

use crate::icons::{draw_lucide_icon, LucideIcon};
use crate::model::TreeAction;
use crate::theme::ThemeColors;
use crate::widgets::{row_label, show_tree_context_menu};

pub(crate) const TREE_ROW_HEIGHT: f32 = 24.0;
const TREE_FONT_SIZE: f32 = 13.0;
const META_FONT_SIZE: f32 = 12.0;
const ROW_LEFT_PAD: f32 = 4.0;
const CHEVRON_W: f32 = 16.0;
const CHEVRON_ICON_SIZE: f32 = 12.0;
const CHEVRON_GAP: f32 = 4.0;
pub(crate) const GUTTER_W: f32 = CHEVRON_W + CHEVRON_GAP;

#[derive(Clone, Copy)]
pub(crate) enum TreeLabel<'a> {
    Root,
    Key(&'a str),
    Index(&'a str),
}

fn render_tree_label(
    ui: &mut egui::Ui,
    label: TreeLabel<'_>,
    colors: ThemeColors,
    plain_json: bool,
    is_container: bool,
) {
    match label {
        TreeLabel::Root => {}
        TreeLabel::Key(key) if plain_json => {
            let quoted = serde_json::to_string(key).expect("a string can be serialized to JSON");
            row_label(
                ui,
                egui::RichText::new(quoted)
                    .monospace()
                    .size(TREE_FONT_SIZE)
                    .color(colors.key),
            );
            row_label(
                ui,
                egui::RichText::new(": ")
                    .monospace()
                    .size(TREE_FONT_SIZE)
                    .color(colors.colon),
            );
        }
        TreeLabel::Key(key) => {
            let display = if key.is_empty() { "\"\"" } else { key };
            row_label(
                ui,
                egui::RichText::new(display)
                    .monospace()
                    .size(TREE_FONT_SIZE)
                    .color(colors.key),
            );
            if !is_container {
                row_label(
                    ui,
                    egui::RichText::new(":")
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.colon),
                );
            }
            ui.add_space(6.0);
        }
        TreeLabel::Index(index) if !plain_json => {
            row_label(
                ui,
                egui::RichText::new(index)
                    .monospace()
                    .size(META_FONT_SIZE)
                    .color(colors.ui.faint),
            );
            ui.add_space(8.0);
        }
        TreeLabel::Index(_) => {}
    }
}

pub(crate) fn render_tree_node(
    ui: &mut egui::Ui,
    value: &Value,
    path: &str,
    label: TreeLabel<'_>,
    depth: usize,
    collapsed: &mut HashSet<String>,
    search_query: &str,
    search_matches: &[String],
    cur_match_path: Option<&str>,
    selected_path: Option<&str>,
    colors: ThemeColors,
    indent_step: f32,
    plain_json: bool,
    show_fold_arrows: bool,
    trailing_comma: bool,
    action: &mut Option<TreeAction>,
    max_row_w: &mut f32,
    current_match_rect: &mut Option<egui::Rect>,
) {
    let base_x = ui.cursor().min.x;
    let indent = depth as f32 * indent_step;

    let is_current_match = cur_match_path == Some(path);
    let is_selected = selected_path == Some(path);

    let row_height = TREE_ROW_HEIGHT;
    let available_w = ui.available_width();

    // Rows outside the viewport only reserve their space, so big documents stay fast.
    let row_top = ui.cursor().min.y;
    let clip = ui.clip_rect();
    let off_screen =
        (row_top > clip.bottom() || row_top + row_height < clip.top()) && !is_current_match;
    let is_collapsed = collapsed.contains(path);
    let container = match value {
        Value::Object(map) => Some(('{', '}', map.len())),
        Value::Array(arr) => Some(('[', ']', arr.len())),
        _ => None,
    };

    let key_label = match label {
        TreeLabel::Root => None,
        TreeLabel::Key(key) | TreeLabel::Index(key) => Some(key),
    };
    if off_screen {
        ui.allocate_space(egui::vec2(available_w, row_height));
    } else {
        let is_search_match = !search_query.is_empty() && search_matches.iter().any(|m| m == path);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(available_w, row_height), egui::Sense::click());
        if is_current_match {
            *current_match_rect = Some(rect);
        }

        let bg_rect = rect.shrink2(egui::vec2(0.0, 0.5));
        if is_current_match {
            ui.painter().rect_filled(bg_rect, 4.0, colors.match_current);
        } else if is_search_match {
            ui.painter()
                .rect_filled(bg_rect, 4.0, colors.match_highlight);
        } else if is_selected {
            ui.painter().rect_filled(bg_rect, 4.0, colors.row_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(bg_rect, 4.0, colors.row_hover);
        }

        if !plain_json {
            for i in 0..depth {
                let line_x = base_x + ROW_LEFT_PAD + i as f32 * indent_step + CHEVRON_W * 0.5;
                ui.painter().vline(
                    line_x,
                    rect.top()..=rect.bottom(),
                    egui::Stroke::new(1.0, colors.indent_guide),
                );
            }
        }

        show_tree_context_menu(&resp, value, path, key_label, container.is_some(), action);

        let mut ui_row = ui.new_child(
            egui::UiBuilder::default()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui_row.style_mut().interaction.selectable_labels = false;
        ui_row.spacing_mut().item_spacing.x = 0.0;

        if let Some((open, close, count)) = container {
            if !plain_json {
                ui_row.add_space(ROW_LEFT_PAD + indent);
            }
            let can_fold = count > 0;
            let chevron_clicked = if can_fold {
                let (chev_rect, chev_resp) =
                    ui_row.allocate_exact_size(egui::vec2(CHEVRON_W, 18.0), egui::Sense::click());
                show_tree_context_menu(&chev_resp, value, path, key_label, true, action);
                let show_chevron =
                    !plain_json || is_collapsed || show_fold_arrows || chev_resp.hovered();
                if show_chevron {
                    let icon = if is_collapsed {
                        LucideIcon::ChevronRight
                    } else {
                        LucideIcon::ChevronDown
                    };
                    let color = if chev_resp.hovered() || resp.hovered() {
                        colors.ui.text
                    } else {
                        colors.ui.muted
                    };
                    let icon_rect = egui::Rect::from_center_size(
                        chev_rect.center(),
                        egui::vec2(CHEVRON_ICON_SIZE, CHEVRON_ICON_SIZE),
                    );
                    draw_lucide_icon(ui_row.painter(), icon_rect, icon, color);
                }
                chev_resp.clicked()
            } else {
                ui_row.add_space(CHEVRON_W);
                false
            };
            if plain_json {
                ui_row.add_space(CHEVRON_GAP + indent);
            } else {
                ui_row.add_space(CHEVRON_GAP);
            }

            if chevron_clicked || (can_fold && resp.double_clicked()) {
                if is_collapsed {
                    collapsed.remove(path);
                } else {
                    collapsed.insert(path.to_string());
                }
                *action = Some(TreeAction::Select(path.to_string()));
            } else if resp.clicked() {
                *action = Some(TreeAction::Select(path.to_string()));
            }

            render_tree_label(&mut ui_row, label, colors, plain_json, true);
            let mut brackets = if count == 0 {
                format!("{open}{close}")
            } else if !plain_json {
                format!("{open}{count}{close}")
            } else if is_collapsed {
                format!("{open}…{close}")
            } else {
                open.to_string()
            };
            if plain_json && trailing_comma && (is_collapsed || count == 0) {
                brackets.push(',');
            }
            row_label(
                &mut ui_row,
                egui::RichText::new(brackets)
                    .monospace()
                    .size(if plain_json {
                        TREE_FONT_SIZE
                    } else {
                        META_FONT_SIZE
                    })
                    .color(if !plain_json {
                        colors.ui.faint
                    } else if is_collapsed && count > 0 {
                        colors.colon
                    } else {
                        colors.punctuation
                    }),
            );
        } else {
            let is_cmd =
                ui.input(|i| i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl);

            // Interaction mapping:
            // - Double click: copy value
            // - Command + click: copy key (deferred until a double click is ruled out)
            // - Command + double click: edit value
            // - Normal click: select node
            let pending_id = resp.id.with("pending_copy_key");
            let now = ui.input(|i| i.time);
            let dbl_delay = ui.ctx().options(|o| o.input_options.max_double_click_delay);
            if let Some(t) = ui.data(|d| d.get_temp::<f64>(pending_id)) {
                if resp.double_clicked() {
                    ui.data_mut(|d| d.remove::<f64>(pending_id));
                } else if now - t >= dbl_delay {
                    ui.data_mut(|d| d.remove::<f64>(pending_id));
                    if let Some(k) = key_label {
                        *action = Some(TreeAction::CopyKey(path.to_string(), k.to_string()));
                    }
                } else {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs_f64(
                            dbl_delay - (now - t),
                        ));
                }
            }
            if resp.double_clicked() {
                if is_cmd {
                    let k_label = key_label.unwrap_or("").to_string();
                    *action = Some(TreeAction::StartEdit(
                        path.to_string(),
                        k_label,
                        value.clone(),
                    ));
                } else {
                    let val_str = match value {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    *action = Some(TreeAction::CopyValue(path.to_string(), val_str));
                }
            } else if resp.clicked() {
                if is_cmd {
                    if key_label.is_some() {
                        ui.data_mut(|d| d.insert_temp(pending_id, now));
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs_f64(dbl_delay));
                    }
                } else {
                    *action = Some(TreeAction::Select(path.to_string()));
                }
            }

            if let Value::String(s) = value {
                if crate::image_preview::is_image_url(s) {
                    resp.on_hover_ui_at_pointer(|ui| {
                        crate::image_preview::show_image_preview(ui, s);
                    });
                }
            }

            let leaf_pad = if plain_json {
                GUTTER_W + indent
            } else {
                ROW_LEFT_PAD + indent + CHEVRON_W + CHEVRON_GAP
            };
            ui_row.add_space(leaf_pad);
            render_tree_label(&mut ui_row, label, colors, plain_json, false);

            // Value formatting & coloring (No type badges, clean and airy)
            let (val_text, val_color) = match value {
                Value::String(_) if plain_json => (value.to_string(), colors.string),
                Value::String(s) => {
                    let sanitized = s
                        .replace("\r\n", "\\n")
                        .replace('\n', "\\n")
                        .replace('\r', "\\n");
                    (format!("\"{}\"", sanitized), colors.string)
                }
                Value::Number(n) => (n.to_string(), colors.number),
                Value::Bool(b) => (b.to_string(), colors.boolean),
                Value::Null => ("null".to_string(), colors.null),
                _ => unreachable!(),
            };

            row_label(
                &mut ui_row,
                egui::RichText::new(val_text)
                    .monospace()
                    .size(TREE_FONT_SIZE)
                    .color(val_color),
            );
            if plain_json && trailing_comma {
                row_label(
                    &mut ui_row,
                    egui::RichText::new(",")
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.punctuation),
                );
            }
        }
        let row_content_w = ui_row.min_rect().right() - rect.left();
        *max_row_w = max_row_w.max(row_content_w);
        ui.expand_to_include_rect(ui_row.min_rect());
    }

    let Some((_, close, count)) = container else {
        return;
    };
    if is_collapsed || count == 0 {
        return;
    }

    let mut child = |index, v: &Value, child_path: &str, label: TreeLabel<'_>| {
        render_tree_node(
            ui,
            v,
            child_path,
            label,
            depth + 1,
            collapsed,
            search_query,
            search_matches,
            cur_match_path,
            selected_path,
            colors,
            indent_step,
            plain_json,
            show_fold_arrows,
            index + 1 < count,
            action,
            max_row_w,
            current_match_rect,
        );
    };
    match value {
        Value::Object(map) => {
            for (i, (k, v)) in map.iter().enumerate() {
                child(i, v, &format!("{path}.{k}"), TreeLabel::Key(k));
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let index_label = i.to_string();
                child(
                    i,
                    v,
                    &format!("{path}[{i}]"),
                    TreeLabel::Index(&index_label),
                );
            }
        }
        _ => unreachable!(),
    }

    if plain_json {
        let (_, rect) = ui.allocate_space(egui::vec2(available_w, row_height));
        if ui.is_rect_visible(rect) {
            let text = if trailing_comma {
                format!("{close},")
            } else {
                close.to_string()
            };
            let galley = ui.painter().layout_no_wrap(
                text,
                egui::FontId::monospace(TREE_FONT_SIZE),
                colors.punctuation,
            );
            let pos = egui::pos2(
                rect.left() + GUTTER_W + indent,
                rect.center().y - galley.size().y / 2.0,
            );
            *max_row_w = max_row_w.max(GUTTER_W + indent + galley.size().x);
            ui.expand_to_include_rect(egui::Rect::from_min_size(pos, galley.size()));
            ui.painter().galley(pos, galley, colors.punctuation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_labels(
        value: &Value,
        plain_json: bool,
        mut collapsed: HashSet<String>,
    ) -> Vec<String> {
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_tree_node(
                    ui,
                    value,
                    "$",
                    TreeLabel::Root,
                    0,
                    &mut collapsed,
                    "",
                    &[],
                    None,
                    None,
                    ThemeColors::current(ctx),
                    16.0,
                    plain_json,
                    true,
                    false,
                    &mut None,
                    &mut 0.0,
                    &mut None,
                );
            });
        });
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn array_indices_and_object_keys_have_distinct_labels() {
        let value = serde_json::json!([7, {"0": true}, [null]]);
        assert_eq!(
            rendered_labels(&value, false, HashSet::new()),
            ["[3]", "0", "7", "1", "{1}", "0", ":", "true", "2", "[1]", "0", "null"]
        );
    }

    #[test]
    fn collapsed_tree_hides_children() {
        let value = serde_json::json!({
            "tags": ["rust", "json"],
            "user": {"name": "Alice", "active": true}
        });
        let collapsed = HashSet::from(["$.tags".to_owned(), "$.user".to_owned()]);
        assert_eq!(
            rendered_labels(&value, false, collapsed),
            ["{2}", "tags", "[2]", "user", "{2}"]
        );
    }

    #[test]
    fn expanded_plain_tree_renders_json_syntax() {
        let value = serde_json::json!([{"quote\"": "first\r\nsecond"}, [], {}, [true, null]]);
        let text = rendered_labels(&value, true, HashSet::new()).join("");
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
    }

    #[test]
    fn folded_plain_tree_keeps_delimiters_and_commas() {
        let value = serde_json::json!([{"hidden": true}, [1], []]);
        let collapsed = HashSet::from(["$[0]".to_owned(), "$[1]".to_owned()]);
        assert_eq!(
            rendered_labels(&value, true, collapsed),
            ["[", "{…},", "[…],", "[]", "]"]
        );
    }

    fn rendered_path_count(value: &Value, collapsed: HashSet<String>, show_fold_arrows: bool) -> usize {
        let ctx = egui::Context::default();
        let mut collapsed = collapsed;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_tree_node(
                    ui,
                    value,
                    "$",
                    TreeLabel::Root,
                    0,
                    &mut collapsed,
                    "",
                    &[],
                    None,
                    None,
                    ThemeColors::current(ctx),
                    16.0,
                    true,
                    show_fold_arrows,
                    false,
                    &mut None,
                    &mut 0.0,
                    &mut None,
                );
            });
        });
        output
            .shapes
            .iter()
            .filter(|shape| matches!(shape.shape, egui::Shape::Path(_)))
            .count()
    }

    #[test]
    fn fold_arrows_hide_when_expanded_unless_gutter_hovered_and_always_show_when_collapsed() {
        let value = serde_json::json!({"a": [1, 2], "b": {"c": 3}});
        // Expanded + not hovered => 0 chevron arrows
        assert_eq!(rendered_path_count(&value, HashSet::new(), false), 0);
        // Expanded + gutter hovered => 3 chevron arrows (root, $.a, $.b)
        assert_eq!(rendered_path_count(&value, HashSet::new(), true), 3);
        // Collapsed $.a + not hovered => 1 chevron arrow (only $.a)
        let collapsed = HashSet::from(["$.a".to_owned()]);
        assert_eq!(rendered_path_count(&value, collapsed, false), 1);
    }
}
