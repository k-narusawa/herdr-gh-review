use crate::diff::{CommentTarget, DiffLine, ParsedDiff, Side};
use crate::review::Draft;
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
    Comment {
        path: String,
        line: u32,
        side: Side,
        body_line: String,
    },
}

pub struct App {
    pub diff: ParsedDiff,
    pub draft: Draft,
    pub collapsed: HashSet<usize>,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub scroll: usize,
    pub status: Option<String>,
}

impl App {
    pub fn new(diff: ParsedDiff, draft: Draft) -> Self {
        let mut app = Self {
            diff,
            draft,
            collapsed: HashSet::new(),
            rows: Vec::new(),
            cursor: 0,
            scroll: 0,
            status: None,
        };
        app.rebuild_rows();
        app
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
                for (line_idx, line) in hunk.lines.iter().enumerate() {
                    rows.push(Row::Line { file_idx, hunk_idx, line_idx });
                    let Some(target) = file.comment_target(line) else {
                        continue;
                    };
                    let Some(comment) = self.draft.comment_at(&target) else {
                        continue;
                    };
                    for body_line in comment.body.lines() {
                        rows.push(Row::Comment {
                            path: target.path.clone(),
                            line: target.line,
                            side: target.side,
                            body_line: body_line.to_string(),
                        });
                    }
                }
            }
        }
        self.rows = rows;
        if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len().saturating_sub(1);
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
            | Row::Line { file_idx, .. } => Some(*file_idx),
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
        if let Some(pos) = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::FileHeader { file_idx: i } if *i == file_idx))
        {
            self.cursor = pos;
        }
    }

    pub fn cursor_line(&self) -> Option<(&crate::diff::FileDiff, &DiffLine)> {
        let Row::Line { file_idx, hunk_idx, line_idx } = self.rows.get(self.cursor)? else {
            return None;
        };
        let file = self.diff.files.get(*file_idx)?;
        Some((file, file.hunks.get(*hunk_idx)?.lines.get(*line_idx)?))
    }

    pub fn cursor_target(&self) -> Option<CommentTarget> {
        let (file, line) = self.cursor_line()?;
        file.comment_target(line)
    }
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
            crate::diff::parse(TWO_FILES),
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
            "一行目\n二行目".into(),
        );
        a.rebuild_rows();
        // rows[2]="-one" rows[3]="+ONE"(a.rs:1 RIGHT) の直後にコメント2行
        let Row::Comment { ref body_line, .. } = a.rows[4] else {
            panic!("expected a comment row, got {:?}", a.rows[4]);
        };
        assert_eq!(body_line, "一行目");
        let Row::Comment { ref body_line, .. } = a.rows[5] else {
            panic!("expected a comment row");
        };
        assert_eq!(body_line, "二行目");
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
}
