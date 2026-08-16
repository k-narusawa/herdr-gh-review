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
    /// deltaが色付けした生diffの各行。deltaが使えなければ `None`
    pub colored: Option<Vec<String>>,
    pub draft: Draft,
    pub collapsed: HashSet<usize>,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub scroll: usize,
    /// 左のファイルツリーのスクロール位置
    pub tree_scroll: usize,
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
            tree_scroll: 0,
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

    /// deltaが色付けした行。行番号カラムの右に、マーカーごとそのまま出す
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

    /// 現在のdiffに一致する行が無い下書きコメント。PRに新しいコミットが積まれると発生し、
    /// 画面には出ないまま提出リクエストには含まれる
    pub fn unmatched_comments(&self) -> usize {
        self.draft
            .comments
            .iter()
            .filter(|comment| !self.is_on_current_diff(comment))
            .count()
    }

    fn is_on_current_diff(&self, comment: &crate::review::DraftComment) -> bool {
        self.diff.files.iter().any(|file| {
            file.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
                file.comment_target(line).is_some_and(|target| {
                    target.path == comment.path
                        && target.line == comment.line
                        && target.side == comment.side
                })
            })
        })
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

    #[test]
    fn comment_on_a_current_line_is_matched() {
        let mut a = app();
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right },
            "見えているコメント".into(),
        );
        a.rebuild_rows();
        assert_eq!(a.unmatched_comments(), 0);
    }

    #[test]
    fn comment_on_a_vanished_line_is_reported_as_unmatched() {
        let mut a = app();
        a.draft.upsert_comment(
            CommentTarget { path: "a.rs".into(), line: 999, side: Side::Right },
            "PRが更新されて消えた行のコメント".into(),
        );
        a.rebuild_rows();
        assert_eq!(a.unmatched_comments(), 1);
        // 画面には出ない = これが罠の本体
        assert!(!a.rows.iter().any(|r| matches!(r, Row::Comment { .. })));
    }

    #[test]
    fn empty_diff_survives_every_cursor_operation() {
        let mut a = App::new(
            "",
            crate::review::Draft::new("o/r", 1, "sha"),
        );
        assert!(a.rows.is_empty());

        // どれも落ちないことがこのテストの主張
        a.move_cursor(1);
        a.move_cursor(-1);
        a.next_file();
        a.prev_file();
        a.toggle_collapse();
        a.rebuild_rows();

        assert_eq!(a.cursor, 0);
        assert_eq!(a.cursor_file_idx(), None);
        assert_eq!(a.cursor_target(), None);
        assert!(a.cursor_line().is_none());
    }
}
