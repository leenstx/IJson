use similar::{ChangeTag, TextDiff};

use crate::tab::JsonTab;

/// The containing tab is the left document; the right document owns all its editor state.
pub(crate) struct Comparison {
    pub(crate) right: JsonTab,
    pub(crate) right_active: bool,
    pub(crate) removed: Vec<usize>,
    pub(crate) added: Vec<usize>,
    /// Mouse-wheel Cmd+scroll not yet applied, eased in over a few frames.
    pub(crate) pending_sync_scroll: f32,
    revisions: Option<(u64, u64)>,
}

impl Comparison {
    pub(crate) fn new(right: JsonTab) -> Self {
        Self {
            right,
            right_active: true,
            removed: Vec::new(),
            added: Vec::new(),
            pending_sync_scroll: 0.0,
            revisions: None,
        }
    }

    pub(crate) fn refresh(&mut self, left: &str, revision: u64) {
        let revisions = (revision, self.right.revision);
        if self.revisions == Some(revisions) {
            return;
        }
        self.revisions = Some(revisions);
        self.removed.clear();
        self.added.clear();
        if self.right.input.trim().is_empty() || left == self.right.input {
            return;
        }

        // Bound expensive diffs while typing. On timeout similar keeps an approximate diff.
        let diff = TextDiff::configure()
            .timeout(std::time::Duration::from_millis(25))
            .diff_lines(left, self.right.input.as_str());
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => {
                    if let Some(line) = change.old_index() {
                        self.removed.push(line);
                    }
                }
                ChangeTag::Insert => {
                    if let Some(line) = change.new_index() {
                        self.added.push(line);
                    }
                }
                ChangeTag::Equal => {}
            }
        }
    }
}
