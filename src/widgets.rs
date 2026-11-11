use eframe::egui;
use serde_json::Value;

use crate::i18n::tr;
use crate::icons::{draw_lucide_icon, LucideIcon};
use crate::keymap::{tip_with_shortcut, Action};
use crate::image_preview::is_image_url;
use crate::model::{SortMode, TreeAction};
use crate::theme::UiColors;

pub(crate) fn draw_button_bg(ui: &egui::Ui, rect: egui::Rect, resp: &egui::Response) {
    let p = UiColors::current(ui);
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), p.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), p.hover);
    }
}

pub(crate) fn btn_new_tab(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::Plus, UiColors::current(ui).blue, "New Tab", Some(Action::NewTab))
}

pub(crate) fn btn_sidebar(ui: &mut egui::Ui, shown: bool) -> egui::Response {
    let tip = if shown { "Hide sidebar" } else { "Show sidebar" };
    icon_button(ui, LucideIcon::PanelLeft, UiColors::current(ui).strong, tip, Some(Action::ToggleSidebar))
}

pub(crate) fn btn_format(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::WandSparkles, UiColors::current(ui).blue, "Format JSON (Pretty Print)", Some(Action::Format))
}

pub(crate) fn btn_sort(ui: &mut egui::Ui, mode: SortMode) -> egui::Response {
    let p = UiColors::current(ui);
    let (icon, tip, active) = match mode {
        SortMode::None => (LucideIcon::ArrowUpDown, "Sort: Original (Click to sort Key Ascending A→Z)", false),
        SortMode::Asc => (LucideIcon::ArrowDownAZ, "Sort: Key Ascending A→Z (Click to sort Descending Z→A)", true),
        SortMode::Desc => (LucideIcon::ArrowDownZA, "Sort: Key Descending Z→A (Click for Original)", true),
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
    if active {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), p.blue_bg);
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(4), egui::Stroke::new(1.0, p.blue_border), egui::StrokeKind::Inside);
    } else {
        draw_button_bg(ui, rect, &resp);
    }
    let color = if active { p.blue } else if resp.hovered() { p.strong } else { p.text };
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
    draw_lucide_icon(ui.painter(), icon_rect, icon, color);
    resp.on_hover_text(tip_text(ui.ctx(), tip, Some(Action::Sort)))
}

pub(crate) fn btn_undo(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    state_button(ui, LucideIcon::Undo, UiColors::current(ui).strong, "Undo (⌘Z)", enabled, None)
}

pub(crate) fn btn_redo(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    state_button(ui, LucideIcon::Redo, UiColors::current(ui).strong, "Redo (⌘⇧Z)", enabled, None)
}

pub(crate) fn btn_compress(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::FoldVertical, UiColors::current(ui).amber, "Copy Minified JSON", Some(Action::CopyMinified))
}

pub(crate) fn btn_simplify(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    state_button(ui, LucideIcon::Eraser, UiColors::current(ui).blue,
        "Extract Minimal JSON / Clear Values\n1st click: keep only the first array element with original values\n2nd click: clear values (numbers → 0, booleans → false, strings → \"\")", enabled, Some(Action::Simplify))
}

pub(crate) fn btn_copy(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::Copy, UiColors::current(ui).strong, "Copy Full JSON", Some(Action::CopyJson))
}

pub(crate) fn btn_clear(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::Trash, UiColors::current(ui).danger, "Clear Editor", Some(Action::Clear))
}

pub(crate) fn btn_compare(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::Columns2, UiColors::current(ui).purple, "Compare JSON in New Tab", Some(Action::Compare))
}

pub(crate) fn btn_expand_all(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::ChevronsUpDown, UiColors::current(ui).strong, "Expand All", Some(Action::ExpandAll))
}

pub(crate) fn btn_collapse_all(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, LucideIcon::ChevronsDownUp, UiColors::current(ui).strong, "Collapse All", Some(Action::CollapseAll))
}

/// View/Edit switch: shows the current mode's icon (eye or pencil); a click flips it.
pub(crate) fn btn_mode(ui: &mut egui::Ui, edit_mode: bool) -> egui::Response {
    let p = UiColors::current(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
    draw_button_bg(ui, rect, &resp);
    let (icon, color, tip) = if edit_mode {
        (LucideIcon::PencilLine, p.blue, "Edit mode · click to view")
    } else {
        (LucideIcon::Eye, p.blue, "View mode · click to edit")
    };
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
    draw_lucide_icon(ui.painter(), icon_rect, icon, color);
    resp.on_hover_text(tip_text(ui.ctx(), tip, Some(Action::ToggleEditMode)))
}

/// Small borderless icon button used inside the search pill.
pub(crate) fn mini_icon_button(ui: &mut egui::Ui, icon: LucideIcon, tip: &str) -> egui::Response {
    let p = UiColors::current(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), p.hover);
    }
    let color = if resp.hovered() { p.strong } else { p.muted };
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(12.0, 12.0));
    draw_lucide_icon(ui.painter(), icon_rect, icon, color);
    resp.on_hover_text(tr(ui.ctx(), tip))
}

/// Thin divider between toolbar groups.
pub(crate) fn toolbar_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 16.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, UiColors::current(ui).border),
    );
}

pub(crate) fn icon_button(ui: &mut egui::Ui, icon: LucideIcon, hover: egui::Color32, tip: &str, action: Option<Action>) -> egui::Response {
    state_button(ui, icon, hover, tip, true, action)
}

/// Translated tooltip, plus the action's shortcut when it has one.
fn tip_text(ctx: &egui::Context, tip: &str, action: Option<Action>) -> String {
    let tip = tr(ctx, tip);
    match action {
        Some(a) => tip_with_shortcut(ctx, tip, a),
        None => tip.to_string(),
    }
}

fn state_button(ui: &mut egui::Ui, icon: LucideIcon, hover: egui::Color32, tip: &str, enabled: bool, action: Option<Action>) -> egui::Response {
    let p = UiColors::current(ui);
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), sense);
    if enabled { draw_button_bg(ui, rect, &resp); }
    let color = if !enabled { p.disabled } else if resp.hovered() { hover } else { p.text };
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
    draw_lucide_icon(ui.painter(), icon_rect, icon, color);
    resp.on_hover_text(tip_text(ui.ctx(), tip, action))
}

/// Labels must not steal right-clicks from the tree row underneath.
pub(crate) fn row_label(ui: &mut egui::Ui, text: egui::RichText) {
    ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend).selectable(false).sense(egui::Sense::empty()));
}

pub(crate) fn menu_separator(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 9.0), egui::Sense::hover());
    ui.painter().hline(rect.left() + 8.0..=rect.right() - 8.0, rect.center().y, egui::Stroke::new(1.0, UiColors::current(ui).border));
}

pub(crate) fn menu_item(ui: &mut egui::Ui, label: &str, icon: LucideIcon, danger: bool) -> bool {
    let p = UiColors::current(ui);
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());
    if resp.hovered() {
        let bg = if danger { p.danger_bg } else { p.blue_bg };
        ui.painter().rect_filled(rect.shrink(1.0), egui::CornerRadius::same(8), bg);
    }
    let color = if danger { p.danger } else if resp.hovered() { p.strong } else { p.text };
    let icon_center = egui::pos2(rect.left() + 16.0, rect.center().y);
    let icon_rect = egui::Rect::from_center_size(icon_center, egui::vec2(15.0, 15.0));
    draw_lucide_icon(ui.painter(), icon_rect, icon, color);
    ui.painter().text(egui::pos2(rect.left() + 32.0, rect.center().y), egui::Align2::LEFT_CENTER,
        tr(ui.ctx(), label), egui::FontId::proportional(13.0), color);
    resp.clicked()
}

pub(crate) fn show_tree_context_menu(resp: &egui::Response, value: &Value, path: &str, key_label: Option<&str>, is_container: bool, action: &mut Option<TreeAction>) {
    resp.context_menu(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 1.0);
        ui.set_min_width(188.0);
        ui.set_max_width(220.0);
        let title = key_label.filter(|k| !k.is_empty()).unwrap_or(if path == "$" { tr(ui.ctx(), "root") } else { path });
        ui.add(egui::Label::new(egui::RichText::new(title).size(11.0).color(UiColors::current(ui).muted)).truncate().selectable(false));
        menu_separator(ui);
        if let Some(k) = key_label {
            if menu_item(ui, "Copy Key", LucideIcon::Key, false) {
                *action = Some(TreeAction::CopyText(k.to_string()));
                ui.close_menu();
            }
        }
        if is_container {
            if menu_item(ui, "Copy JSON", LucideIcon::Braces, false) {
                let json_str = serde_json::to_string_pretty(value).unwrap_or_default();
                *action = Some(TreeAction::CopyText(json_str));
                ui.close_menu();
            }
        } else if menu_item(ui, "Copy Value", LucideIcon::Type, false) {
            let val_str = match value { Value::String(s) => s.clone(), other => other.to_string() };
            *action = Some(TreeAction::CopyText(val_str));
            ui.close_menu();
        }
        if menu_item(ui, "Copy Path", LucideIcon::Route, false) {
            *action = Some(TreeAction::CopyText(path.to_string()));
            ui.close_menu();
        }
        if let Value::String(s) = value {
            if is_image_url(s) && menu_item(ui, "Download Image", LucideIcon::Download, false) {
                *action = Some(TreeAction::DownloadImage(s.clone()));
                ui.close_menu();
            }
        }
        menu_separator(ui);
        if is_container {
            if menu_item(ui, "Expand Subtree", LucideIcon::ChevronsDown, false) {
                *action = Some(TreeAction::ExpandSubtree(path.to_string()));
                ui.close_menu();
            }
            if menu_item(ui, "Collapse Subtree", LucideIcon::ChevronsUp, false) {
                *action = Some(TreeAction::CollapseSubtree(path.to_string(), value.clone()));
                ui.close_menu();
            }
        } else if menu_item(ui, "Edit Value", LucideIcon::Pencil, false) {
            let k_label = key_label.unwrap_or("").to_string();
            *action = Some(TreeAction::StartEdit(path.to_string(), k_label, value.clone()));
            ui.close_menu();
        }
        if path != "$" {
            menu_separator(ui);
            if menu_item(ui, "Delete", LucideIcon::Trash, true) {
                *action = Some(TreeAction::DeleteNode(path.to_string()));
                ui.close_menu();
            }
        }
    });
}

pub(crate) fn type_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let label = tr(ui.ctx(), label);
    let p = UiColors::current(ui);
    let font = egui::FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), p.text);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 22.0, 28.0), egui::Sense::click());
    let (bg, fg) = if selected { (p.primary, egui::Color32::WHITE) } else if resp.hovered() { (p.hover, p.strong) } else { (p.subtle, p.text) };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(14), bg);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, fg);
    resp
}

pub(crate) fn modal_button(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    let p = UiColors::current(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(92.0, 32.0), egui::Sense::click());
    let (bg, fg, stroke) = if primary {
        let bg = if resp.is_pointer_button_down_on() { p.primary.gamma_multiply(0.85) } else if resp.hovered() { p.primary.gamma_multiply(1.1) } else { p.primary };
        (bg, egui::Color32::WHITE, egui::Stroke::NONE)
    } else if resp.hovered() { (p.hover, p.strong, egui::Stroke::new(1.0, p.pressed)) }
    else { (p.surface, p.text, egui::Stroke::new(1.0, p.border)) };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(8), bg);
    if stroke != egui::Stroke::NONE {
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(8), stroke, egui::StrokeKind::Inside);
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, tr(ui.ctx(), label), egui::FontId::proportional(13.0), fg);
    resp
}

/// macOS-style segmented control. Returns the index clicked this frame, if any.
pub(crate) fn segmented(ui: &mut egui::Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let p = UiColors::current(ui);
    let font = egui::FontId::proportional(12.5);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| ui.painter().layout_no_wrap(tr(ui.ctx(), l).to_owned(), font.clone(), p.text).size().x + 24.0)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total, 30.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::same(9), p.subtle);
    ui.painter().rect_stroke(rect, egui::CornerRadius::same(9), egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let mut clicked = None;
    let mut x = rect.left() + 2.0;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let seg = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), egui::vec2(*w, rect.height() - 4.0));
        x += w;
        let resp = ui.interact(seg, ui.id().with(("segment", i)), egui::Sense::click());
        if i == selected {
            let shadow = egui::epaint::Shadow { offset: [0, 1], blur: 3, spread: 0, color: egui::Color32::from_black_alpha(if ui.visuals().dark_mode { 80 } else { 22 }) };
            ui.painter().add(shadow.as_shape(seg, egui::CornerRadius::same(7)));
            ui.painter().rect_filled(seg, egui::CornerRadius::same(7), p.surface);
        } else if resp.hovered() {
            ui.painter().rect_filled(seg, egui::CornerRadius::same(7), p.hover.gamma_multiply(0.6));
        }
        let color = if i == selected { p.strong } else if resp.hovered() { p.text } else { p.muted };
        ui.painter().text(seg.center(), egui::Align2::CENTER_CENTER, tr(ui.ctx(), label), font.clone(), color);
        if resp.clicked() {
            clicked = Some(i);
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
}

/// Small dialog button (64x28), for secondary dialogs like the shortcut panel.
pub(crate) fn compact_button(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    let p = UiColors::current(ui);
    let text = tr(ui.ctx(), label);
    let font = egui::FontId::proportional(12.5);
    let w = (ui.painter().layout_no_wrap(text.to_owned(), font.clone(), p.text).size().x + 28.0).max(64.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 28.0), egui::Sense::click());
    let (bg, fg, stroke) = if primary {
        let bg = if resp.is_pointer_button_down_on() { p.primary.gamma_multiply(0.85) } else { p.primary };
        (bg, egui::Color32::WHITE, egui::Stroke::NONE)
    } else if resp.hovered() {
        (p.hover, p.strong, egui::Stroke::new(1.0, p.pressed))
    } else {
        (p.surface, p.text, egui::Stroke::new(1.0, p.border))
    };
    ui.painter().rect(rect, egui::CornerRadius::same(7), bg, stroke, egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
