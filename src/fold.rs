use eframe::egui::{self, Color32, Pos2, Stroke};
use std::collections::HashSet;

/// Multi-line `{...}` / `[...]` range in the editor source.
#[derive(Clone, Copy)]
pub(crate) struct FoldSpan {
    pub start_line: usize,
    pub end_line: usize,
    open: char,
    open_col: usize,
    close_col: usize,
}

/// One visible editor row and the source lines it stands for.
#[derive(Clone, Copy)]
pub(crate) struct ViewLine {
    pub source_start: usize,
    pub source_end: usize,
    synthetic: bool,
    open: char,
    open_col: usize,
}

pub(crate) enum ViewCommit {
    Unchanged,
    Applied {
        source: String,
        src_lo: usize,
        src_hi: usize,
        new_line_count: usize,
        /// Collapsed blocks whose display was replaced by their real source lines.
        expand: Vec<usize>,
    },
    /// The edit would write a fold placeholder into the document. Keep the source.
    Restore,
}

const PLACEHOLDER: &str = "...";

pub(crate) fn compute_folds(text: &str) -> Vec<FoldSpan> {
    let mut spans = Vec::new();
    let mut stack: Vec<(usize, char, usize)> = Vec::new();
    let mut line = 0usize;
    let mut col = 0usize;
    let mut in_string = false;
    let mut escape = false;

    for c in text.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += c.len_utf8();
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                col += 1;
            }
            '{' | '[' => {
                stack.push((line, c, col));
                col += 1;
            }
            '}' | ']' => {
                let close_col = col;
                col += 1;
                if let Some((start_line, open, open_col)) = stack.pop() {
                    let matched = (open == '{' && c == '}') || (open == '[' && c == ']');
                    if matched && line > start_line {
                        spans.push(FoldSpan {
                            start_line,
                            end_line: line,
                            open,
                            open_col,
                            close_col,
                        });
                    }
                }
            }
            '\n' => {
                line += 1;
                col = 0;
            }
            _ => col += c.len_utf8(),
        }
    }
    spans
}

pub(crate) fn build_view(
    source: &str,
    collapsed: &HashSet<usize>,
) -> (String, Vec<ViewLine>, Vec<FoldSpan>) {
    let spans = compute_folds(source);
    let lines = split_lines(source);
    let mut out_lines = Vec::new();
    let mut view = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(span) = outermost_collapsed(&spans, collapsed, i) {
            out_lines.push(synthetic_line(&lines, span));
            view.push(ViewLine {
                source_start: span.start_line,
                source_end: span.end_line,
                synthetic: true,
                open: span.open,
                open_col: span.open_col,
            });
            i = span.end_line + 1;
            continue;
        }
        out_lines.push(lines[i].clone());
        view.push(ViewLine {
            source_start: i,
            source_end: i,
            synthetic: false,
            open: '{',
            open_col: 0,
        });
        i += 1;
    }
    if view.is_empty() {
        view.push(ViewLine {
            source_start: 0,
            source_end: 0,
            synthetic: false,
            open: '{',
            open_col: 0,
        });
        out_lines.push(String::new());
    }
    (join_lines(&out_lines), view, spans)
}

pub(crate) fn apply_view_edit(
    source: &str,
    map: &[ViewLine],
    old_view: &str,
    new_view: &str,
) -> ViewCommit {
    if old_view == new_view {
        return ViewCommit::Unchanged;
    }
    let old_lines = split_lines(old_view);
    let new_lines = split_lines(new_view);
    let src_lines = split_lines(source);
    if map.len() != old_lines.len() {
        return ViewCommit::Restore;
    }

    let mut pre = 0;
    while pre < old_lines.len() && pre < new_lines.len() && old_lines[pre] == new_lines[pre] {
        pre += 1;
    }
    let mut old_suf = old_lines.len();
    let mut new_suf = new_lines.len();
    while old_suf > pre && new_suf > pre && old_lines[old_suf - 1] == new_lines[new_suf - 1] {
        old_suf -= 1;
        new_suf -= 1;
    }

    let (src_lo, src_hi) = if pre == old_suf {
        let at = if pre == 0 {
            0
        } else {
            map[pre - 1].source_end + 1
        };
        (at, at)
    } else if pre < map.len() && old_suf > 0 && old_suf <= map.len() {
        (map[pre].source_start, map[old_suf - 1].source_end + 1)
    } else {
        return ViewCommit::Restore;
    };
    if src_lo > src_hi || src_hi > src_lines.len() {
        return ViewCommit::Restore;
    }

    let region_synthetic = pre < old_suf && map[pre..old_suf].iter().any(|line| line.synthetic);
    let mut replacement = new_lines[pre..new_suf].to_vec();
    let mut expand = Vec::new();

    if region_synthetic && replacement == src_lines[src_lo..src_hi] {
        // Undo, or a paste of the hidden text, is an expand — not a rewrite.
        for line in &map[pre..old_suf] {
            if line.synthetic {
                expand.push(line.source_start);
            }
        }
        return ViewCommit::Applied {
            source: source.to_string(),
            src_lo,
            src_hi,
            new_line_count: src_hi - src_lo,
            expand,
        };
    }

    if region_synthetic && old_suf == pre + 1 && new_suf == pre + 1 {
        // Keep the hidden lines. Only the text around the [...] / {...} chip is editable.
        match update_folded_boundaries(&src_lines, &map[pre], &new_lines[pre]) {
            Some(updated) => replacement = updated,
            None => return ViewCommit::Restore,
        }
    } else if replacement.iter().any(|line| line_has_marker(line)) {
        return ViewCommit::Restore;
    }

    let mut out = Vec::with_capacity(src_lines.len() - (src_hi - src_lo) + replacement.len());
    out.extend_from_slice(&src_lines[..src_lo]);
    out.extend(replacement.iter().cloned());
    out.extend_from_slice(&src_lines[src_hi..]);
    ViewCommit::Applied {
        source: join_lines(&out),
        src_lo,
        src_hi,
        new_line_count: replacement.len(),
        expand,
    }
}

/// When a selection covers a folded block, copy the hidden source instead of `[...]`.
pub(crate) fn folded_selection_source(
    source: &str,
    map: &[ViewLine],
    view: &str,
    start_char: usize,
    end_char: usize,
) -> Option<String> {
    if start_char >= end_char || map.is_empty() {
        return None;
    }
    let start_byte = char_to_byte(view, start_char);
    let end_byte = char_to_byte(view, end_char);
    let lines = split_lines(view);
    let mut offset = 0usize;
    let mut first = None;
    let mut last = None;
    for (i, line) in lines.iter().enumerate() {
        let line_end = offset + line.len();
        let next = if i + 1 == lines.len() {
            line_end
        } else {
            line_end + 1
        };
        if end_byte > offset && start_byte < next {
            first.get_or_insert(i);
            last = Some(i);
        }
        offset = next;
    }
    let first = first?;
    let last = last?;
    if last >= map.len() || !map[first..=last].iter().any(|line| line.synthetic) {
        return None;
    }
    let src_lines = split_lines(source);
    if src_lines.is_empty() {
        return None;
    }
    let src_lo = map[first].source_start.min(src_lines.len() - 1);
    let src_hi = map[last].source_end.min(src_lines.len() - 1);
    if src_lo > src_hi {
        return None;
    }
    Some(src_lines[src_lo..=src_hi].join("\n"))
}

fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

pub(crate) fn shift_folds(
    folds: &mut HashSet<usize>,
    src_lo: usize,
    src_hi: usize,
    new_line_count: usize,
    expand: &[usize],
) {
    let old_count = src_hi.saturating_sub(src_lo);
    let delta = new_line_count as isize - old_count as isize;
    let mut next = HashSet::new();
    for start in folds.drain() {
        if expand.contains(&start) || (start > src_lo && start < src_hi) {
            continue;
        }
        if start == src_lo && new_line_count != old_count {
            continue;
        }
        if start >= src_hi {
            let shifted = start as isize + delta;
            if shifted >= 0 {
                next.insert(shifted as usize);
            }
        } else {
            next.insert(start);
        }
    }
    *folds = next;
}

pub(crate) fn draw_fold_chevron(
    painter: &egui::Painter,
    center: Pos2,
    collapsed: bool,
    color: Color32,
) {
    let points = if collapsed {
        vec![
            center + egui::vec2(-2.0, -3.4),
            center + egui::vec2(-2.0, 3.4),
            center + egui::vec2(3.2, 0.0),
        ]
    } else {
        vec![
            center + egui::vec2(-3.4, -2.0),
            center + egui::vec2(3.4, -2.0),
            center + egui::vec2(0.0, 3.2),
        ]
    };
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn outermost_collapsed<'a>(
    spans: &'a [FoldSpan],
    collapsed: &HashSet<usize>,
    line: usize,
) -> Option<&'a FoldSpan> {
    spans
        .iter()
        .filter(|span| span.start_line == line && collapsed.contains(&span.start_line))
        .max_by_key(|span| span.end_line)
}

fn synthetic_line(lines: &[String], span: &FoldSpan) -> String {
    let start = lines.get(span.start_line).map(String::as_str).unwrap_or("");
    let end = lines.get(span.end_line).map(String::as_str).unwrap_or("");
    let prefix = &start[..span.open_col.min(start.len())];
    let close = if span.open == '{' { '}' } else { ']' };
    let suffix_at = (span.close_col + close.len_utf8()).min(end.len());
    let suffix = &end[suffix_at..];
    format!("{prefix}{}{PLACEHOLDER}{close}{suffix}", span.open)
}

fn update_folded_boundaries(
    src_lines: &[String],
    view: &ViewLine,
    edited: &str,
) -> Option<Vec<String>> {
    if !view.synthetic || view.source_end >= src_lines.len() {
        return None;
    }
    let start = &src_lines[view.source_start];
    let end = &src_lines[view.source_end];
    let open = view.open;
    let (prefix, suffix) = split_marker(edited, open)?;
    let open_col = view.open_col.min(start.len());
    if start.as_bytes().get(open_col) != Some(&(open as u8)) {
        return None;
    }
    let rest = &start[open_col + 1..];
    let close = if open == '{' { '}' } else { ']' };
    let close_col = end.rfind(close)?;
    let mut updated = src_lines[view.source_start..=view.source_end].to_vec();
    updated[0] = format!("{prefix}{open}{rest}");
    let last = updated.len() - 1;
    updated[last] = format!("{}{close}{suffix}", &end[..close_col]);
    Some(updated)
}

fn split_marker(line: &str, open: char) -> Option<(String, String)> {
    let token = if open == '[' { "[...]" } else { "{...}" };
    let at = find_unquoted(line, token)?;
    Some((line[..at].to_string(), line[at + token.len()..].to_string()))
}

fn line_has_marker(line: &str) -> bool {
    find_unquoted(line, "[...]").is_some() || find_unquoted(line, "{...}").is_some()
}

fn find_unquoted(line: &str, token: &str) -> Option<usize> {
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    let bytes = line.as_bytes();
    while i < bytes.len() {
        if in_string {
            if escape {
                escape = false;
            } else if bytes[i] == b'\\' {
                escape = true;
            } else if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if line[i..].starts_with(token) {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub(crate) fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn join_lines(lines: &[String]) -> String {
    if lines.len() == 1 && lines[0].is_empty() {
        return String::new();
    }
    lines.join("\n")
}
