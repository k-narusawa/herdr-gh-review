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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub text: String,
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

fn split_files(input: &str) -> Vec<Vec<&str>> {
    let mut chunks: Vec<Vec<&str>> = Vec::new();
    for line in input.lines() {
        if line.starts_with("diff --git ") || (chunks.is_empty() && line.starts_with("--- ")) {
            chunks.push(Vec::new());
        }
        if let Some(last) = chunks.last_mut() {
            last.push(line);
        }
    }
    chunks
}

fn parse_file(lines: Vec<&str>) -> FileDiff {
    let mut file = FileDiff {
        old_path: None,
        new_path: None,
        is_binary: false,
        hunks: Vec::new(),
    };
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut in_hunk = false;

    for line in lines {
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

        // "\ No newline at end of file" は直前の行の属性であって、行そのものではない
        if line.starts_with('\\') {
            continue;
        }

        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Removed, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            // ponytail: 先頭マーカーの無い行はハンク終端とみなす。git/GitHub のどちらも
            // 空の文脈行を " " として出すため実際には踏まないが、マーカーを落とす
            // 実装に当たった場合は行番号がずれるより打ち切る方が安全
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
        });
    }

    file
}

fn read_header_line(file: &mut FileDiff, line: &str) {
    if let Some(p) = line.strip_prefix("--- ") {
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

/// `@@ -10,6 +10,7 @@ fn login() {` から、各サイドの「1行目の1つ前」の番号を返す
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

        // " }" — 削除1行・追加2行のあとの文脈行
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
}
