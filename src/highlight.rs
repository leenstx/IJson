use eframe::egui;

use crate::theme::ThemeColors;

pub(crate) fn highlight_json(
    text: &str,
    font_id: egui::FontId,
    line_height: f32,
    colors: ThemeColors,
) -> egui::text::LayoutJob {
    if text.is_empty() {
        return egui::text::LayoutJob {
            sections: Vec::new(),
            text: String::new(),
            wrap: egui::text::TextWrapping {
                max_width: f32::INFINITY,
                ..Default::default()
            },
            break_on_newline: true,
            ..Default::default()
        };
    }

    let mut sections = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let punctuation_color = colors.punctuation;
    let default_color = colors.text;

    let add_sec = |sections: &mut Vec<egui::text::LayoutSection>,
                   start: usize,
                   end: usize,
                   color: egui::Color32| {
        if start < end {
            sections.push(egui::text::LayoutSection {
                leading_space: 0.0,
                byte_range: start..end,
                format: egui::text::TextFormat {
                    font_id: font_id.clone(),
                    color,
                    line_height: Some(line_height),
                    extra_letter_spacing: 0.15,
                    ..Default::default()
                },
            });
        }
    };

    while i < len {
        let b = bytes[i];

        // 1. Whitespace (spaces, tabs, newlines)
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            let start = i;
            while i < len
                && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\r' || bytes[i] == b'\n')
            {
                i += 1;
            }
            add_sec(&mut sections, start, i, default_color);
            continue;
        }

        // 2. String literal: "..."
        if b == b'"' {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < len {
                let cb = bytes[i];
                if escaped {
                    escaped = false;
                } else if cb == b'\\' {
                    escaped = true;
                } else if cb == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }

            // Peek ahead past whitespace to check if followed by ':'
            let mut peek = i;
            while peek < len
                && (bytes[peek] == b' '
                    || bytes[peek] == b'\t'
                    || bytes[peek] == b'\r'
                    || bytes[peek] == b'\n')
            {
                peek += 1;
            }
            let is_key = peek < len && bytes[peek] == b':';
            let color = if is_key { colors.key } else { colors.string };
            add_sec(&mut sections, start, i, color);
            continue;
        }

        // 3. Numbers: starts with digit or '-'
        if b.is_ascii_digit() || b == b'-' {
            let start = i;
            i += 1;
            while i < len {
                let cb = bytes[i];
                if cb.is_ascii_digit()
                    || cb == b'.'
                    || cb == b'e'
                    || cb == b'E'
                    || cb == b'+'
                    || cb == b'-'
                {
                    i += 1;
                } else {
                    break;
                }
            }
            add_sec(&mut sections, start, i, colors.number);
            continue;
        }

        // 4. Identifiers: true, false, null
        if b.is_ascii_alphabetic() {
            let start = i;
            while i < len && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &text[start..i];
            let color = match word {
                "true" | "false" => colors.boolean,
                "null" => colors.null,
                _ => default_color,
            };
            add_sec(&mut sections, start, i, color);
            continue;
        }

        // 5. Colon ':'
        if b == b':' {
            add_sec(&mut sections, i, i + 1, colors.colon);
            i += 1;
            continue;
        }

        // Folded block chip: [...] or {...}
        if (b == b'[' || b == b'{')
            && (text[i..].starts_with("[...]") || text[i..].starts_with("{...}"))
        {
            add_sec(
                &mut sections,
                i,
                i + 5,
                colors.fold_chip_fg,
            );
            if let Some(section) = sections.last_mut() {
                section.format.background = colors.fold_chip_bg;
            }
            i += 5;
            continue;
        }

        // 6. Brackets & commas: '{', '}', '[', ']', ','
        if b == b'{' || b == b'}' || b == b'[' || b == b']' || b == b',' {
            add_sec(&mut sections, i, i + 1, punctuation_color);
            i += 1;
            continue;
        }

        // 7. Any other UTF-8 character
        let start = i;
        if let Some(ch) = text[start..].chars().next() {
            i += ch.len_utf8();
        } else {
            i += 1;
        }
        add_sec(&mut sections, start, i, default_color);
    }

    egui::text::LayoutJob {
        sections,
        text: text.to_string(),
        wrap: egui::text::TextWrapping {
            max_width: f32::INFINITY,
            ..Default::default()
        },
        break_on_newline: true,
        ..Default::default()
    }
}
