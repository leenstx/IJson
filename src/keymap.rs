//! Customizable keyboard shortcuts.

use std::collections::HashMap;

use eframe::egui;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Action {
    ToggleSidebar,
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    ToggleEditMode,
    Search,
    ExpandAll,
    CollapseAll,
    Format,
    Sort,
    Simplify,
    CopyJson,
    CopyMinified,
    Compare,
    Clear,
}

impl Action {
    /// Display order in the settings panel.
    pub(crate) const ALL: [Action; 16] = [
        Action::ToggleSidebar,
        Action::NewTab,
        Action::CloseTab,
        Action::NextTab,
        Action::PrevTab,
        Action::ToggleEditMode,
        Action::Search,
        Action::ExpandAll,
        Action::CollapseAll,
        Action::Format,
        Action::Sort,
        Action::Simplify,
        Action::CopyJson,
        Action::CopyMinified,
        Action::Compare,
        Action::Clear,
    ];

    /// Stable name used in the saved session.
    fn id(self) -> &'static str {
        match self {
            Action::ToggleSidebar => "toggle_sidebar",
            Action::NewTab => "new_tab",
            Action::CloseTab => "close_tab",
            Action::NextTab => "next_tab",
            Action::PrevTab => "prev_tab",
            Action::ToggleEditMode => "toggle_edit_mode",
            Action::Search => "search",
            Action::ExpandAll => "expand_all",
            Action::CollapseAll => "collapse_all",
            Action::Format => "format",
            Action::Sort => "sort",
            Action::Simplify => "simplify",
            Action::CopyJson => "copy_json",
            Action::CopyMinified => "copy_minified",
            Action::Compare => "compare",
            Action::Clear => "clear",
        }
    }

    /// English label; translated by the caller.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Action::ToggleSidebar => "Show / hide sidebar",
            Action::NewTab => "New tab",
            Action::CloseTab => "Close tab",
            Action::NextTab => "Next tab",
            Action::PrevTab => "Previous tab",
            Action::ToggleEditMode => "Switch View / Edit",
            Action::Search => "Search",
            Action::ExpandAll => "Expand all",
            Action::CollapseAll => "Collapse all",
            Action::Format => "Format JSON",
            Action::Sort => "Sort keys",
            Action::Simplify => "Simplify JSON",
            Action::CopyJson => "Copy JSON",
            Action::CopyMinified => "Copy minified JSON",
            Action::Compare => "Compare in new tab",
            Action::Clear => "Clear editor",
        }
    }

    fn default_shortcut(self) -> Option<Shortcut> {
        use egui::Key;
        let cmd = |key| Some(Shortcut { cmd: true, shift: false, alt: false, ctrl: false, key });
        let cmd_shift = |key| Some(Shortcut { cmd: true, shift: true, alt: false, ctrl: false, key });
        match self {
            Action::ToggleSidebar => cmd(Key::S),
            Action::NewTab => cmd(Key::T),
            Action::CloseTab => cmd(Key::W),
            Action::NextTab => cmd_shift(Key::CloseBracket),
            Action::PrevTab => cmd_shift(Key::OpenBracket),
            Action::ToggleEditMode => cmd(Key::Slash),
            Action::Search => cmd(Key::F),
            Action::ExpandAll => cmd(Key::Plus),
            Action::CollapseAll => cmd(Key::Minus),
            Action::Format => cmd_shift(Key::F),
            Action::Sort => cmd_shift(Key::S),
            Action::Simplify => None,
            Action::CopyJson => cmd_shift(Key::C),
            Action::CopyMinified => cmd_shift(Key::M),
            Action::Compare => cmd_shift(Key::D),
            // Destructive, so not bound unless the user asks for it.
            Action::Clear => None,
        }
    }
}

/// A key plus modifiers, matched exactly (⌘⇧F does not trigger ⌘F).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Shortcut {
    pub(crate) cmd: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    /// The Control key on macOS (elsewhere Control is `cmd`).
    pub(crate) ctrl: bool,
    pub(crate) key: egui::Key,
}

impl Shortcut {
    fn from_event(key: egui::Key, m: egui::Modifiers) -> Self {
        // "=" and "+" share a key; treat them as one so ⌘= and ⌘+ both work.
        let (key, shift) = match key {
            egui::Key::Equals | egui::Key::Plus => (egui::Key::Plus, false),
            _ => (key, m.shift),
        };
        Self {
            cmd: m.command,
            shift,
            alt: m.alt,
            ctrl: m.ctrl && !m.command,
            key,
        }
    }

    /// Whether this key press can be a shortcut: it needs a modifier, unless it is a function key.
    fn is_bindable(&self) -> bool {
        let function_key = matches!(
            self.key,
            egui::Key::F1
                | egui::Key::F2
                | egui::Key::F3
                | egui::Key::F4
                | egui::Key::F5
                | egui::Key::F6
                | egui::Key::F7
                | egui::Key::F8
                | egui::Key::F9
                | egui::Key::F10
                | egui::Key::F11
                | egui::Key::F12
        );
        self.cmd || self.ctrl || self.alt || function_key
    }

    /// Short form, e.g. "⌘⇧F" on macOS or "Ctrl+Shift+F" elsewhere.
    pub(crate) fn label(&self) -> String {
        let key = match self.key {
            egui::Key::Plus => "+",
            egui::Key::Minus => "-",
            k => k.symbol_or_name(),
        };
        if cfg!(target_os = "macos") {
            let mut s = String::new();
            if self.ctrl {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.shift {
                s.push('⇧');
            }
            if self.cmd {
                s.push('⌘');
            }
            s + key
        } else {
            let mut parts = Vec::new();
            if self.cmd || self.ctrl {
                parts.push("Ctrl");
            }
            if self.alt {
                parts.push("Alt");
            }
            if self.shift {
                parts.push("Shift");
            }
            parts.push(key);
            parts.join("+")
        }
    }

    /// The individual keys, for drawing as keycaps: ["⌘", "⇧", "F"] or ["Ctrl", "Shift", "F"].
    pub(crate) fn keys(&self) -> Vec<String> {
        if cfg!(target_os = "macos") {
            let label = self.label();
            let mut chars = label.chars();
            let mut keys = Vec::new();
            // Modifier symbols come first, one char each; the rest is the key.
            let mut rest = String::new();
            for c in chars.by_ref() {
                if matches!(c, '⌃' | '⌥' | '⇧' | '⌘') && rest.is_empty() {
                    keys.push(c.to_string());
                } else {
                    rest.push(c);
                }
            }
            keys.push(rest);
            keys
        } else {
            self.label().split('+').filter(|k| !k.is_empty()).map(str::to_string).collect()
        }
    }

    fn to_code(self) -> String {
        let mut parts = Vec::new();
        if self.cmd {
            parts.push("cmd");
        }
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    fn from_code(code: &str) -> Option<Self> {
        let mut s = Shortcut { cmd: false, shift: false, alt: false, ctrl: false, key: egui::Key::A };
        let mut key = None;
        // The key name is the last part; it may itself be "+" written as "Plus".
        for part in code.split('+') {
            match part {
                "cmd" => s.cmd = true,
                "ctrl" => s.ctrl = true,
                "alt" => s.alt = true,
                "shift" => s.shift = true,
                name => key = egui::Key::from_name(name),
            }
        }
        s.key = key?;
        Some(s)
    }
}

#[derive(Clone)]
pub(crate) struct Keymap {
    bindings: HashMap<Action, Option<Shortcut>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: Action::ALL.iter().map(|&a| (a, a.default_shortcut())).collect(),
        }
    }
}

impl Keymap {
    pub(crate) fn get(&self, action: Action) -> Option<Shortcut> {
        self.bindings.get(&action).copied().flatten()
    }

    /// Binds `shortcut` to `action`. Any other action that had it loses it; that action is returned.
    pub(crate) fn set(&mut self, action: Action, shortcut: Option<Shortcut>) -> Option<Action> {
        let mut displaced = None;
        if let Some(sc) = shortcut {
            for (&other, bound) in self.bindings.iter_mut() {
                if other != action && *bound == Some(sc) {
                    *bound = None;
                    displaced = Some(other);
                }
            }
        }
        self.bindings.insert(action, shortcut);
        displaced
    }

    pub(crate) fn is_default(&self) -> bool {
        Action::ALL.iter().all(|&a| self.get(a) == a.default_shortcut())
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        for &a in &Action::ALL {
            let v = match self.get(a) {
                Some(sc) => json!(sc.to_code()),
                None => Value::Null,
            };
            map.insert(a.id().to_string(), v);
        }
        Value::Object(map)
    }

    /// Reads saved bindings; actions missing from `v` keep their defaults.
    pub(crate) fn from_json(v: &Value) -> Self {
        let mut keymap = Self::default();
        if let Some(obj) = v.as_object() {
            for &a in &Action::ALL {
                match obj.get(a.id()) {
                    Some(Value::Null) => {
                        keymap.bindings.insert(a, None);
                    }
                    Some(Value::String(code)) => {
                        if let Some(sc) = Shortcut::from_code(code) {
                            keymap.bindings.insert(a, Some(sc));
                        }
                    }
                    _ => {}
                }
            }
        }
        keymap
    }

    /// Finds a bound shortcut pressed this frame, consumes that key event, and returns its action.
    pub(crate) fn take_pressed(&self, ctx: &egui::Context) -> Option<Action> {
        ctx.input_mut(|i| {
            let mut found = None;
            i.events.retain(|event| {
                if found.is_some() {
                    return true;
                }
                let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
                    return true;
                };
                let pressed = Shortcut::from_event(*key, *modifiers);
                let action = Action::ALL.iter().copied().find(|&a| self.get(a) == Some(pressed));
                found = action;
                action.is_none()
            });
            found
        })
    }

    /// Makes the keymap available to widgets (for tooltips) this frame.
    pub(crate) fn publish(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("keymap"), self.clone()));
    }
}

/// Reads the next key press as a shortcut for the settings panel.
pub(crate) enum Recorded {
    Shortcut(Shortcut),
    Clear,
    Cancel,
}

pub(crate) fn record_key(ctx: &egui::Context) -> Option<Recorded> {
    ctx.input_mut(|i| {
        let mut result = None;
        i.events.retain(|event| {
            if result.is_some() {
                return true;
            }
            let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
                return true;
            };
            let plain = !modifiers.command && !modifiers.ctrl && !modifiers.alt && !modifiers.shift;
            result = match key {
                egui::Key::Escape if plain => Some(Recorded::Cancel),
                egui::Key::Backspace | egui::Key::Delete if plain => Some(Recorded::Clear),
                _ => {
                    let sc = Shortcut::from_event(*key, *modifiers);
                    sc.is_bindable().then_some(Recorded::Shortcut(sc))
                }
            };
            result.is_none()
        });
        result
    })
}

/// Tooltip text with the action's current shortcut, e.g. "Expand All (⌘+)".
pub(crate) fn tip_with_shortcut(ctx: &egui::Context, tip: &str, action: Action) -> String {
    let shortcut = ctx
        .data(|d| d.get_temp::<Keymap>(egui::Id::new("keymap")))
        .and_then(|k| k.get(action));
    match shortcut {
        // Multi-line tips get the shortcut on their first line.
        Some(sc) => match tip.split_once('\n') {
            Some((first, rest)) => format!("{} ({})\n{}", first, sc.label(), rest),
            None => format!("{} ({})", tip, sc.label()),
        },
        None => tip.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_keymap_round_trips() {
        let mut keymap = Keymap::default();
        keymap.set(Action::Clear, Some(Shortcut { cmd: true, shift: true, alt: false, ctrl: false, key: egui::Key::Backspace }));
        keymap.set(Action::NewTab, None);
        let restored = Keymap::from_json(&keymap.to_json());
        for &a in &Action::ALL {
            assert_eq!(restored.get(a), keymap.get(a), "{a:?}");
        }
    }

    #[test]
    fn rebinding_takes_the_shortcut_from_its_old_action() {
        let mut keymap = Keymap::default();
        let cmd_s = keymap.get(Action::ToggleSidebar);
        let displaced = keymap.set(Action::Search, cmd_s);
        assert_eq!(displaced, Some(Action::ToggleSidebar));
        assert_eq!(keymap.get(Action::ToggleSidebar), None);
    }

    #[test]
    fn equals_and_plus_are_the_same_key() {
        let a = Shortcut::from_event(egui::Key::Equals, egui::Modifiers::COMMAND);
        let b = Shortcut::from_event(egui::Key::Plus, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert_eq!(a, b);
        assert_eq!(Some(a), Action::ExpandAll.default_shortcut());
    }
}
