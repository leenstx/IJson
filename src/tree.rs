use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;

use crate::icons::{draw_lucide_icon, LucideIcon};
use crate::model::TreeAction;
use crate::theme::ThemeColors;
use crate::widgets::{row_label, show_tree_context_menu};

const TREE_FONT_SIZE: f32 = 13.0;
const BADGE_FONT_SIZE: f32 = 12.0;

pub(crate) fn render_tree_node(
    ui: &mut egui::Ui,
    value: &Value,
    path: &str,
    key_label: Option<&str>,
    depth: usize,
    collapsed: &mut HashSet<String>,
    search_query: &str,
    search_matches: &[String],
    cur_match_path: Option<&str>,
    selected_path: Option<&str>,
    colors: ThemeColors,
    action: &mut Option<TreeAction>,
    max_row_w: &mut f32,
    current_match_rect: &mut Option<egui::Rect>,
) {
    let indent_step = 20.0;
    let base_x = ui.cursor().min.x;
    let indent = depth as f32 * indent_step;

    let is_search_match = !search_query.is_empty() && search_matches.iter().any(|m| m == path);
    let is_current_match = cur_match_path == Some(path);
    let is_selected = selected_path == Some(path);

    let row_height = 25.0;
    let available_w = ui.available_width();

    match value {
        Value::Object(map) => {
            let is_collapsed = collapsed.contains(path);
            let count = map.len();

            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(available_w, row_height), egui::Sense::click());
            if is_current_match {
                *current_match_rect = Some(rect);
            }

            // Row background highlight
            if is_current_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_current);
            } else if is_search_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_highlight);
            } else if is_selected {
                ui.painter().rect_filled(rect, 4.0, colors.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, colors.row_hover);
            }

            // Draw vertical indentation guidelines
            for i in 0..depth {
                let line_x = base_x + i as f32 * indent_step + 10.0;
                ui.painter().vline(
                    line_x,
                    rect.top()..=rect.bottom(),
                    egui::Stroke::new(1.0, colors.indent_guide),
                );
            }

            show_tree_context_menu(&resp, value, path, key_label, true, action);

            if resp.clicked() {
                *action = Some(TreeAction::Select(path.to_string()));
            }

            // Render row content. Labels are non-interactive so right-click hits this row.
            let mut ui_row = ui.new_child(
                egui::UiBuilder::default()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            ui_row.style_mut().interaction.selectable_labels = false;

            ui_row.add_space(indent);

            let chevron_color = if is_collapsed {
                colors.ui.muted
            } else {
                colors.ui.blue
            };
            let (chev_rect, chev_resp) =
                ui_row.allocate_exact_size(egui::vec2(16.0, 18.0), egui::Sense::click());
            show_tree_context_menu(&chev_resp, value, path, key_label, true, action);
            let chev_icon = if is_collapsed {
                LucideIcon::ChevronRight
            } else {
                LucideIcon::ChevronDown
            };
            let icon_rect =
                egui::Rect::from_center_size(chev_rect.center(), egui::vec2(12.0, 12.0));
            draw_lucide_icon(ui_row.painter(), icon_rect, chev_icon, chevron_color);
            if chev_resp.clicked() {
                if is_collapsed {
                    collapsed.remove(path);
                } else {
                    collapsed.insert(path.to_string());
                }
            }

            if let Some(k) = key_label {
                row_label(
                    &mut ui_row,
                    egui::RichText::new(format!("\"{}\"", k))
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.key),
                );
                row_label(
                    &mut ui_row,
                    egui::RichText::new(":")
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.colon),
                );
            }

            // Object Badge Pill
            let badge_text = if count == 0 {
                "{}".to_string()
            } else {
                format!("{{ {} }}", count)
            };
            let badge = egui::RichText::new(badge_text)
                .monospace()
                .size(BADGE_FONT_SIZE)
                .color(colors.badge_obj_fg);

            let pill_frame = egui::Frame::default()
                .fill(colors.badge_obj_bg)
                .stroke(egui::Stroke::new(0.5, colors.badge_obj_stroke))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(5, 1));

            pill_frame.show(&mut ui_row, |ui| {
                row_label(ui, badge);
            });
            let row_content_w = ui_row.min_rect().right() - rect.left();
            *max_row_w = max_row_w.max(row_content_w);
            ui.expand_to_include_rect(ui_row.min_rect());

            // Recursively render children if not collapsed
            if !is_collapsed {
                for (k, v) in map {
                    let child_path = format!("{}.{}", path, k);
                    render_tree_node(
                        ui,
                        v,
                        &child_path,
                        Some(k),
                        depth + 1,
                        collapsed,
                        search_query,
                        search_matches,
                        cur_match_path,
                        selected_path,
                        colors,
                        action,
                        max_row_w,
                        current_match_rect,
                    );
                }
            }
        }
        Value::Array(arr) => {
            let is_collapsed = collapsed.contains(path);
            let count = arr.len();

            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(available_w, row_height), egui::Sense::click());
            if is_current_match {
                *current_match_rect = Some(rect);
            }

            // Row background highlight
            if is_current_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_current);
            } else if is_search_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_highlight);
            } else if is_selected {
                ui.painter().rect_filled(rect, 4.0, colors.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, colors.row_hover);
            }

            // Draw vertical indentation guidelines
            for i in 0..depth {
                let line_x = base_x + i as f32 * indent_step + 10.0;
                ui.painter().vline(
                    line_x,
                    rect.top()..=rect.bottom(),
                    egui::Stroke::new(1.0, colors.indent_guide),
                );
            }

            show_tree_context_menu(&resp, value, path, key_label, true, action);

            if resp.clicked() {
                *action = Some(TreeAction::Select(path.to_string()));
            }

            let mut ui_row = ui.new_child(
                egui::UiBuilder::default()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            ui_row.style_mut().interaction.selectable_labels = false;

            ui_row.add_space(indent);

            let chevron_color = if is_collapsed {
                colors.ui.muted
            } else {
                colors.ui.blue
            };
            let (chev_rect, chev_resp) =
                ui_row.allocate_exact_size(egui::vec2(16.0, 18.0), egui::Sense::click());
            show_tree_context_menu(&chev_resp, value, path, key_label, true, action);
            let chev_icon = if is_collapsed {
                LucideIcon::ChevronRight
            } else {
                LucideIcon::ChevronDown
            };
            let icon_rect =
                egui::Rect::from_center_size(chev_rect.center(), egui::vec2(12.0, 12.0));
            draw_lucide_icon(ui_row.painter(), icon_rect, chev_icon, chevron_color);
            if chev_resp.clicked() {
                if is_collapsed {
                    collapsed.remove(path);
                } else {
                    collapsed.insert(path.to_string());
                }
            }

            if let Some(k) = key_label {
                row_label(
                    &mut ui_row,
                    egui::RichText::new(format!("\"{}\"", k))
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.key),
                );
                row_label(
                    &mut ui_row,
                    egui::RichText::new(":")
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.colon),
                );
            }

            // Array Badge Pill
            let badge_text = if count == 0 {
                "[]".to_string()
            } else {
                format!("[ {} ]", count)
            };
            let badge = egui::RichText::new(badge_text)
                .monospace()
                .size(BADGE_FONT_SIZE)
                .color(colors.badge_arr_fg);

            let pill_frame = egui::Frame::default()
                .fill(colors.badge_arr_bg)
                .stroke(egui::Stroke::new(0.5, colors.badge_arr_stroke))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(5, 1));

            pill_frame.show(&mut ui_row, |ui| {
                row_label(ui, badge);
            });
            let row_content_w = ui_row.min_rect().right() - rect.left();
            *max_row_w = max_row_w.max(row_content_w);
            ui.expand_to_include_rect(ui_row.min_rect());

            // Recursively render array items if not collapsed
            if !is_collapsed {
                for (i, v) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", path, i);
                    let idx_label = format!("[{}]", i);
                    render_tree_node(
                        ui,
                        v,
                        &child_path,
                        Some(&idx_label),
                        depth + 1,
                        collapsed,
                        search_query,
                        search_matches,
                        cur_match_path,
                        selected_path,
                        colors,
                        action,
                        max_row_w,
                        current_match_rect,
                    );
                }
            }
        }
        // Leaf value (String, Number, Bool, Null) - No type tags shown
        _ => {
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(available_w, row_height), egui::Sense::click());
            if is_current_match {
                *current_match_rect = Some(rect);
            }

            // Row background highlight
            if is_current_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_current);
            } else if is_search_match {
                ui.painter().rect_filled(rect, 4.0, colors.match_highlight);
            } else if is_selected {
                ui.painter().rect_filled(rect, 4.0, colors.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, colors.row_hover);
            }

            // Draw vertical indentation guidelines
            for i in 0..depth {
                let line_x = base_x + i as f32 * indent_step + 10.0;
                ui.painter().vline(
                    line_x,
                    rect.top()..=rect.bottom(),
                    egui::Stroke::new(1.0, colors.indent_guide),
                );
            }

            show_tree_context_menu(&resp, value, path, key_label, false, action);

            if resp.clicked() {
                *action = Some(TreeAction::Select(path.to_string()));
            }

            // Double click to edit value
            if resp.double_clicked() {
                let k_label = key_label.unwrap_or("").to_string();
                *action = Some(TreeAction::StartEdit(
                    path.to_string(),
                    k_label,
                    value.clone(),
                ));
            }

            if let Value::String(s) = value {
                if crate::image_preview::is_image_url(s) {
                    resp.on_hover_ui_at_pointer(|ui| {
                        crate::image_preview::show_image_preview(ui, s);
                    });
                }
            }

            let mut ui_row = ui.new_child(
                egui::UiBuilder::default()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            ui_row.style_mut().interaction.selectable_labels = false;

            ui_row.add_space(indent + 18.0);

            if let Some(k) = key_label {
                row_label(
                    &mut ui_row,
                    egui::RichText::new(format!("\"{}\"", k))
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.key),
                );
                row_label(
                    &mut ui_row,
                    egui::RichText::new(":")
                        .monospace()
                        .size(TREE_FONT_SIZE)
                        .color(colors.colon),
                );
            }

            // Value formatting & coloring (No type badges, clean and airy)
            let (val_text, val_color) = match value {
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
            let row_content_w = ui_row.min_rect().right() - rect.left();
            *max_row_w = max_row_w.max(row_content_w);
            ui.expand_to_include_rect(ui_row.min_rect());
        }
    }
}
