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

#[derive(Clone, Copy, PartialEq)]
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
                titlebar: rgb(20, 21, 23),
                canvas: rgb(20, 21, 23), surface: rgb(30, 31, 34),
                sidebar: rgb(20, 21, 23), subtle: rgb(38, 40, 45),
                hover: rgb(48, 50, 56), pressed: rgb(62, 65, 73),
                border: rgb(48, 50, 56), text: rgb(228, 229, 233),
                strong: rgb(245, 246, 248), muted: rgb(154, 158, 168),
                faint: rgb(108, 112, 122), disabled: rgb(76, 79, 88),
                blue: rgb(108, 178, 255), blue_bg: rgb(28, 48, 76),
                blue_border: rgb(56, 98, 152), primary: rgb(37, 99, 235),
                purple: rgb(183, 148, 255), amber: rgb(245, 189, 98),
                danger: rgb(248, 113, 113), danger_bg: rgb(68, 30, 34),
                danger_border: rgb(138, 58, 66), added: rgb(26, 56, 42),
            }
        } else {
            Self {
                titlebar: rgb(243, 244, 246),
                canvas: rgb(243, 244, 246), surface: egui::Color32::WHITE,
                sidebar: rgb(243, 244, 246), subtle: rgb(246, 247, 249),
                hover: rgb(232, 234, 239), pressed: rgb(214, 218, 226),
                border: rgb(225, 228, 233), text: rgb(31, 35, 40),
                strong: rgb(17, 20, 24), muted: rgb(101, 109, 118),
                faint: rgb(140, 149, 159), disabled: rgb(200, 206, 214),
                blue: rgb(9, 105, 218), blue_bg: rgb(221, 240, 255),
                blue_border: rgb(147, 197, 253), primary: rgb(9, 105, 218),
                purple: rgb(124, 58, 237), amber: rgb(180, 83, 9),
                danger: rgb(207, 34, 46), danger_bg: rgb(255, 235, 233),
                danger_border: rgb(252, 165, 165), added: rgb(218, 251, 225),
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

    // Popups, menus, tooltips and dialogs use clean solid surfaces with a crisp border.
    let dark = theme == egui::Theme::Dark;
    visuals.window_fill = p.surface;
    visuals.window_stroke = egui::Stroke::new(1.0, p.border);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.menu_corner_radius = egui::CornerRadius::same(10);
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 90 } else { 18 }),
    };
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: egui::Color32::from_black_alpha(if dark { 110 } else { 24 }),
    };

    ctx.set_visuals_of(theme, visuals);
    ctx.style_mut_of(theme, |style| {
        style.spacing.menu_width = 204.0;
        style.spacing.menu_margin = egui::Margin::symmetric(6, 6);
        style.spacing.window_margin = egui::Margin::same(16);
        // Refined macOS overlay scrollbars:
        style.spacing.scroll.bar_width = 4.0;
        style.spacing.scroll.floating_width = 4.0;
        style.spacing.scroll.handle_min_length = 20.0;
        style.spacing.scroll.bar_inner_margin = 1.0;
        style.spacing.scroll.bar_outer_margin = 2.0;
        style.spacing.scroll.floating = true;
        style.spacing.scroll.dormant_background_opacity = 0.0;
        style.spacing.scroll.active_background_opacity = 0.0;
        style.spacing.scroll.interact_background_opacity = 0.0;
        style.spacing.scroll.dormant_handle_opacity = 0.0;
        style.spacing.scroll.active_handle_opacity = 0.22;
        style.spacing.scroll.interact_handle_opacity = 0.45;
    });
    }
}

#[derive(Clone, Copy, PartialEq)]
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
                key: egui::Color32::from_rgb(199, 125, 187),     // Orchid (#C77DBB)
                string: egui::Color32::from_rgb(106, 171, 115),  // Sage Green (#6AAB73)
                number: egui::Color32::from_rgb(42, 172, 184),   // Teal (#2AACB8)
                boolean: egui::Color32::from_rgb(207, 142, 109), // Copper (#CF8E6D)
                null: egui::Color32::from_rgb(157, 160, 168),    // Muted Gray (#9DA0A8)
                colon: egui::Color32::from_rgb(122, 126, 133),
                punctuation: egui::Color32::from_rgb(188, 190, 196), // IDEA Dark (#BCBEC4)
                text: egui::Color32::from_rgb(228, 229, 233),
                fold_chip_bg: egui::Color32::from_rgb(48, 50, 56),
                fold_chip_fg: egui::Color32::from_rgb(198, 201, 210),
                badge_obj_bg: egui::Color32::from_rgb(48, 36, 75),
                badge_obj_fg: egui::Color32::from_rgb(183, 148, 255),
                badge_obj_stroke: egui::Color32::from_rgb(91, 67, 132),
                badge_arr_bg: egui::Color32::from_rgb(24, 53, 65),
                badge_arr_fg: egui::Color32::from_rgb(103, 211, 229),
                badge_arr_stroke: egui::Color32::from_rgb(40, 92, 108),
                match_highlight: egui::Color32::from_rgb(84, 68, 27),
                match_current: egui::Color32::from_rgb(118, 88, 28),
                row_hover: egui::Color32::from_white_alpha(10),
                row_selected: egui::Color32::from_rgb(32, 52, 82),
                indent_guide: egui::Color32::from_rgb(46, 48, 54),
                ui: UiColors::new(true),
            }
        } else {
            Self::light()
        }
    }

    fn light() -> Self {
        Self {
            key: egui::Color32::from_rgb(135, 16, 148),    // Plum (#871094)
            string: egui::Color32::from_rgb(6, 125, 23),   // Deep Green (#067D17)
            number: egui::Color32::from_rgb(23, 80, 235),  // Crisp Blue (#1750EB)
            boolean: egui::Color32::from_rgb(0, 51, 179),  // Navy (#0033B3)
            null: egui::Color32::from_rgb(108, 112, 126),  // Neutral Gray (#6C707E)
            colon: egui::Color32::from_rgb(140, 145, 158), // Muted Gray
            punctuation: egui::Color32::from_rgb(8, 8, 8), // IDEA Light (#080808)
            text: egui::Color32::from_rgb(31, 35, 40),
            fold_chip_bg: egui::Color32::from_rgb(232, 234, 239),
            fold_chip_fg: egui::Color32::from_rgb(87, 96, 106),
            badge_obj_bg: egui::Color32::from_rgb(245, 243, 255), // Lavender Pill
            badge_obj_fg: egui::Color32::from_rgb(124, 58, 237),
            badge_obj_stroke: egui::Color32::from_rgb(221, 214, 254),
            badge_arr_bg: egui::Color32::from_rgb(236, 254, 255), // Cyan Pill
            badge_arr_fg: egui::Color32::from_rgb(8, 145, 178),
            badge_arr_stroke: egui::Color32::from_rgb(186, 230, 253),
            match_highlight: egui::Color32::from_rgba_premultiplied(254, 240, 138, 160),
            match_current: egui::Color32::from_rgba_premultiplied(253, 224, 71, 240),
            row_hover: egui::Color32::from_rgba_premultiplied(0, 0, 0, 8),
            row_selected: egui::Color32::from_rgb(232, 243, 255),
            indent_guide: egui::Color32::from_rgb(232, 235, 240),
            ui: UiColors::new(false),
        }
    }
}
