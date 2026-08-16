use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Right,
    Left,
}

impl Side {
    pub fn flip(self) -> Self {
        match self {
            Side::Right => Side::Left,
            Side::Left => Side::Right,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub text: String,
    /// Index into the raw diff, kept only to line up with delta's colored output
    pub raw_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    pub fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("(unknown)")
    }

    pub fn additions(&self) -> usize {
        self.count(LineKind::Added)
    }

    pub fn deletions(&self) -> usize {
        self.count(LineKind::Removed)
    }

    fn count(&self, kind: LineKind) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == kind)
            .count()
    }

    pub fn is_rename(&self) -> bool {
        match (&self.old_path, &self.new_path) {
            (Some(o), Some(n)) => o != n,
            _ => false,
        }
    }

    pub fn comment_target(&self, line: &DiffLine) -> Option<CommentTarget> {
        if self.is_binary {
            return None;
        }
        match line.kind {
            LineKind::Added | LineKind::Context => Some(CommentTarget {
                path: self.new_path.clone()?,
                line: line.new_lineno?,
                side: Side::Right,
            }),
            LineKind::Removed => Some(CommentTarget {
                path: self.old_path.clone()?,
                line: line.old_lineno?,
                side: Side::Left,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentTarget {
    pub path: String,
    pub line: u32,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiff {
    pub files: Vec<FileDiff>,
}

pub fn parse(input: &str) -> ParsedDiff {
    ParsedDiff {
        files: split_files(input).into_iter().map(parse_file).collect(),
    }
}

fn split_files(input: &str) -> Vec<Vec<(usize, &str)>> {
    let mut chunks: Vec<Vec<(usize, &str)>> = Vec::new();
    for (raw_idx, line) in input.lines().enumerate() {
        if line.starts_with("diff --git ") || (chunks.is_empty() && line.starts_with("--- ")) {
            chunks.push(Vec::new());
        }
        if let Some(last) = chunks.last_mut() {
            last.push((raw_idx, line));
        }
    }
    chunks
}

fn parse_file(lines: Vec<(usize, &str)>) -> FileDiff {
    let mut file = FileDiff {
        old_path: None,
        new_path: None,
        is_binary: false,
        hunks: Vec::new(),
    };
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut in_hunk = false;

    for (raw_idx, line) in lines {
        if line.starts_with("@@") {
            let (o, n) = parse_hunk_header(line);
            old_no = o;
            new_no = n;
            file.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            read_header_line(&mut file, line);
            continue;
        }

        // "\ No newline at end of file" is a property of the line above, not a line of its own
        if line.starts_with('\\') {
            continue;
        }

        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Removed, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            // ponytail: a line with no leading marker ends the hunk. Both git and GitHub emit an
            // empty context line as " ", so this never fires in practice — but against an
            // implementation that drops the marker, stopping beats shifting every line number
            _ => {
                in_hunk = false;
                read_header_line(&mut file, line);
                continue;
            }
        };

        let (old_lineno, new_lineno) = match kind {
            LineKind::Added => {
                new_no += 1;
                (None, Some(new_no))
            }
            LineKind::Removed => {
                old_no += 1;
                (Some(old_no), None)
            }
            LineKind::Context => {
                old_no += 1;
                new_no += 1;
                (Some(old_no), Some(new_no))
            }
        };

        file.hunks.last_mut().expect("hunk pushed above").lines.push(DiffLine {
            kind,
            old_lineno,
            new_lineno,
            text: text.to_string(),
            raw_idx,
        });
    }

    file
}

fn read_header_line(file: &mut FileDiff, line: &str) {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        // ponytail: a path containing " b/" splits wrong. If that ever bites, trust only --- / +++
        if let Some((a, b)) = rest.split_once(" b/") {
            file.old_path = Some(a.strip_prefix("a/").unwrap_or(a).to_string());
            file.new_path = Some(b.to_string());
        }
    } else if let Some(p) = line.strip_prefix("--- ") {
        file.old_path = header_path(p);
    } else if let Some(p) = line.strip_prefix("+++ ") {
        file.new_path = header_path(p);
    } else if let Some(p) = line.strip_prefix("rename from ") {
        file.old_path = Some(p.to_string());
    } else if let Some(p) = line.strip_prefix("rename to ") {
        file.new_path = Some(p.to_string());
    } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
        file.is_binary = true;
    }
}

fn header_path(raw: &str) -> Option<String> {
    let path = raw.split('\t').next().unwrap_or(raw).trim();
    if path == "/dev/null" {
        return None;
    }
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);
    Some(path.to_string())
}

/// From `@@ -10,6 +10,7 @@ fn login() {`, the number just before each side's first line
fn parse_hunk_header(line: &str) -> (u32, u32) {
    let body = line.trim_start_matches('@').trim();
    let body = body.split("@@").next().unwrap_or("");
    let mut old = 0u32;
    let mut new = 0u32;
    for token in body.split_whitespace() {
        let (target, rest) = match token.chars().next() {
            Some('-') => (&mut old, &token[1..]),
            Some('+') => (&mut new, &token[1..]),
            _ => continue,
        };
        *target = rest
            .split(',')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
    }
    (old.saturating_sub(1), new.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = "\
diff --git a/src/auth.rs b/src/auth.rs
index 1111111..2222222 100644
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -10,6 +10,7 @@ fn login() {
 let token = read_token();
-if token.is_none() {
+if token.is_empty() {
+    return Err(Unauthorized);
 }
 verify(token)
";

    #[test]
    fn parses_single_file_paths() {
        let d = parse(SIMPLE);
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].old_path.as_deref(), Some("src/auth.rs"));
        assert_eq!(d.files[0].new_path.as_deref(), Some("src/auth.rs"));
        assert!(!d.files[0].is_binary);
    }

    #[test]
    fn assigns_line_numbers_on_both_sides() {
        let d = parse(SIMPLE);
        let lines = &d.files[0].hunks[0].lines;

        // " let token = read_token();"
        assert_eq!(lines[0].kind, LineKind::Context);
        assert_eq!(lines[0].old_lineno, Some(10));
        assert_eq!(lines[0].new_lineno, Some(10));

        // "-if token.is_none() {"
        assert_eq!(lines[1].kind, LineKind::Removed);
        assert_eq!(lines[1].old_lineno, Some(11));
        assert_eq!(lines[1].new_lineno, None);

        // "+if token.is_empty() {"
        assert_eq!(lines[2].kind, LineKind::Added);
        assert_eq!(lines[2].old_lineno, None);
        assert_eq!(lines[2].new_lineno, Some(11));

        // "+    return Err(Unauthorized);"
        assert_eq!(lines[3].new_lineno, Some(12));

        // " }" — the context line after 1 removal and 2 additions
        assert_eq!(lines[4].kind, LineKind::Context);
        assert_eq!(lines[4].old_lineno, Some(12));
        assert_eq!(lines[4].new_lineno, Some(13));
    }

    #[test]
    fn keeps_line_text_without_marker() {
        let d = parse(SIMPLE);
        assert_eq!(d.files[0].hunks[0].lines[2].text, "if token.is_empty() {");
        assert_eq!(d.files[0].hunks[0].header, "@@ -10,6 +10,7 @@ fn login() {");
    }

    #[test]
    fn counts_additions_and_deletions() {
        let d = parse(SIMPLE);
        assert_eq!(d.files[0].additions(), 2);
        assert_eq!(d.files[0].deletions(), 1);
    }

    const MULTI: &str = "\
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,2 @@
+fn brand_new() {}
+
diff --git a/src/gone.rs b/src/gone.rs
deleted file mode 100644
index 4444444..0000000
--- a/src/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn removed() {}
-
diff --git a/old_name.rs b/new_name.rs
similarity index 100%
rename from old_name.rs
rename to new_name.rs
diff --git a/logo.png b/logo.png
index 5555555..6666666 100644
Binary files a/logo.png and b/logo.png differ
";

    #[test]
    fn splits_every_file() {
        let d = parse(MULTI);
        assert_eq!(d.files.len(), 4);
    }

    #[test]
    fn marks_added_file_with_no_old_path() {
        let f = &parse(MULTI).files[0];
        assert_eq!(f.old_path, None);
        assert_eq!(f.new_path.as_deref(), Some("src/new.rs"));
        assert_eq!(f.display_path(), "src/new.rs");
        assert_eq!(f.hunks[0].lines[0].new_lineno, Some(1));
        assert_eq!(f.hunks[0].lines[0].old_lineno, None);
    }

    #[test]
    fn marks_deleted_file_with_no_new_path() {
        let f = &parse(MULTI).files[1];
        assert_eq!(f.old_path.as_deref(), Some("src/gone.rs"));
        assert_eq!(f.new_path, None);
        assert_eq!(f.display_path(), "src/gone.rs");
        assert_eq!(f.hunks[0].lines[0].old_lineno, Some(1));
    }

    #[test]
    fn detects_rename_without_hunks() {
        let f = &parse(MULTI).files[2];
        assert_eq!(f.old_path.as_deref(), Some("old_name.rs"));
        assert_eq!(f.new_path.as_deref(), Some("new_name.rs"));
        assert!(f.is_rename());
        assert!(f.hunks.is_empty());
    }

    #[test]
    fn detects_binary_file() {
        let f = &parse(MULTI).files[3];
        assert!(f.is_binary);
        assert!(f.hunks.is_empty());
    }

    #[test]
    fn binary_file_keeps_its_path() {
        let f = &parse(MULTI).files[3];
        assert_eq!(f.old_path.as_deref(), Some("logo.png"));
        assert_eq!(f.new_path.as_deref(), Some("logo.png"));
        assert_eq!(f.display_path(), "logo.png");
    }

    const MULTI_HUNK: &str = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-one
+ONE
 two
@@ -50,2 +50,3 @@
 fifty
+fifty-one
 fifty-two
";

    #[test]
    fn restarts_line_numbers_at_each_hunk() {
        let f = &parse(MULTI_HUNK).files[0];
        assert_eq!(f.hunks.len(), 2);
        assert_eq!(f.hunks[0].lines[0].old_lineno, Some(1));
        assert_eq!(f.hunks[1].lines[0].new_lineno, Some(50));
        assert_eq!(f.hunks[1].lines[1].new_lineno, Some(51));
        assert_eq!(f.hunks[1].lines[2].new_lineno, Some(52));
    }

    const NO_NEWLINE: &str = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old
\\ No newline at end of file
+new
\\ No newline at end of file
";

    #[test]
    fn ignores_no_newline_marker() {
        let f = &parse(NO_NEWLINE).files[0];
        assert_eq!(f.hunks[0].lines.len(), 2);
        assert_eq!(f.hunks[0].lines[0].text, "old");
        assert_eq!(f.hunks[0].lines[1].text, "new");
        assert_eq!(f.hunks[0].lines[1].new_lineno, Some(1));
    }

    #[test]
    fn parses_hunk_header_without_counts() {
        let f = &parse(NO_NEWLINE).files[0];
        assert_eq!(f.hunks[0].lines[0].old_lineno, Some(1));
    }

    #[test]
    fn added_line_targets_right_side() {
        let f = &parse(SIMPLE).files[0];
        let line = &f.hunks[0].lines[2]; // "+if token.is_empty() {"
        let t = f.comment_target(line).unwrap();
        assert_eq!(t.path, "src/auth.rs");
        assert_eq!(t.line, 11);
        assert_eq!(t.side, Side::Right);
    }

    #[test]
    fn context_line_targets_right_side() {
        let f = &parse(SIMPLE).files[0];
        let line = &f.hunks[0].lines[0]; // " let token = read_token();"
        let t = f.comment_target(line).unwrap();
        assert_eq!(t.line, 10);
        assert_eq!(t.side, Side::Right);
    }

    #[test]
    fn removed_line_targets_left_side_with_old_number() {
        let f = &parse(SIMPLE).files[0];
        let line = &f.hunks[0].lines[1]; // "-if token.is_none() {"
        let t = f.comment_target(line).unwrap();
        assert_eq!(t.path, "src/auth.rs");
        assert_eq!(t.line, 11);
        assert_eq!(t.side, Side::Left);
    }

    #[test]
    fn removed_line_of_deleted_file_uses_old_path() {
        let f = &parse(MULTI).files[1]; // src/gone.rs
        let line = &f.hunks[0].lines[0];
        let t = f.comment_target(line).unwrap();
        assert_eq!(t.path, "src/gone.rs");
        assert_eq!(t.side, Side::Left);
    }

    #[test]
    fn binary_file_has_no_comment_target() {
        let f = &parse(MULTI).files[3];
        let line = DiffLine {
            kind: LineKind::Added,
            old_lineno: None,
            new_lineno: Some(1),
            text: String::new(),
            raw_idx: 0,
        };
        assert!(f.comment_target(&line).is_none());
    }

    #[test]
    fn raw_idx_points_back_at_the_source_line() {
        let d = parse(SIMPLE);
        let src: Vec<&str> = SIMPLE.lines().collect();
        for line in &d.files[0].hunks[0].lines {
            assert_eq!(&src[line.raw_idx][1..], line.text);
        }
    }

    #[test]
    fn raw_idx_keeps_counting_across_files() {
        let d = parse(MULTI);
        let src: Vec<&str> = MULTI.lines().collect();
        let deleted = &d.files[1].hunks[0].lines[0]; // "-fn removed() {}" in the second file
        assert_eq!(src[deleted.raw_idx], "-fn removed() {}");
    }
}
