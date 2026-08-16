use crate::diff::{CommentTarget, DiffLine, FileDiff, LineKind, ParsedDiff, Side};
use crate::review::{Draft, DraftComment};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    FileHeader {
        file_idx: usize,
    },
    HunkHeader {
        file_idx: usize,
        hunk_idx: usize,
    },
    Line {
        file_idx: usize,
        hunk_idx: usize,
        line_idx: usize,
    },
    /// One row of split view. Each cell holds an index into the hunk, `None` if that side is empty
    Pair {
        file_idx: usize,
        hunk_idx: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    Comment {
        path: String,
        line: u32,
        side: Side,
        body_line: String,
    },
}

pub struct App {
    pub diff: ParsedDiff,
    /// The raw diff colored by delta, line for line. `None` when delta is unavailable
    pub colored: Option<Vec<String>>,
    pub draft: Draft,
    pub collapsed: HashSet<usize>,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub scroll: usize,
    pub show_tree: bool,
    /// Scroll position of the file tree on the left
    pub tree_scroll: usize,
    pub split: bool,
    /// The cell the cursor prefers in split view
    pub cursor_side: Side,
    pub status: Option<String>,
}

impl App {
    pub fn new(raw_diff: &str, draft: Draft) -> Self {
        let mut app = Self {
            diff: ParsedDiff { files: Vec::new() },
            colored: None,
            draft,
            collapsed: HashSet::new(),
            rows: Vec::new(),
            cursor: 0,
            scroll: 0,
            show_tree: true,
            tree_scroll: 0,
            split: false,
            cursor_side: Side::Right,
            status: None,
        };
        app.set_diff(raw_diff);
        app
    }

    pub fn set_diff(&mut self, raw_diff: &str) {
        self.diff = crate::diff::parse(raw_diff);
        self.colored = crate::delta::colorize(raw_diff);
        self.rebuild_rows();
    }

    /// A delta-colored line, printed as-is with its marker, right of the line-number column
    pub fn colored_line(&self, raw_idx: usize) -> Option<&str> {
        Some(self.colored.as_ref()?.get(raw_idx)?.as_str())
    }

    pub fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        for (file_idx, file) in self.diff.files.iter().enumerate() {
            rows.push(Row::FileHeader { file_idx });
            if self.collapsed.contains(&file_idx) {
                continue;
            }
            for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
                rows.push(Row::HunkHeader { file_idx, hunk_idx });
                if self.split {
                    for (left, right) in pair_lines(&hunk.lines) {
                        rows.push(Row::Pair { file_idx, hunk_idx, left, right });
                        // A context line is the same line on both sides — show its comment once
                        let cells = [left, if left == right { None } else { right }];
                        for line_idx in cells.into_iter().flatten() {
                            self.push_comments(&mut rows, file, &hunk.lines[line_idx]);
                        }
                    }
                } else {
                    for (line_idx, line) in hunk.lines.iter().enumerate() {
                        rows.push(Row::Line { file_idx, hunk_idx, line_idx });
                        self.push_comments(&mut rows, file, line);
                    }
                }
            }
        }
        self.rows = rows;
        if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len().saturating_sub(1);
        }
    }

    fn push_comments(&self, rows: &mut Vec<Row>, file: &FileDiff, line: &DiffLine) {
        let Some(target) = file.comment_target(line) else {
            return;
        };
        let Some(comment) = self.draft.comment_at(&target) else {
            return;
        };
        for (i, body_line) in comment.body.lines().enumerate() {
            let body_line = if comment.ai && i == 0 {
                format!("[AI] {body_line}")
            } else {
                body_line.to_string()
            };
            rows.push(Row::Comment {
                path: target.path.clone(),
                line: target.line,
                side: target.side,
                body_line,
            });
        }
    }

    pub fn toggle_split(&mut self) {
        let file_idx = self.cursor_file_idx();
        self.split = !self.split;
        self.rebuild_rows();
        self.focus_file(file_idx);
    }

    fn focus_file(&mut self, file_idx: Option<usize>) {
        let Some(file_idx) = file_idx else { return };
        if let Some(pos) = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::FileHeader { file_idx: i } if *i == file_idx))
        {
            self.cursor = pos;
        }
    }

    /// The cell the cursor really lands on in a Pair row, falling to the other side if empty
    pub fn active_side(&self, left: Option<usize>, right: Option<usize>) -> Side {
        let (preferred, other) = match self.cursor_side {
            Side::Left => (left, right),
            Side::Right => (right, left),
        };
        if preferred.is_none() && other.is_some() {
            self.cursor_side.flip()
        } else {
            self.cursor_side
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    pub fn cursor_file_idx(&self) -> Option<usize> {
        match self.rows.get(self.cursor)? {
            Row::FileHeader { file_idx }
            | Row::HunkHeader { file_idx, .. }
            | Row::Line { file_idx, .. }
            | Row::Pair { file_idx, .. } => Some(*file_idx),
            Row::Comment { .. } => None,
        }
    }

    pub fn next_file(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let found = self.rows[self.cursor + 1..]
            .iter()
            .position(|r| matches!(r, Row::FileHeader { .. }));
        if let Some(offset) = found {
            self.cursor += offset + 1;
        }
    }

    pub fn prev_file(&mut self) {
        let found = self.rows[..self.cursor]
            .iter()
            .rposition(|r| matches!(r, Row::FileHeader { .. }));
        if let Some(idx) = found {
            self.cursor = idx;
        }
    }

    pub fn toggle_collapse(&mut self) {
        let Some(file_idx) = self.cursor_file_idx() else {
            return;
        };
        if !self.collapsed.remove(&file_idx) {
            self.collapsed.insert(file_idx);
        }
        self.rebuild_rows();
        self.focus_file(Some(file_idx));
    }

    pub fn cursor_line(&self) -> Option<(&FileDiff, &DiffLine)> {
        let (file_idx, hunk_idx, line_idx) = match self.rows.get(self.cursor)? {
            Row::Line { file_idx, hunk_idx, line_idx } => (*file_idx, *hunk_idx, *line_idx),
            Row::Pair { file_idx, hunk_idx, left, right } => {
                let line_idx = match self.active_side(*left, *right) {
                    Side::Left => *left,
                    Side::Right => *right,
                }?;
                (*file_idx, *hunk_idx, line_idx)
            }
            _ => return None,
        };
        let file = self.diff.files.get(file_idx)?;
        Some((file, file.hunks.get(hunk_idx)?.lines.get(line_idx)?))
    }

    pub fn cursor_target(&self) -> Option<CommentTarget> {
        let (file, line) = self.cursor_line()?;
        file.comment_target(line)
    }

    /// Draft comments whose line is gone from the current diff, as new commits on the PR do
    pub fn stale_comments(&self) -> usize {
        self.draft
            .comments
            .iter()
            .filter(|c| !is_on_diff(&self.diff, c))
            .count()
    }

    /// The draft as GitHub gets it. One line outside the diff would 422 the whole review
    pub fn submittable_draft(&self) -> Draft {
        let mut draft = self.draft.clone();
        draft.comments.retain(|c| is_on_diff(&self.diff, c));
        draft
    }

    /// Drop the comments that were submitted, returning how many were left behind
    pub fn retain_stale_comments(&mut self) -> usize {
        let diff = &self.diff;
        self.draft.comments.retain(|c| !is_on_diff(diff, c));
        self.draft.comments.len()
    }

    /// Throw away the comments that are no longer in the diff, returning how many went
    pub fn discard_stale_comments(&mut self) -> usize {
        let before = self.draft.comments.len();
        let diff = &self.diff;
        self.draft.comments.retain(|c| is_on_diff(diff, c));
        before - self.draft.comments.len()
    }
}

fn is_on_diff(diff: &crate::diff::ParsedDiff, comment: &DraftComment) -> bool {
    diff.files.iter().any(|file| {
        file.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
            file.comment_target(line).is_some_and(|target| {
                target.path == comment.path
                    && target.line == comment.line
                    && target.side == comment.side
            })
        })
    })
}

/// Pair removed lines with added ones. Whatever does not pair up gets an empty cell (`None`).
/// Context lines always break the run, so pairing stays inside a single block of change
fn pair_lines(lines: &[DiffLine]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();

    fn flush(
        out: &mut Vec<(Option<usize>, Option<usize>)>,
        removed: &mut Vec<usize>,
        added: &mut Vec<usize>,
    ) {
        for i in 0..removed.len().max(added.len()) {
            out.push((removed.get(i).copied(), added.get(i).copied()));
        }
        removed.clear();
        added.clear();
    }

    for (i, line) in lines.iter().enumerate() {
        match line.kind {
            LineKind::Removed => removed.push(i),
            LineKind::Added => added.push(i),
            LineKind::Context => {
                flush(&mut out, &mut removed, &mut added);
                out.push((Some(i), Some(i)));
            }
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::Side;

    const TWO_FILES: &str = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-one
+ONE
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1,1 +1,2 @@
 keep
+added
";

    fn app() -> App {
        App::new(
            TWO_FILES,
            crate::review::Draft::new("o/r", 1, "sha"),
        )
    }

    #[test]
    fn flattens_files_hunks_and_lines_in_order() {
        let a = app();
        assert!(matches!(a.rows[0], Row::FileHeader { file_idx: 0 }));
        assert!(matches!(a.rows[1], Row::HunkHeader { file_idx: 0, hunk_idx: 0 }));
        assert!(matches!(a.rows[2], Row::Line { file_idx: 0, .. }));
        assert!(matches!(a.rows[4], Row::FileHeader { file_idx: 1 }));
    }

    #[test]
    fn collapsed_file_hides_its_hunks_and_lines() {
        let mut a = app();
        a.collapsed.insert(0);
        a.rebuild_rows();
        assert!(matches!(a.rows[0], Row::FileHeader { file_idx: 0 }));
        assert!(matches!(a.rows[1], Row::FileHeader { file_idx: 1 }));
    }

    #[test]
    fn draft_comment_renders_below_its_line() {
        let mut a = app();
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right },
            "first line\nsecond line".into(),
            false,
        );
        a.rebuild_rows();
        // rows[2]="-one", rows[3]="+ONE" (a.rs:1 RIGHT), then the two comment lines
        let Row::Comment { ref body_line, .. } = a.rows[4] else {
            panic!("expected a comment row, got {:?}", a.rows[4]);
        };
        assert_eq!(body_line, "first line");
        let Row::Comment { ref body_line, .. } = a.rows[5] else {
            panic!("expected a comment row");
        };
        assert_eq!(body_line, "second line");
    }

    #[test]
    fn cursor_stays_within_bounds() {
        let mut a = app();
        a.move_cursor(-5);
        assert_eq!(a.cursor, 0);
        a.move_cursor(1000);
        assert_eq!(a.cursor, a.rows.len() - 1);
    }

    #[test]
    fn next_file_jumps_to_the_following_file_header() {
        let mut a = app();
        a.next_file();
        assert!(matches!(a.rows[a.cursor], Row::FileHeader { file_idx: 1 }));
        a.next_file();
        assert!(matches!(a.rows[a.cursor], Row::FileHeader { file_idx: 1 }));
        a.prev_file();
        assert!(matches!(a.rows[a.cursor], Row::FileHeader { file_idx: 0 }));
    }

    #[test]
    fn cursor_target_resolves_only_on_line_rows() {
        let mut a = app();
        assert_eq!(a.cursor_target(), None); // FileHeader
        a.cursor = 3; // "+ONE"
        let t = a.cursor_target().unwrap();
        assert_eq!(t.path, "a.rs");
        assert_eq!(t.line, 1);
        assert_eq!(t.side, Side::Right);
    }

    /// One live comment, and one on a line a PR update took away
    fn app_with_a_stale_comment() -> App {
        let mut a = app();
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right },
            "on a line that is still here".into(),
            false,
        );
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 999, side: Side::Right },
            "on a line the PR update removed".into(),
            false,
        );
        a.rebuild_rows();
        a
    }

    #[test]
    fn comment_on_a_current_line_is_matched() {
        let mut a = app();
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right },
            "a visible comment".into(),
            false,
        );
        a.rebuild_rows();
        assert_eq!(a.stale_comments(), 0);
    }

    #[test]
    fn comment_on_a_vanished_line_is_reported_as_stale() {
        let a = app_with_a_stale_comment();
        assert_eq!(a.stale_comments(), 1);
        // A comment on a vanished line never renders — being submitted unseen is the trap
        assert_eq!(a.rows.iter().filter(|r| matches!(r, Row::Comment { .. })).count(), 1);
    }

    /// This is what keeps the 422 away: only lines that are in the diff go out
    #[test]
    fn submittable_draft_leaves_stale_comments_behind() {
        let a = app_with_a_stale_comment();
        let sent = a.submittable_draft();
        assert_eq!(sent.comments.len(), 1);
        assert_eq!(sent.comments[0].line, 1);
        // the draft itself is untouched
        assert_eq!(a.draft.comments.len(), 2);
    }

    #[test]
    fn retain_stale_comments_drops_only_what_was_submitted() {
        let mut a = app_with_a_stale_comment();
        assert_eq!(a.retain_stale_comments(), 1);
        assert_eq!(a.draft.comments[0].line, 999);
    }

    #[test]
    fn discard_stale_comments_drops_only_the_stale_ones() {
        let mut a = app_with_a_stale_comment();
        assert_eq!(a.discard_stale_comments(), 1);
        assert_eq!(a.draft.comments.len(), 1);
        assert_eq!(a.draft.comments[0].line, 1);
        assert_eq!(a.stale_comments(), 0);
    }

    const UNEVEN: &str = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,4 +1,3 @@
 keep
-one
-two
+ONE
 tail
";

    fn pairs(diff: &str) -> Vec<(Option<usize>, Option<usize>)> {
        pair_lines(&crate::diff::parse(diff).files[0].hunks[0].lines)
    }

    #[test]
    fn context_lines_sit_on_both_sides() {
        let p = pairs(UNEVEN);
        assert_eq!(p[0], (Some(0), Some(0)));
        assert_eq!(p.last(), Some(&(Some(4), Some(4))));
    }

    #[test]
    fn extra_removed_line_gets_an_empty_right_cell() {
        // -one/-two and +ONE fold into 2 rows, not 3; the leftover removed line has an empty right
        let p = pairs(UNEVEN);
        assert_eq!(p[1], (Some(1), Some(3)));
        assert_eq!(p[2], (Some(2), None));
        assert_eq!(p.len(), 4);
    }

    #[test]
    fn split_rows_replace_line_rows() {
        let mut a = app();
        a.toggle_split();
        assert!(a.rows.iter().all(|r| !matches!(r, Row::Line { .. })));
        assert!(a.rows.iter().any(|r| matches!(r, Row::Pair { .. })));
        // toggling lands back on the file you were in
        assert!(matches!(a.rows[a.cursor], Row::FileHeader { file_idx: 0 }));
    }

    #[test]
    fn cursor_target_follows_the_active_side() {
        let mut a = App::new(UNEVEN, crate::review::Draft::new("o/r", 1, "sha"));
        a.toggle_split();
        // rows: FileHeader, HunkHeader, (keep,keep), (-one,+ONE), (-two,None), (tail,tail)
        a.cursor = 3;
        assert_eq!(a.cursor_target().unwrap().side, Side::Right);
        assert_eq!(a.cursor_target().unwrap().line, 2); // +ONE is new:2

        a.cursor_side = Side::Left;
        assert_eq!(a.cursor_target().unwrap().side, Side::Left);
        assert_eq!(a.cursor_target().unwrap().line, 2); // -one is old:2
    }

    #[test]
    fn cursor_falls_back_when_the_preferred_cell_is_empty() {
        let mut a = App::new(UNEVEN, crate::review::Draft::new("o/r", 1, "sha"));
        a.toggle_split();
        a.cursor = 4; // (-two, none)
        assert_eq!(a.cursor_target().unwrap().side, Side::Left);
        assert_eq!(a.cursor_target().unwrap().line, 3);
    }

    #[test]
    fn split_shows_a_comment_once_per_context_line() {
        let mut a = App::new(UNEVEN, crate::review::Draft::new("o/r", 1, "sha"));
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right },
            "a comment on the keep line".into(),
            false,
        );
        a.toggle_split();
        assert_eq!(
            a.rows.iter().filter(|r| matches!(r, Row::Comment { .. })).count(),
            1
        );
    }

    #[test]
    fn empty_diff_survives_every_cursor_operation() {
        let mut a = App::new(
            "",
            crate::review::Draft::new("o/r", 1, "sha"),
        );
        assert!(a.rows.is_empty());

        // the claim of this test is that none of these panic
        a.move_cursor(1);
        a.move_cursor(-1);
        a.next_file();
        a.prev_file();
        a.toggle_collapse();
        a.toggle_split();
        a.rebuild_rows();

        assert_eq!(a.cursor, 0);
        assert_eq!(a.cursor_file_idx(), None);
        assert_eq!(a.cursor_target(), None);
        assert!(a.cursor_line().is_none());
    }
}
