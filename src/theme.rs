use eframe::egui;

use crate::icons::LucideIcon;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    #[default]
    Light,
    Dark,
    System,
}

impl ThemeMode {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
            Self::System => Self::Light,
        }
    }

    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub(crate) fn icon(self) -> LucideIcon {
        match self {
            Self::Light => LucideIcon::Sun,
            Self::Dark => LucideIcon::MoonStar,
            Self::System => LucideIcon::SunMoon,
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::Light => "Light · click for Dark",
            Self::Dark => "Dark · click for System",
            Self::System => "System · click for Light",
        }
    }

    pub(crate) fn apply(self, ctx: &egui::Context) {
        let (preference, native) = match self {
            Self::Light => (egui::ThemePreference::Light, egui::SystemTheme::Light),
            Self::Dark => (egui::ThemePreference::Dark, egui::SystemTheme::Dark),
            Self::System => (egui::ThemePreference::System, egui::SystemTheme::SystemDefault),
        };
        ctx.set_theme(preference);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(native));
        ctx.request_repaint();
    }
}

#[derive(Clone, Copy)]
pub(crate) struct UiColors {
    pub(crate) titlebar: egui::Color32,
    pub(crate) canvas: egui::Color32,
    pub(crate) surface: egui::Color32,
    pub(crate) sidebar: egui::Color32,
    pub(crate) subtle: egui::Color32,
    pub(crate) hover: egui::Color32,
    pub(crate) pressed: egui::Color32,
    pub(crate) border: egui::Color32,
    pub(crate) text: egui::Color32,
    pub(crate) strong: egui::Color32,
    pub(crate) muted: egui::Color32,
    pub(crate) faint: egui::Color32,
    pub(crate) disabled: egui::Color32,
    pub(crate) blue: egui::Color32,
    pub(crate) blue_bg: egui::Color32,
    pub(crate) blue_border: egui::Color32,
    pub(crate) primary: egui::Color32,
    pub(crate) purple: egui::Color32,
    pub(crate) amber: egui::Color32,
    pub(crate) danger: egui::Color32,
    pub(crate) danger_bg: egui::Color32,
    pub(crate) danger_border: egui::Color32,
    pub(crate) added: egui::Color32,
}

impl UiColors {
    pub(crate) fn current(ui: &egui::Ui) -> Self {
        Self::new(ui.visuals().dark_mode)
    }

    pub(crate) fn from_ctx(ctx: &egui::Context) -> Self {
        Self::new(ctx.theme() == egui::Theme::Dark)
    }

    pub(crate) fn new(dark: bool) -> Self {
        let rgb = egui::Color32::from_rgb;
        if dark {
            Self {
                titlebar: rgb(23, 29, 41),
                canvas: rgb(17, 21, 29), surface: rgb(27, 33, 48),
                sidebar: rgb(21, 27, 39), subtle: rgb(35, 44, 61),
                hover: rgb(48, 61, 82), pressed: rgb(64, 81, 107),
                border: rgb(52, 65, 86), text: rgb(226, 232, 240),
                strong: rgb(241, 245, 249), muted: rgb(168, 182, 204),
                faint: rgb(116, 132, 158), disabled: rgb(77, 90, 112),
                blue: rgb(121, 184, 255), blue_bg: rgb(35, 60, 97),
                blue_border: rgb(66, 109, 158), primary: rgb(39, 103, 169),
                purple: rgb(183, 148, 255), amber: rgb(246, 193, 107),
                danger: rgb(255, 146, 146), danger_bg: rgb(75, 38, 49),
                danger_border: rgb(143, 70, 87), added: rgb(30, 61, 50),
            }
        } else {
            Self {
                titlebar: rgb(236, 238, 242),
                canvas: rgb(246, 248, 250), surface: egui::Color32::WHITE,
                sidebar: rgb(243, 244, 248), subtle: rgb(241, 245, 249),
                hover: rgb(226, 232, 240), pressed: rgb(203, 213, 225),
                border: rgb(226, 232, 240), text: rgb(30, 41, 59),
                strong: rgb(15, 23, 42), muted: rgb(100, 116, 139),
                faint: rgb(148, 163, 184), disabled: rgb(203, 213, 225),
                blue: rgb(9, 105, 218), blue_bg: rgb(219, 234, 254),
                blue_border: rgb(147, 197, 253), primary: rgb(15, 108, 189),
                purple: rgb(124, 58, 237), amber: rgb(180, 83, 9),
                danger: rgb(185, 28, 28), danger_bg: rgb(254, 226, 226),
                danger_border: rgb(252, 165, 165), added: rgb(220, 252, 231),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Theme & Styling (Daytime / Light Theme)
// ---------------------------------------------------------------------------

pub(crate) fn setup_theme(ctx: &egui::Context) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let p = UiColors::new(theme == egui::Theme::Dark);
        let mut visuals = theme.default_visuals();

    visuals.panel_fill = p.canvas;
    visuals.extreme_bg_color = p.surface;
    visuals.faint_bg_color = p.subtle;

    visuals.widgets.noninteractive.bg_fill = p.canvas;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, p.text);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.border);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.inactive.bg_fill = p.subtle;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, p.text);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.border);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.hovered.bg_fill = p.hover;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, p.strong);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.pressed);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.active.bg_fill = p.primary;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);

    visuals.widgets.open.bg_fill = p.surface;
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(5);

    visuals.selection.bg_fill = p.blue_bg;
    visuals.selection.stroke = egui::Stroke::new(1.0, p.blue);

    // Refined editor text cursor: softer, lighter modern blue
    visuals.text_cursor = egui::style::TextCursorStyle {
        stroke: egui::Stroke::new(1.2, egui::Color32::from_rgb(96, 165, 250)),
        preview: false,
        blink: true,
        on_duration: 0.55,
        off_duration: 0.45,
    };

    visuals.window_fill = p.surface;
    visuals.window_stroke = egui::Stroke::new(1.0, p.border);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.menu_corner_radius = egui::CornerRadius::same(10);
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 22,
        spread: 0,
        color: egui::Color32::from_black_alpha(32),
    };
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 28,
        spread: 0,
        color: egui::Color32::from_black_alpha(36),
    };

    ctx.set_visuals_of(theme, visuals);
    ctx.style_mut_of(theme, |style| {
        style.spacing.menu_width = 204.0;
        style.spacing.menu_margin = egui::Margin::symmetric(6, 6);
        style.spacing.window_margin = egui::Margin::same(16);
    });
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ThemeColors {
    pub(crate) key: egui::Color32,
    pub(crate) string: egui::Color32,
    pub(crate) number: egui::Color32,
    pub(crate) boolean: egui::Color32,
    pub(crate) null: egui::Color32,
    pub(crate) colon: egui::Color32,
    pub(crate) punctuation: egui::Color32,
    pub(crate) text: egui::Color32,
    pub(crate) fold_chip_bg: egui::Color32,
    pub(crate) fold_chip_fg: egui::Color32,
    pub(crate) badge_obj_bg: egui::Color32,
    pub(crate) badge_obj_fg: egui::Color32,
    pub(crate) badge_obj_stroke: egui::Color32,
    pub(crate) badge_arr_bg: egui::Color32,
    pub(crate) badge_arr_fg: egui::Color32,
    pub(crate) badge_arr_stroke: egui::Color32,
    pub(crate) match_highlight: egui::Color32,
    pub(crate) match_current: egui::Color32,
    pub(crate) row_hover: egui::Color32,
    pub(crate) row_selected: egui::Color32,
    pub(crate) indent_guide: egui::Color32,
    pub(crate) ui: UiColors,
}

impl ThemeColors {
    pub(crate) fn current(ctx: &egui::Context) -> Self {
        if ctx.theme() == egui::Theme::Dark {
            Self {
                key: egui::Color32::from_rgb(121, 184, 255),
                string: egui::Color32::from_rgb(134, 219, 165),
                number: egui::Color32::from_rgb(255, 184, 108),
                boolean: egui::Color32::from_rgb(255, 146, 146),
                null: egui::Color32::from_rgb(168, 182, 204),
                colon: egui::Color32::from_rgb(116, 132, 158),
                punctuation: egui::Color32::from_rgb(148, 163, 184),
                text: egui::Color32::from_rgb(226, 232, 240),
                fold_chip_bg: egui::Color32::from_rgb(48, 61, 82),
                fold_chip_fg: egui::Color32::from_rgb(203, 213, 225),
                badge_obj_bg: egui::Color32::from_rgb(48, 36, 75),
                badge_obj_fg: egui::Color32::from_rgb(183, 148, 255),
                badge_obj_stroke: egui::Color32::from_rgb(91, 67, 132),
                badge_arr_bg: egui::Color32::from_rgb(24, 53, 65),
                badge_arr_fg: egui::Color32::from_rgb(103, 211, 229),
                badge_arr_stroke: egui::Color32::from_rgb(40, 92, 108),
                match_highlight: egui::Color32::from_rgb(84, 68, 27),
                match_current: egui::Color32::from_rgb(118, 88, 28),
                row_hover: egui::Color32::from_white_alpha(12),
                row_selected: egui::Color32::from_rgb(35, 60, 97),
                indent_guide: egui::Color32::from_rgb(52, 65, 86),
                ui: UiColors::new(true),
            }
        } else {
            Self::light()
        }
    }

    fn light() -> Self {
        Self {
            key: egui::Color32::from_rgb(9, 105, 218), // macOS/GitHub Deep Blue
            string: egui::Color32::from_rgb(21, 128, 61), // Forest Green
            number: egui::Color32::from_rgb(194, 65, 12), // Amber Orange
            boolean: egui::Color32::from_rgb(185, 28, 28), // Crimson Red
            null: egui::Color32::from_rgb(100, 116, 139), // Slate Gray
            colon: egui::Color32::from_rgb(148, 163, 184), // Light Muted Gray
            punctuation: egui::Color32::from_rgb(71, 85, 105),
            text: egui::Color32::from_rgb(30, 41, 59),
            fold_chip_bg: egui::Color32::from_rgb(226, 232, 240),
            fold_chip_fg: egui::Color32::from_rgb(71, 85, 105),
            badge_obj_bg: egui::Color32::from_rgb(245, 243, 255), // Lavender Pill
            badge_obj_fg: egui::Color32::from_rgb(124, 58, 237),
            badge_obj_stroke: egui::Color32::from_rgb(221, 214, 254),
            badge_arr_bg: egui::Color32::from_rgb(236, 254, 255), // Cyan Pill
            badge_arr_fg: egui::Color32::from_rgb(8, 145, 178),
            badge_arr_stroke: egui::Color32::from_rgb(186, 230, 253),
            match_highlight: egui::Color32::from_rgba_premultiplied(254, 240, 138, 160),
            match_current: egui::Color32::from_rgba_premultiplied(253, 224, 71, 240),
            row_hover: egui::Color32::from_rgba_premultiplied(0, 0, 0, 10),
            row_selected: egui::Color32::from_rgba_premultiplied(15, 108, 189, 28),
            indent_guide: egui::Color32::from_rgb(226, 232, 240),
            ui: UiColors::new(false),
        }
    }
}
