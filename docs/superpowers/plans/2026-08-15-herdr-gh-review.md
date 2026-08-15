# herdr-gh-review 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GitHubのPull Requestをherdrのペイン内で読み、行コメントを書き、レビューとして提出できるRust製TUIプラグインを作る。

**Architecture:** 単一のRustバイナリ `herdr-gh-review` を herdr のプラグインペインで起動する。GitHubとの通信はHTTPクライアントを自前で持たず `gh` コマンドの呼び出しに委譲する。中核は「unified diffのパース（`diff.rs`）」と「下書きコメント→APIリクエストの組み立て（`review.rs`）」という2つの純粋なロジックで、ここにテストを集中させる。TUIは ratatui で描画し、全ファイルのdiffを縦に連結した1画面スクロールとして見せる。

**Tech Stack:** Rust 1.97.1 (edition 2024) / ratatui 0.30 / crossterm 0.29 / serde + serde_json / anyhow / tempfile / gh CLI 2.97

**Spec:** `docs/superpowers/specs/2026-08-15-herdr-gh-review-design.md`

## Global Constraints

- Rustツールチェーンは **1.97.1**、edition は **2024**。現在システムに入っている Homebrew の Rust 1.84.0 では依存クレートが `edition2024` 機能を要求してビルドに失敗する（実機で確認済み）。Task 1 でmise管理に切り替える。
- edition 2024 では `std::env::set_var` が `unsafe` になっている。**環境変数を書き換える実装は取らない**（`gh` は絶対パスを解決して起動する）。
- 対応プラットフォームは macOS と Linux のみ。
- herdr は **0.7.5以上**を前提とする（`--env` 付きの `plugin pane open` が必要）。
- プラグインID は `k-narusawa.gh-review`。
- **レビュー提出が失敗した場合、下書きは絶対に削除しない。**
- **下書きの型 `DraftComment` は `path` / `line` / `side` / `body` の4つだけを持つ。** 第2段階のAI連携でそのまま整形して使えるよう、GitHub API都合のフィールドを足さない。
- コミットは Conventional Commits 形式、subject は日本語、末尾に句点を付けない。
- コード内コメントは、メソッド名を見れば分かることには書かない。意図的に妥協した箇所には `ponytail:` コメントで上限と改善の道筋を書く。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `Cargo.toml` / `.mise.toml` | 依存とツールチェーンの固定 |
| `src/main.rs` | 起動、Target解決、端末セットアップ、画面遷移 |
| `src/gh.rs` | `gh` コマンドの呼び出しとJSONの構造体化。外部通信はすべてここ |
| `src/diff.rs` | unified diff のパースと、行→コメント対象の解決 |
| `src/review.rs` | 下書きコメントの保持・永続化・APIリクエスト組み立て |
| `src/app.rs` | アプリ状態、表示行の平坦化、カーソル移動 |
| `src/editor.rs` | `$EDITOR` を起動してテキストを受け取る |
| `src/ui/diffview.rs` | diff閲覧画面の描画 |
| `src/ui/prlist.rs` | PR一覧画面の描画 |
| `src/ui/submit.rs` | 提出ダイアログの描画 |
| `herdr-plugin.toml` | herdrへの登録情報 |
| `herdr/install.sh` | `plugin install` 時のビルド |
| `herdr/pane.sh` | ペインのopen/close/toggle |

---

### Task 1: プロジェクト初期化

**Files:**
- Create: `.mise.toml`, `Cargo.toml`, `src/main.rs`
- Modify: `.gitignore`

- [ ] **Step 1: Rustツールチェーンをmiseで固定する**

```bash
cd /Users/narusawakohei/projects/herdr-gh-review
mise use rust@1.97.1
```

期待: `.mise.toml` が作られる。`cargo --version` が `cargo 1.97.x` を返すこと。`1.84.0` のままなら `mise reshim` を実行し、それでも変わらなければ新しいシェルで確認する。

- [ ] **Step 2: cargoプロジェクトを作る**

```bash
cargo init --name herdr-gh-review
```

- [ ] **Step 3: Cargo.tomlを書く**

```toml
[package]
name = "herdr-gh-review"
version = "0.1.0"
edition = "2024"

[dependencies]
ratatui = "0.30"
crossterm = "0.29"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tempfile = "3"

[profile.release]
strip = true
```

- [ ] **Step 4: .gitignoreを更新する**

```
target/
bin/
```

- [ ] **Step 5: ビルドが通ることを確認する**

Run: `cargo build`
Expected: 成功する。Rust 1.84 のままだと `feature 'edition2024' is required` で失敗するので、その場合は Step 1 に戻る。

- [ ] **Step 6: コミット**

```bash
git add -A
git commit -m "chore: Rustプロジェクトを初期化"
```

---

### Task 2: diff.rs — 型定義と基本パース

diffのパースがこのプラグインの土台。ここが1行でもずれると、コメントが別の行に付く。テストを厚くする。

**Files:**
- Create: `src/diff.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `LineKind`, `Side`, `DiffLine`, `Hunk`, `FileDiff`, `ParsedDiff`, `parse(&str) -> ParsedDiff`

- [ ] **Step 1: 失敗するテストを書く**

`src/diff.rs` の末尾に置く。

```rust
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
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test --lib` （まだ `src/lib.rs` が無いので `cargo test` でよい）
Expected: コンパイルエラー。`cannot find function 'parse'`

- [ ] **Step 3: 型と`parse`を実装する**

`src/diff.rs` の先頭に置く。

```rust
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
```

`src/main.rs` に `mod diff;` を追加する。

```rust
mod diff;

fn main() {
    println!("herdr-gh-review");
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 4件すべてPASS

- [ ] **Step 5: コミット**

```bash
git add src/diff.rs src/main.rs
git commit -m "feat(diff): unified diffのパーサを追加"
```

---

### Task 3: diff.rs — 新規/削除/リネーム/バイナリ/複数ファイル

**Files:**
- Modify: `src/diff.rs`

**Interfaces:**
- Consumes: Task 2 の `parse`
- Produces: 変更なし（既存関数がエッジケースを正しく扱えるようになる）

- [ ] **Step 1: 失敗するテストを書く**

`src/diff.rs` の `mod tests` に追加する。

```rust
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
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: `splits_every_file` などが失敗する可能性がある。Task 2 の実装で通るものもあるが、**通ったテストも消さない**（回帰の網になる）。

- [ ] **Step 3: 失敗したテストに合わせて実装を直す**

想定される修正点は2つ:

1. リネームのみ・バイナリのみのファイルは `@@` を持たないため、`in_hunk` が `false` のまま `read_header_line` を通る。Task 2 の実装で既に対応済み。
2. `MULTI` の新規ファイルの最終行 `+` （空行の追加）は `line[1..]` が空文字になる。これも対応済み。

失敗が残る場合はその箇所だけを直す。実装を大きく書き換えないこと。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全11件PASS

- [ ] **Step 5: 実物のdiffで確認する**

```bash
gh pr diff 14148 --repo cli/cli > /tmp/real.diff && wc -l /tmp/real.diff
```

一時的なテストを書いて実物を通す。

```rust
    #[test]
    #[ignore]
    fn parses_real_diff() {
        let raw = std::fs::read_to_string("/tmp/real.diff").unwrap();
        let d = parse(&raw);
        assert!(!d.files.is_empty());
        for f in &d.files {
            for h in &f.hunks {
                for l in &h.lines {
                    assert!(l.old_lineno.is_some() || l.new_lineno.is_some());
                }
            }
        }
        eprintln!("{} files", d.files.len());
    }
```

Run: `cargo test -- --ignored parses_real_diff --nocapture`
Expected: PASS。ファイル数が `gh pr diff 14148 --repo cli/cli | grep -c '^diff --git'` と一致すること。

- [ ] **Step 6: コミット**

```bash
git add src/diff.rs
git commit -m "feat(diff): 新規・削除・リネーム・バイナリファイルに対応"
```

---

### Task 4: diff.rs — コメント対象の解決

diffの行から「GitHubのどこにコメントするか」を決める。仕様上もっとも間違えやすい箇所。

**Files:**
- Modify: `src/diff.rs`

**Interfaces:**
- Produces: `CommentTarget { path: String, line: u32, side: Side }`, `FileDiff::comment_target(&self, line: &DiffLine) -> Option<CommentTarget>`

- [ ] **Step 1: 失敗するテストを書く**

```rust
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
        };
        assert!(f.comment_target(&line).is_none());
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test comment_target`
Expected: コンパイルエラー。`no method named 'comment_target'`

- [ ] **Step 3: 実装する**

`impl FileDiff` に追加する。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentTarget {
    pub path: String,
    pub line: u32,
    pub side: Side,
}
```

```rust
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
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全16件PASS

- [ ] **Step 5: コミット**

```bash
git add src/diff.rs
git commit -m "feat(diff): diff行からコメント対象を解決する処理を追加"
```

---

### Task 5: review.rs — 下書きモデルとAPIリクエスト組み立て

**Files:**
- Create: `src/review.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `diff::{Side, CommentTarget}`
- Produces: `DraftComment`, `Draft`, `ReviewEvent`, `ReviewError`, `Draft::{new, upsert_comment, remove_comment, comment_at}`, `build_review_request(&Draft, ReviewEvent) -> Result<serde_json::Value, ReviewError>`

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft_with_two_comments() -> Draft {
        let mut d = Draft::new("k-narusawa/app", 42, "abc123");
        d.upsert_comment(
            CommentTarget { path: "src/auth.rs".into(), line: 11, side: Side::Right },
            "null時に401を返すべきでは？".into(),
        );
        d.upsert_comment(
            CommentTarget { path: "src/auth.rs".into(), line: 11, side: Side::Left },
            "この分岐は消して良いのか".into(),
        );
        d
    }

    #[test]
    fn builds_request_with_both_sides() {
        let req = build_review_request(&draft_with_two_comments(), ReviewEvent::Comment).unwrap();
        assert_eq!(
            req,
            json!({
                "commit_id": "abc123",
                "event": "COMMENT",
                "body": "",
                "comments": [
                    { "path": "src/auth.rs", "line": 11, "side": "RIGHT", "body": "null時に401を返すべきでは？" },
                    { "path": "src/auth.rs", "line": 11, "side": "LEFT", "body": "この分岐は消して良いのか" }
                ]
            })
        );
    }

    #[test]
    fn approve_with_body_only_is_allowed() {
        let mut d = Draft::new("k-narusawa/app", 42, "abc123");
        d.body = "LGTM".into();
        let req = build_review_request(&d, ReviewEvent::Approve).unwrap();
        assert_eq!(req["event"], "APPROVE");
        assert_eq!(req["body"], "LGTM");
        assert_eq!(req["comments"], json!([]));
    }

    #[test]
    fn request_changes_without_body_is_rejected() {
        let d = draft_with_two_comments();
        assert_eq!(
            build_review_request(&d, ReviewEvent::RequestChanges).unwrap_err(),
            ReviewError::BodyRequired
        );
    }

    #[test]
    fn empty_draft_is_rejected() {
        let d = Draft::new("k-narusawa/app", 42, "abc123");
        assert_eq!(
            build_review_request(&d, ReviewEvent::Comment).unwrap_err(),
            ReviewError::Empty
        );
    }

    #[test]
    fn upsert_replaces_comment_on_same_target() {
        let mut d = draft_with_two_comments();
        d.upsert_comment(
            CommentTarget { path: "src/auth.rs".into(), line: 11, side: Side::Right },
            "書き直した".into(),
        );
        assert_eq!(d.comments.len(), 2);
        let t = CommentTarget { path: "src/auth.rs".into(), line: 11, side: Side::Right };
        assert_eq!(d.comment_at(&t).unwrap().body, "書き直した");
    }

    #[test]
    fn remove_comment_deletes_only_matching_side() {
        let mut d = draft_with_two_comments();
        let t = CommentTarget { path: "src/auth.rs".into(), line: 11, side: Side::Right };
        d.remove_comment(&t);
        assert_eq!(d.comments.len(), 1);
        assert_eq!(d.comments[0].side, Side::Left);
    }

    #[test]
    fn draft_survives_json_roundtrip() {
        let d = draft_with_two_comments();
        let json = serde_json::to_string(&d).unwrap();
        let back: Draft = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`unresolved module 'review'`

- [ ] **Step 3: 実装する**

`src/review.rs`:

```rust
use crate::diff::{CommentTarget, Side};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftComment {
    pub path: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub repo: String,
    pub pr_number: u32,
    pub head_sha: String,
    pub body: String,
    pub comments: Vec<DraftComment>,
}

impl Draft {
    pub fn new(repo: &str, pr_number: u32, head_sha: &str) -> Self {
        Self {
            repo: repo.to_string(),
            pr_number,
            head_sha: head_sha.to_string(),
            body: String::new(),
            comments: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.body.trim().is_empty() && self.comments.is_empty()
    }

    fn position_of(&self, target: &CommentTarget) -> Option<usize> {
        self.comments
            .iter()
            .position(|c| c.path == target.path && c.line == target.line && c.side == target.side)
    }

    pub fn comment_at(&self, target: &CommentTarget) -> Option<&DraftComment> {
        self.position_of(target).map(|i| &self.comments[i])
    }

    pub fn upsert_comment(&mut self, target: CommentTarget, body: String) {
        match self.position_of(&target) {
            Some(i) => self.comments[i].body = body,
            None => self.comments.push(DraftComment {
                path: target.path,
                line: target.line,
                side: target.side,
                body,
            }),
        }
    }

    pub fn remove_comment(&mut self, target: &CommentTarget) {
        if let Some(i) = self.position_of(target) {
            self.comments.remove(i);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewEvent {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewEvent {
    pub fn as_api_str(self) -> &'static str {
        match self {
            ReviewEvent::Comment => "COMMENT",
            ReviewEvent::Approve => "APPROVE",
            ReviewEvent::RequestChanges => "REQUEST_CHANGES",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReviewEvent::Comment => "Comment",
            ReviewEvent::Approve => "Approve",
            ReviewEvent::RequestChanges => "Request changes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewError {
    /// REQUEST_CHANGES は本文が必須（GitHubが空本文を拒否する）
    BodyRequired,
    Empty,
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewError::BodyRequired => write!(f, "Request changes には全体コメントが必要です"),
            ReviewError::Empty => write!(f, "提出する内容がありません"),
        }
    }
}

impl std::error::Error for ReviewError {}

pub fn build_review_request(
    draft: &Draft,
    event: ReviewEvent,
) -> Result<serde_json::Value, ReviewError> {
    if draft.is_empty() && event != ReviewEvent::Approve {
        return Err(ReviewError::Empty);
    }
    if event == ReviewEvent::RequestChanges && draft.body.trim().is_empty() {
        return Err(ReviewError::BodyRequired);
    }

    Ok(json!({
        "commit_id": draft.head_sha,
        "event": event.as_api_str(),
        "body": draft.body,
        "comments": draft.comments,
    }))
}
```

`src/main.rs` に `mod review;` を追加する。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全23件PASS

- [ ] **Step 5: コミット**

```bash
git add src/review.rs src/main.rs
git commit -m "feat(review): 下書きモデルとレビューAPIリクエストの組み立てを追加"
```

---

### Task 6: review.rs — 下書きの永続化

**Files:**
- Modify: `src/review.rs`

**Interfaces:**
- Produces: `state_dir() -> PathBuf`, `draft_path(&Path, &str, u32) -> PathBuf`, `save(&Path, &Draft) -> Result<()>`, `load(&Path, &str, u32) -> Result<Option<Draft>>`, `delete(&Path, &str, u32) -> Result<()>`

- [ ] **Step 1: 失敗するテストを書く**

```rust
    #[test]
    fn draft_path_flattens_repo_slash() {
        let p = draft_path(std::path::Path::new("/state"), "k-narusawa/app", 42);
        assert_eq!(p, std::path::Path::new("/state/drafts/k-narusawa-app-42.json"));
    }

    #[test]
    fn save_then_load_returns_same_draft() {
        let dir = tempfile::tempdir().unwrap();
        let d = draft_with_two_comments();
        save(dir.path(), &d).unwrap();
        let loaded = load(dir.path(), "k-narusawa/app", 42).unwrap();
        assert_eq!(loaded, Some(d));
    }

    #[test]
    fn load_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path(), "k-narusawa/app", 42).unwrap(), None);
    }

    #[test]
    fn delete_removes_the_file_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &draft_with_two_comments()).unwrap();
        delete(dir.path(), "k-narusawa/app", 42).unwrap();
        assert_eq!(load(dir.path(), "k-narusawa/app", 42).unwrap(), None);
        delete(dir.path(), "k-narusawa/app", 42).unwrap();
    }

    #[test]
    fn load_returns_none_when_file_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let p = draft_path(dir.path(), "k-narusawa/app", 42);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{ broken").unwrap();
        assert_eq!(load(dir.path(), "k-narusawa/app", 42).unwrap(), None);
    }
```

`tempfile` を dev 用途にも使うため、`Cargo.toml` の `[dependencies]` に既に入っていることを確認する（Task 1 で追加済み）。

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`cannot find function 'draft_path'`

- [ ] **Step 3: 実装する**

`src/review.rs` に追加する。

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("HERDR_PLUGIN_STATE_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/state/herdr/plugins/k-narusawa.gh-review")
}

pub fn draft_path(state_dir: &Path, repo: &str, pr_number: u32) -> PathBuf {
    state_dir
        .join("drafts")
        .join(format!("{}-{}.json", repo.replace('/', "-"), pr_number))
}

pub fn save(state_dir: &Path, draft: &Draft) -> Result<()> {
    let path = draft_path(state_dir, &draft.repo, draft.pr_number);
    std::fs::create_dir_all(path.parent().expect("draft path has a parent"))
        .with_context(|| format!("下書きディレクトリを作れません: {}", path.display()))?;
    let json = serde_json::to_string_pretty(draft)?;
    std::fs::write(&path, json)
        .with_context(|| format!("下書きを保存できません: {}", path.display()))
}

/// 壊れた下書きは無いものとして扱う。読めないファイルのために起動できない方が損失が大きい
pub fn load(state_dir: &Path, repo: &str, pr_number: u32) -> Result<Option<Draft>> {
    let path = draft_path(state_dir, repo, pr_number);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&raw).ok())
}

pub fn delete(state_dir: &Path, repo: &str, pr_number: u32) -> Result<()> {
    let path = draft_path(state_dir, repo, pr_number);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("下書きを削除できません: {}", path.display())),
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全28件PASS

- [ ] **Step 5: コミット**

```bash
git add src/review.rs
git commit -m "feat(review): 下書きのローカル永続化を追加"
```

---

### Task 7: gh.rs — GitHubとのやりとり

外部通信はすべてこのファイルに閉じ込める。JSONを構造体に変換する部分だけをテストする。

**Files:**
- Create: `src/gh.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `review::{Draft, ReviewEvent, build_review_request}`
- Produces: `PrSummary`, `PrDetail`, `Gh::{new, list_prs, search_review_requested, pr_detail, pr_diff, submit_review, open_in_browser}`, `repo_from_url(&str) -> Option<(String, u32)>`

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PR_LIST_JSON: &str = r#"[
      {"additions":36,"author":{"id":"MDQ6VXNlcjE1","is_bot":false,"login":"heaths","name":"Heath Stewart"},
       "deletions":40,"isDraft":true,"number":14148,"title":"Update glamour to v2"}
    ]"#;

    const SEARCH_JSON: &str = r#"[
      {"author":{"id":"MDQ6VXNlcjI3","is_bot":false,"login":"steveklabnik"},"isDraft":true,"number":2413,
       "repository":{"name":"rue","nameWithOwner":"rue-language/rue"},
       "title":"remove duplicate scans","url":"https://github.com/rue-language/rue/pull/2413"}
    ]"#;

    const PR_VIEW_JSON: &str = r#"{"additions":36,"author":{"id":"MDQ6VXNlcjE1","is_bot":false,"login":"heaths","name":"Heath Stewart"},
      "deletions":40,"headRefOid":"f6260aa5f65b721454cbe36d3d66b9b860c08f9b","number":14148,
      "title":"Update glamour to v2","url":"https://github.com/cli/cli/pull/14148"}"#;

    #[test]
    fn parses_pr_list() {
        let prs = parse_pr_list(PR_LIST_JSON).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 14148);
        assert_eq!(prs[0].author, "heaths");
        assert_eq!(prs[0].additions, Some(36));
        assert_eq!(prs[0].deletions, Some(40));
        assert!(prs[0].is_draft);
        assert_eq!(prs[0].repo, None);
    }

    #[test]
    fn parses_search_results_with_repo() {
        let prs = parse_search(SEARCH_JSON).unwrap();
        assert_eq!(prs[0].repo.as_deref(), Some("rue-language/rue"));
        assert_eq!(prs[0].number, 2413);
        assert_eq!(prs[0].author, "steveklabnik");
        // search API は増減行数を返さない
        assert_eq!(prs[0].additions, None);
    }

    #[test]
    fn parses_pr_detail_and_derives_repo_from_url() {
        let d = parse_pr_detail(PR_VIEW_JSON).unwrap();
        assert_eq!(d.number, 14148);
        assert_eq!(d.head_sha, "f6260aa5f65b721454cbe36d3d66b9b860c08f9b");
        assert_eq!(d.repo, "cli/cli");
    }

    #[test]
    fn extracts_repo_and_number_from_pr_url() {
        assert_eq!(
            repo_from_url("https://github.com/cli/cli/pull/14148"),
            Some(("cli/cli".to_string(), 14148))
        );
        assert_eq!(
            repo_from_url("https://github.com/cli/cli/pull/14148/files#r123"),
            Some(("cli/cli".to_string(), 14148))
        );
        assert_eq!(repo_from_url("https://github.com/cli/cli/issues/1"), None);
        assert_eq!(repo_from_url("not a url"), None);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`unresolved module 'gh'`

- [ ] **Step 3: 実装する**

`src/gh.rs`:

```rust
use crate::review::{Draft, ReviewEvent, build_review_request};
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSummary {
    pub repo: Option<String>,
    pub number: u32,
    pub title: String,
    pub author: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub is_draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDetail {
    pub repo: String,
    pub number: u32,
    pub title: String,
    pub author: String,
    pub additions: u32,
    pub deletions: u32,
    pub head_sha: String,
    pub url: String,
}

#[derive(Deserialize)]
struct RawAuthor {
    login: String,
}

#[derive(Deserialize)]
struct RawRepository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPrListItem {
    number: u32,
    title: String,
    author: RawAuthor,
    additions: u32,
    deletions: u32,
    is_draft: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSearchItem {
    number: u32,
    title: String,
    author: RawAuthor,
    repository: RawRepository,
    is_draft: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPrView {
    number: u32,
    title: String,
    author: RawAuthor,
    additions: u32,
    deletions: u32,
    head_ref_oid: String,
    url: String,
}

pub fn parse_pr_list(json: &str) -> Result<Vec<PrSummary>> {
    let raw: Vec<RawPrListItem> = serde_json::from_str(json)?;
    Ok(raw
        .into_iter()
        .map(|r| PrSummary {
            repo: None,
            number: r.number,
            title: r.title,
            author: r.author.login,
            additions: Some(r.additions),
            deletions: Some(r.deletions),
            is_draft: r.is_draft,
        })
        .collect())
}

pub fn parse_search(json: &str) -> Result<Vec<PrSummary>> {
    let raw: Vec<RawSearchItem> = serde_json::from_str(json)?;
    Ok(raw
        .into_iter()
        .map(|r| PrSummary {
            repo: Some(r.repository.name_with_owner),
            number: r.number,
            title: r.title,
            author: r.author.login,
            additions: None,
            deletions: None,
            is_draft: r.is_draft,
        })
        .collect())
}

pub fn parse_pr_detail(json: &str) -> Result<PrDetail> {
    let r: RawPrView = serde_json::from_str(json)?;
    let repo = repo_from_url(&r.url)
        .map(|(repo, _)| repo)
        .ok_or_else(|| anyhow!("PRのURLからリポジトリを特定できません: {}", r.url))?;
    Ok(PrDetail {
        repo,
        number: r.number,
        title: r.title,
        author: r.author.login,
        additions: r.additions,
        deletions: r.deletions,
        head_sha: r.head_ref_oid,
        url: r.url,
    })
}

pub fn repo_from_url(url: &str) -> Option<(String, u32)> {
    let rest = url.split("github.com/").nth(1)?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next()? != "pull" {
        return None;
    }
    let number: u32 = parts
        .next()?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((format!("{owner}/{repo}"), number))
}

pub struct Gh {
    bin: PathBuf,
}

impl Gh {
    pub fn new() -> Result<Self> {
        let bin = find_gh().ok_or_else(|| {
            anyhow!("gh コマンドが見つかりません。https://cli.github.com/ からインストールしてください")
        })?;
        let gh = Self { bin };
        gh.run(&["auth", "status"])
            .context("GitHubにログインしていません。`gh auth login` を実行してください")?;
        Ok(gh)
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("gh {} の起動に失敗しました", args.join(" ")))?;
        if !out.status.success() {
            bail!(
                "gh {} が失敗しました: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn list_prs(&self) -> Result<Vec<PrSummary>> {
        let json = self.run(&[
            "pr", "list", "--limit", "50", "--json",
            "number,title,author,additions,deletions,isDraft",
        ])?;
        parse_pr_list(&json)
    }

    pub fn search_review_requested(&self) -> Result<Vec<PrSummary>> {
        let json = self.run(&[
            "search", "prs", "--review-requested=@me", "--state=open",
            "--limit", "50", "--json", "number,title,author,repository,isDraft,url",
        ])?;
        parse_search(&json)
    }

    pub fn pr_detail(&self, repo: Option<&str>, number: u32) -> Result<PrDetail> {
        let number = number.to_string();
        let mut args = vec![
            "pr", "view", &number, "--json",
            "number,title,author,additions,deletions,headRefOid,url",
        ];
        if let Some(repo) = repo {
            args.extend_from_slice(&["--repo", repo]);
        }
        parse_pr_detail(&self.run(&args)?)
    }

    pub fn pr_diff(&self, repo: &str, number: u32) -> Result<String> {
        let number = number.to_string();
        self.run(&["pr", "diff", &number, "--repo", repo])
    }

    pub fn open_in_browser(&self, repo: &str, number: u32) -> Result<()> {
        let number = number.to_string();
        self.run(&["pr", "view", &number, "--repo", repo, "--web"])?;
        Ok(())
    }

    pub fn submit_review(&self, draft: &Draft, event: ReviewEvent) -> Result<()> {
        let body = build_review_request(draft, event)?;
        let endpoint = format!("repos/{}/pulls/{}/reviews", draft.repo, draft.pr_number);

        let mut child = Command::new(&self.bin)
            .args(["api", "--method", "POST", &endpoint, "--input", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("gh api の起動に失敗しました")?;

        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(serde_json::to_string(&body)?.as_bytes())?;

        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!(
                "レビューの提出に失敗しました: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }
}

/// herdr はプラグインに最小限の PATH しか渡さないため、よくある置き場も探す。
/// 環境変数の書き換え（edition 2024 では unsafe）を避けるため、絶対パスを解決して使う
fn find_gh() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let extra = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{home}/.local/bin"),
        format!("{home}/.local/share/mise/shims"),
    ];
    let path_dirs = std::env::var("PATH").unwrap_or_default();
    path_dirs
        .split(':')
        .map(str::to_string)
        .chain(extra)
        .map(|d| PathBuf::from(d).join("gh"))
        .find(|p| p.is_file())
}
```

`src/main.rs` に `mod gh;` を追加する。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全32件PASS

- [ ] **Step 5: 実物のghで疎通を確認する**

一時的に `src/main.rs` を書き換えて確認する。

```rust
fn main() -> anyhow::Result<()> {
    let gh = gh::Gh::new()?;
    let detail = gh.pr_detail(Some("cli/cli"), 14148)?;
    println!("{} {} by {}", detail.number, detail.title, detail.author);
    println!("head={} repo={}", detail.head_sha, detail.repo);
    let raw = gh.pr_diff(&detail.repo, detail.number)?;
    let parsed = diff::parse(&raw);
    println!("{} files", parsed.files.len());
    Ok(())
}
```

Run: `cargo run`
Expected: PR番号・タイトル・head SHA・ファイル数が出力される。

- [ ] **Step 6: コミット**

```bash
git add src/gh.rs src/main.rs
git commit -m "feat(gh): ghコマンド経由のGitHub連携を追加"
```

---

### Task 8: app.rs — 表示行の平坦化とカーソル

全ファイルのdiffを1本のスクロールにするため、「画面に出る行」の一次元配列を作る。描画とカーソル移動はこの配列だけを見る。

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `diff::{ParsedDiff, CommentTarget, DiffLine}`, `review::Draft`
- Produces: `Row`, `App`, `App::{new, rebuild_rows, move_cursor, next_file, prev_file, toggle_collapse, cursor_target, cursor_file_idx}`

- [ ] **Step 1: 失敗するテストを書く**

```rust
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
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`unresolved module 'app'`

- [ ] **Step 3: 実装する**

`src/app.rs`:

```rust
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
```

`src/main.rs` に `mod app;` を追加する。`next_file` が `self.cursor + 1` でスライスするため、`cursor` が最終要素のときに範囲外にならないことを確認する（`rows[len..]` は空スライスとして有効）。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全38件PASS

- [ ] **Step 5: コミット**

```bash
git add src/app.rs src/main.rs
git commit -m "feat(app): diff表示行の平坦化とカーソル操作を追加"
```

---

### Task 9: diff閲覧画面を動かす

ここで初めて画面が出る。`--pr` 指定でPRを開いて読めるところまで作る。

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/diffview.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `app::{App, Row}`, `gh::Gh`
- Produces: `ui::diffview::render(&mut App, &PrDetail, &mut Frame)`

- [ ] **Step 1: 描画を実装する**

`src/ui/mod.rs`:

```rust
pub mod diffview;
```

`src/ui/diffview.rs`:

```rust
use crate::app::{App, Row};
use crate::diff::LineKind;
use crate::gh::PrDetail;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(app: &mut App, pr: &PrDetail, frame: &mut Frame) {
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(pr, frame, areas[0]);
    render_body(app, frame, areas[1]);
    render_status(app, frame, areas[2]);
}

fn render_header(pr: &PrDetail, frame: &mut Frame, area: Rect) {
    let text = format!(
        " #{} {}   {}  +{} -{} ",
        pr.number, pr.title, pr.author, pr.additions, pr.deletions
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let text = app.status.clone().unwrap_or_else(|| {
        format!(
            " j/k:移動 }}:次ファイル c:コメント S:提出 q:戻る ?:ヘルプ    comments: {} ",
            app.draft.comments.len()
        )
    });
    frame.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn render_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    app.scroll = clamp_scroll(app.scroll, app.cursor, height);

    // app.rows の走査と row_to_line の両方が app を借りるため、ここで共有参照に落とす
    let app: &App = app;
    let lines: Vec<Line> = app
        .rows
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(height)
        .map(|(i, row)| row_to_line(app, row, i == app.cursor))
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// カーソルが画面外に出ないところまでだけスクロールを動かす
fn clamp_scroll(scroll: usize, cursor: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if cursor < scroll {
        return cursor;
    }
    if cursor >= scroll + height {
        return cursor + 1 - height;
    }
    scroll
}

fn row_to_line<'a>(app: &'a App, row: &'a Row, is_cursor: bool) -> Line<'a> {
    let base = match row {
        Row::FileHeader { file_idx } => {
            let file = &app.diff.files[*file_idx];
            let marker = if app.collapsed.contains(file_idx) { "▶" } else { "▼" };
            let extra = if file.is_binary {
                "  (binary)".to_string()
            } else if file.is_rename() {
                format!("  (renamed from {})", file.old_path.as_deref().unwrap_or(""))
            } else {
                String::new()
            };
            Line::from(vec![Span::styled(
                format!(
                    "{marker} {}  +{} -{}{extra}",
                    file.display_path(),
                    file.additions(),
                    file.deletions()
                ),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )])
        }
        Row::HunkHeader { file_idx, hunk_idx } => Line::from(Span::styled(
            app.diff.files[*file_idx].hunks[*hunk_idx].header.clone(),
            Style::default().fg(Color::DarkGray),
        )),
        Row::Line { file_idx, hunk_idx, line_idx } => {
            let line = &app.diff.files[*file_idx].hunks[*hunk_idx].lines[*line_idx];
            let (marker, color) = match line.kind {
                LineKind::Added => ('+', Color::Green),
                LineKind::Removed => ('-', Color::Red),
                LineKind::Context => (' ', Color::Reset),
            };
            let number = line
                .new_lineno
                .or(line.old_lineno)
                .map(|n| format!("{n:>5}"))
                .unwrap_or_else(|| "     ".to_string());
            Line::from(vec![
                Span::styled(format!("{number} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{marker}{}", line.text), Style::default().fg(color)),
            ])
        }
        Row::Comment { body_line, .. } => Line::from(Span::styled(
            format!("      💬 {body_line}"),
            Style::default().fg(Color::Yellow),
        )),
    };

    if is_cursor {
        base.style(Style::default().add_modifier(Modifier::REVERSED))
    } else {
        base
    }
}
```

- [ ] **Step 2: スクロール計算のテストを書く**

`src/ui/diffview.rs` の末尾に置く。

```rust
#[cfg(test)]
mod tests {
    use super::clamp_scroll;

    #[test]
    fn keeps_scroll_when_cursor_is_visible() {
        assert_eq!(clamp_scroll(10, 15, 20), 10);
    }

    #[test]
    fn scrolls_up_when_cursor_is_above() {
        assert_eq!(clamp_scroll(10, 3, 20), 3);
    }

    #[test]
    fn scrolls_down_just_enough_when_cursor_is_below() {
        assert_eq!(clamp_scroll(0, 25, 20), 6);
    }

    #[test]
    fn handles_zero_height() {
        assert_eq!(clamp_scroll(5, 5, 0), 0);
    }
}
```

- [ ] **Step 3: テストを実行して確認する**

Run: `cargo test`
Expected: 全42件PASS

- [ ] **Step 4: main.rsに起動処理とキー操作を実装する**

```rust
mod app;
mod diff;
mod gh;
mod review;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gh::{Gh, PrDetail};

fn main() -> Result<()> {
    let gh = Gh::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (repo, number) = match args.as_slice() {
        [flag, n] if flag == "--pr" => (None, n.parse()?),
        _ => anyhow::bail!("usage: herdr-gh-review --pr <number>"),
    };

    let pr = gh.pr_detail(repo, number)?;
    let parsed = diff::parse(&gh.pr_diff(&pr.repo, pr.number)?);
    let state = review::state_dir();
    let draft = review::load(&state, &pr.repo, pr.number)?
        .unwrap_or_else(|| review::Draft::new(&pr.repo, pr.number, &pr.head_sha));
    let mut app = App::new(parsed, draft);

    let mut terminal = ratatui::init();
    let result = run_diff_view(&mut terminal, &mut app, &pr);
    ratatui::restore();
    result
}

fn run_diff_view(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    pr: &PrDetail,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::diffview::render(app, pr, f))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if handle_key(app, key) {
            return Ok(());
        }
    }
}

/// 戻り値が true なら画面を抜ける
fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let half_page = 15;
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return true,
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => app.move_cursor(1),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => app.move_cursor(-1),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.move_cursor(half_page),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.move_cursor(-half_page),
        (KeyCode::Char('g'), _) => app.cursor = 0,
        (KeyCode::Char('G'), _) => app.cursor = app.rows.len().saturating_sub(1),
        (KeyCode::Char('}'), _) => app.next_file(),
        (KeyCode::Char('{'), _) => app.prev_file(),
        (KeyCode::Tab, _) => app.toggle_collapse(),
        _ => {}
    }
    false
}
```

- [ ] **Step 5: 実際に動かして確認する**

```bash
cargo run -- --pr 14148
```

`gh` のデフォルトリポジトリが無いとエラーになるため、確認は `cli/cli` のクローンか、`--repo` 対応を待たずに一時的に `pr_detail(Some("cli/cli"), number)` を渡して行う。

確認項目:
- ファイルヘッダ・ハンクヘッダ・追加行（緑）・削除行（赤）が出ること
- `j`/`k` でカーソルが動き、画面端でスクロールすること
- `}` で次のファイルへ飛ぶこと
- `Tab` でファイルが畳まれ、もう一度押すと開くこと
- `q` で端末が壊れずに戻ること

- [ ] **Step 6: コミット**

```bash
git add src/ui src/main.rs
git commit -m "feat(ui): diff閲覧画面とキー操作を追加"
```

---

### Task 10: PR一覧画面と起動対象の解決

**Files:**
- Create: `src/ui/prlist.rs`, `src/target.rs`
- Modify: `src/main.rs`, `src/ui/mod.rs`

**Interfaces:**
- Produces: `Target`, `Target::resolve(args: &[String], env: Option<&str>) -> Result<Target>`, `ui::prlist::render(&[PrSummary], usize, &str, &mut Frame)`

- [ ] **Step 1: Target解決の失敗するテストを書く**

`src/target.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_to_repo_pr_list() {
        assert_eq!(Target::resolve(&[], None).unwrap(), Target::RepoPrList);
    }

    #[test]
    fn env_selects_review_requested() {
        assert_eq!(
            Target::resolve(&[], Some("review-requested")).unwrap(),
            Target::ReviewRequested
        );
    }

    #[test]
    fn env_accepts_a_pr_url() {
        assert_eq!(
            Target::resolve(&[], Some("https://github.com/cli/cli/pull/14148")).unwrap(),
            Target::Pr { repo: Some("cli/cli".into()), number: 14148 }
        );
    }

    #[test]
    fn args_take_precedence_over_env() {
        assert_eq!(
            Target::resolve(&args(["--review-requested"].as_slice()), Some("https://github.com/cli/cli/pull/1")).unwrap(),
            Target::ReviewRequested
        );
    }

    #[test]
    fn pr_number_flag_has_no_repo() {
        assert_eq!(
            Target::resolve(&args(["--pr", "42"].as_slice()), None).unwrap(),
            Target::Pr { repo: None, number: 42 }
        );
    }

    #[test]
    fn unknown_env_value_falls_back_to_repo_list() {
        assert_eq!(Target::resolve(&[], Some("garbage")).unwrap(), Target::RepoPrList);
    }

    #[test]
    fn bad_pr_number_is_an_error() {
        assert!(Target::resolve(&args(["--pr", "abc"].as_slice()), None).is_err());
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`unresolved module 'target'`

- [ ] **Step 3: Targetを実装する**

```rust
use crate::gh::repo_from_url;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    RepoPrList,
    ReviewRequested,
    Pr { repo: Option<String>, number: u32 },
}

impl Target {
    pub fn resolve(args: &[String], env: Option<&str>) -> Result<Self> {
        match args {
            [] => {}
            [flag] if flag == "--review-requested" => return Ok(Target::ReviewRequested),
            [flag, value] if flag == "--pr" => {
                let number = value
                    .parse()
                    .map_err(|_| anyhow!("PR番号として解釈できません: {value}"))?;
                return Ok(Target::Pr { repo: None, number });
            }
            [flag, value] if flag == "--url" => return Self::from_url(value),
            _ => return Err(anyhow!(
                "usage: herdr-gh-review [--review-requested | --pr <number> | --url <url>]"
            )),
        }

        match env {
            None => Ok(Target::RepoPrList),
            Some("review-requested") => Ok(Target::ReviewRequested),
            Some(value) if value.contains("/pull/") => Self::from_url(value),
            // 想定外の値で起動を止めるより、既定の一覧を出す方が使う側の損失が小さい
            Some(_) => Ok(Target::RepoPrList),
        }
    }

    fn from_url(url: &str) -> Result<Self> {
        let (repo, number) =
            repo_from_url(url).ok_or_else(|| anyhow!("PRのURLとして解釈できません: {url}"))?;
        Ok(Target::Pr { repo: Some(repo), number })
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全49件PASS

- [ ] **Step 5: PR一覧画面を実装する**

`src/ui/prlist.rs`:

```rust
use crate::gh::PrSummary;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(prs: &[PrSummary], cursor: usize, title: &str, frame: &mut Frame) {
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    frame.render_widget(
        Paragraph::new(format!(" {title} ({}) ", prs.len()))
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        areas[0],
    );

    if prs.is_empty() {
        frame.render_widget(
            Paragraph::new("  該当するPRがありません")
                .style(Style::default().fg(Color::DarkGray)),
            areas[1],
        );
    } else {
        let lines: Vec<Line> = prs
            .iter()
            .enumerate()
            .map(|(i, pr)| {
                let repo = pr.repo.as_deref().map(|r| format!("{r} ")).unwrap_or_default();
                let counts = match (pr.additions, pr.deletions) {
                    (Some(a), Some(d)) => format!("  +{a} -{d}"),
                    _ => String::new(),
                };
                let draft = if pr.is_draft { " [draft]" } else { "" };
                let line = Line::from(vec![
                    Span::styled(format!(" {repo}#{:<6}", pr.number), Style::default().fg(Color::Cyan)),
                    Span::raw(pr.title.clone()),
                    Span::styled(format!("  @{}{counts}{draft}", pr.author), Style::default().fg(Color::DarkGray)),
                ]);
                if i == cursor {
                    line.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    line
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), areas[1]);
    }

    frame.render_widget(
        Paragraph::new(" j/k:移動 Enter:開く r:再読み込み q:終了 ")
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        areas[2],
    );
}
```

`src/ui/mod.rs` に `pub mod prlist;` を追加する。

- [ ] **Step 6: main.rsに画面遷移を組む**

```rust
mod app;
mod diff;
mod gh;
mod review;
mod target;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gh::{Gh, PrDetail, PrSummary};
use target::Target;

fn main() -> Result<()> {
    let gh = Gh::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = std::env::var("GH_REVIEW_TARGET").ok();
    let target = Target::resolve(&args, env.as_deref())?;

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &gh, target);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, gh: &Gh, target: Target) -> Result<()> {
    match target {
        Target::Pr { repo, number } => open_pr(terminal, gh, repo.as_deref(), number),
        Target::RepoPrList => run_pr_list(terminal, gh, false),
        Target::ReviewRequested => run_pr_list(terminal, gh, true),
    }
}

fn run_pr_list(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    review_requested: bool,
) -> Result<()> {
    let (title, fetch): (&str, fn(&Gh) -> Result<Vec<PrSummary>>) = if review_requested {
        ("PRs awaiting my review", Gh::search_review_requested)
    } else {
        ("Open pull requests", Gh::list_prs)
    };

    let mut prs = fetch(gh)?;
    let mut cursor = 0usize;

    loop {
        terminal.draw(|f| ui::prlist::render(&prs, cursor, title, f))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('j') | KeyCode::Down => {
                cursor = (cursor + 1).min(prs.len().saturating_sub(1))
            }
            KeyCode::Char('k') | KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Char('r') => {
                prs = fetch(gh)?;
                cursor = 0;
            }
            KeyCode::Enter => {
                if let Some(pr) = prs.get(cursor) {
                    open_pr(terminal, gh, pr.repo.as_deref(), pr.number)?;
                }
            }
            _ => {}
        }
    }
}

fn open_pr(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    repo: Option<&str>,
    number: u32,
) -> Result<()> {
    let pr = gh.pr_detail(repo, number)?;
    let parsed = diff::parse(&gh.pr_diff(&pr.repo, pr.number)?);
    let state = review::state_dir();
    let draft = review::load(&state, &pr.repo, pr.number)?
        .unwrap_or_else(|| review::Draft::new(&pr.repo, pr.number, &pr.head_sha));

    let mut app = App::new(parsed, draft);
    if app.draft.head_sha != pr.head_sha {
        app.status = Some(
            " 保存されていた下書きは古いコミットのものです。行の位置を確認してください ".into(),
        );
    }
    run_diff_view(terminal, &mut app, &pr)
}
```

`run_diff_view` と `handle_key` は Task 9 のものをそのまま残す。

- [ ] **Step 7: 実際に動かして確認する**

```bash
cargo run                                                    # このリポジトリにPRは無いので空一覧
cargo run -- --review-requested                              # 自分宛のレビュー依頼
cargo run -- --url https://github.com/cli/cli/pull/14148     # URL直接
GH_REVIEW_TARGET=review-requested cargo run                  # 環境変数経由
```

確認項目:
- 一覧で `j`/`k` が動き、Enterでdiffが開き、`q` で一覧に戻ること
- PRが0件のとき「該当するPRがありません」が出て落ちないこと
- 一覧の `q` で終了し、端末が壊れないこと

- [ ] **Step 8: コミット**

```bash
git add src/target.rs src/ui/prlist.rs src/ui/mod.rs src/main.rs
git commit -m "feat(ui): PR一覧画面と起動対象の解決を追加"
```

---

### Task 11: $EDITORによる行コメントの入力

**Files:**
- Create: `src/editor.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `editor::edit_text(initial: &str) -> Result<Option<String>>`（`None` は「入力なし」）

- [ ] **Step 1: エディタコマンド決定の失敗するテストを書く**

`src/editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::editor_command;

    #[test]
    fn prefers_editor_env() {
        assert_eq!(editor_command(Some("nvim"), Some("emacs")), vec!["nvim"]);
    }

    #[test]
    fn falls_back_to_visual_then_vi() {
        assert_eq!(editor_command(None, Some("emacs")), vec!["emacs"]);
        assert_eq!(editor_command(None, None), vec!["vi"]);
    }

    #[test]
    fn splits_editor_with_arguments() {
        assert_eq!(editor_command(Some("code -w"), None), vec!["code", "-w"]);
    }

    #[test]
    fn ignores_empty_editor_value() {
        assert_eq!(editor_command(Some("  "), None), vec!["vi"]);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cargo test`
Expected: コンパイルエラー。`unresolved module 'editor'`

- [ ] **Step 3: 実装する**

```rust
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::Command;

fn editor_command(editor: Option<&str>, visual: Option<&str>) -> Vec<String> {
    let raw = editor
        .filter(|s| !s.trim().is_empty())
        .or(visual.filter(|s| !s.trim().is_empty()))
        .unwrap_or("vi");
    raw.split_whitespace().map(str::to_string).collect()
}

/// エディタを開いてテキストを受け取る。空のまま閉じた場合は None を返す
pub fn edit_text(initial: &str) -> Result<Option<String>> {
    let editor = std::env::var("EDITOR").ok();
    let visual = std::env::var("VISUAL").ok();
    let cmd = editor_command(editor.as_deref(), visual.as_deref());

    let mut file = tempfile::Builder::new()
        .prefix("gh-review-comment-")
        .suffix(".md")
        .tempfile()
        .context("一時ファイルを作れません")?;
    file.write_all(initial.as_bytes())?;
    file.flush()?;
    let path = file.path().to_path_buf();

    let status = Command::new(&cmd[0])
        .args(&cmd[1..])
        .arg(&path)
        .status()
        .with_context(|| format!("エディタを起動できません: {}", cmd.join(" ")))?;

    if !status.success() {
        bail!("エディタが異常終了しました: {}", cmd.join(" "));
    }

    let text = std::fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(text.trim_end().to_string()))
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test`
Expected: 全53件PASS

- [ ] **Step 5: main.rsからエディタを呼ぶ**

`handle_key` はエディタ起動や保存で失敗しうるようになるため、シグネチャを変える。`ratatui::init()` が返す端末を作り直すため、端末そのものを渡す。

```rust
enum KeyOutcome {
    Continue,
    Leave,
}

fn run_diff_view(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    pr: &PrDetail,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::diffview::render(app, pr, f))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match handle_key(terminal, app, key)? {
            KeyOutcome::Leave => return Ok(()),
            KeyOutcome::Continue => {}
        }
    }
}

fn handle_key(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    key: KeyEvent,
) -> Result<KeyOutcome> {
    let half_page = 15;
    app.status = None;

    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return Ok(KeyOutcome::Leave),
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => app.move_cursor(1),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => app.move_cursor(-1),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.move_cursor(half_page),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.move_cursor(-half_page),
        (KeyCode::Char('g'), _) => app.cursor = 0,
        (KeyCode::Char('G'), _) => app.cursor = app.rows.len().saturating_sub(1),
        (KeyCode::Char('}'), _) => app.next_file(),
        (KeyCode::Char('{'), _) => app.prev_file(),
        (KeyCode::Tab, _) => app.toggle_collapse(),
        (KeyCode::Char('c'), _) => comment_on_cursor(terminal, app)?,
        (KeyCode::Char('d'), _) => delete_comment_on_cursor(app)?,
        (KeyCode::Char('e'), _) => edit_review_body(terminal, app)?,
        _ => {}
    }
    Ok(KeyOutcome::Continue)
}

/// 端末を一度畳んでエディタに渡し、戻ってきたら組み立て直す
fn with_editor<T>(
    terminal: &mut ratatui::DefaultTerminal,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ratatui::restore();
    let result = f();
    *terminal = ratatui::init();
    terminal.clear()?;
    result
}

fn comment_on_cursor(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let Some(target) = app.cursor_target() else {
        app.status = Some(" この行にはコメントできません ".into());
        return Ok(());
    };
    let initial = app
        .draft
        .comment_at(&target)
        .map(|c| c.body.clone())
        .unwrap_or_default();

    let Some(body) = with_editor(terminal, || editor::edit_text(&initial))? else {
        app.status = Some(" コメントは空だったので破棄しました ".into());
        return Ok(());
    };

    app.draft.upsert_comment(target, body);
    review::save(&review::state_dir(), &app.draft)?;
    app.rebuild_rows();
    Ok(())
}

fn delete_comment_on_cursor(app: &mut App) -> Result<()> {
    let target = match app.rows.get(app.cursor) {
        Some(app::Row::Comment { path, line, side, .. }) => crate::diff::CommentTarget {
            path: path.clone(),
            line: *line,
            side: *side,
        },
        _ => match app.cursor_target() {
            Some(t) => t,
            None => return Ok(()),
        },
    };
    app.draft.remove_comment(&target);
    review::save(&review::state_dir(), &app.draft)?;
    app.rebuild_rows();
    Ok(())
}

fn edit_review_body(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let initial = app.draft.body.clone();
    let body = with_editor(terminal, || editor::edit_text(&initial))?.unwrap_or_default();
    app.draft.body = body;
    review::save(&review::state_dir(), &app.draft)?;
    Ok(())
}
```

`mod editor;` を `src/main.rs` に追加する。`Ctrl-d` と素の `d` は `KeyModifiers` で区別されるため、`match` の並び順（`CONTROL` 付きを先に書く）を崩さないこと。

- [ ] **Step 6: 実際に動かして確認する**

```bash
cargo run -- --url https://github.com/cli/cli/pull/14148
```

確認項目:
- 追加行で `c` → nvimが開く → 書いて `:wq` → コメントが行の直下に黄色で出ること
- 同じ行でもう一度 `c` → 既存の本文が初期値として入っていること
- 何も書かずに `:wq` → 「空だったので破棄しました」が出ること
- コメント行にカーソルを合わせて `d` → 消えること
- `e` でレビュー全体コメントが書けること
- アプリを `q` で抜けて再度開く → コメントが残っていること
- 削除行（赤）で `c` → コメントできること
- ファイルヘッダ行で `c` → 「この行にはコメントできません」が出ること

- [ ] **Step 7: コミット**

```bash
git add src/editor.rs src/main.rs
git commit -m "feat(editor): \$EDITORによる行コメント入力を追加"
```

---

### Task 12: 提出ダイアログとGitHubへの送信

**Files:**
- Create: `src/ui/submit.rs`
- Modify: `src/main.rs`, `src/ui/mod.rs`

**Interfaces:**
- Consumes: `review::{ReviewEvent, build_review_request}`, `gh::Gh::submit_review`
- Produces: `ui::submit::render(&Draft, usize, &mut Frame)`

- [ ] **Step 1: 提出ダイアログを実装する**

`src/ui/submit.rs`:

```rust
use crate::review::{Draft, ReviewEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub const EVENTS: [ReviewEvent; 3] = [
    ReviewEvent::Comment,
    ReviewEvent::Approve,
    ReviewEvent::RequestChanges,
];

pub fn render(draft: &Draft, cursor: usize, frame: &mut Frame) {
    let [area] = Layout::horizontal([Constraint::Length(56)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(10)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(format!(
            "  行コメント {} 件 / 全体コメント {}",
            draft.comments.len(),
            if draft.body.trim().is_empty() { "なし" } else { "あり" }
        )),
        Line::from(""),
    ];
    for (i, event) in EVENTS.iter().enumerate() {
        let text = format!("  {}  ", event.label());
        lines.push(if i == cursor {
            Line::from(Span::styled(text, Style::default().add_modifier(Modifier::REVERSED)))
        } else {
            Line::from(text)
        });
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  j/k:選択 Enter:提出 Esc:やめる",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Submit review ")),
        area,
    );
}
```

`src/ui/mod.rs` に `pub mod submit;` を追加する。

- [ ] **Step 2: main.rsに提出処理をつなぐ**

`gh` を使う場所が増えるため、呼び出しの連鎖に `gh: &Gh` を通す。変更後のシグネチャは次のとおり。

```rust
fn open_pr(terminal: &mut ratatui::DefaultTerminal, gh: &Gh, repo: Option<&str>, number: u32) -> Result<()>
fn run_diff_view(terminal: &mut ratatui::DefaultTerminal, gh: &Gh, app: &mut App, pr: &PrDetail) -> Result<()>
fn handle_key(terminal: &mut ratatui::DefaultTerminal, gh: &Gh, app: &mut App, key: KeyEvent) -> Result<KeyOutcome>
```

`open_pr` 内の `run_diff_view(terminal, &mut app, &pr)` を `run_diff_view(terminal, gh, &mut app, &pr)`
に、`run_diff_view` 内の `handle_key(terminal, app, key)?` を `handle_key(terminal, gh, app, key)?` に直す。

`handle_key` の `match` に `S` を足す。

```rust
        (KeyCode::Char('S'), _) => submit(terminal, app, gh)?,
```

```rust
fn submit(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    gh: &Gh,
) -> Result<()> {
    let mut cursor = 0usize;

    loop {
        terminal.draw(|f| ui::submit::render(&app.draft, cursor, f))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('j') | KeyCode::Down => {
                cursor = (cursor + 1).min(ui::submit::EVENTS.len() - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Enter => {
                let event = ui::submit::EVENTS[cursor];
                match gh.submit_review(&app.draft, event) {
                    Ok(()) => {
                        // 送信できたときだけ下書きを消す
                        review::delete(&review::state_dir(), &app.draft.repo, app.draft.pr_number)?;
                        app.draft.comments.clear();
                        app.draft.body.clear();
                        app.rebuild_rows();
                        app.status = Some(format!(" {} で提出しました ", event.label()));
                        return Ok(());
                    }
                    Err(e) => {
                        // 下書きは残したまま、原因だけ伝える
                        app.status = Some(format!(" {} ", first_line(&e.to_string())));
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or(message).to_string()
}
```

- [ ] **Step 3: ビルドとテストが通ることを確認する**

Run: `cargo test && cargo build`
Expected: 全53件PASS、ビルド成功

- [ ] **Step 4: 自分のリポジトリで実際に提出して確認する**

**他人のリポジトリで試さないこと。** 自分のGitHubアカウントに検証用のリポジトリとPRを作る。

```bash
gh repo create gh-review-sandbox --private --clone
cd gh-review-sandbox
git commit --allow-empty -m "chore: 初期コミット" && git push -u origin main
git checkout -b test-pr
printf 'line1\nline2\nline3\n' > sample.txt && git add . && git commit -m "test: サンプルを追加"
git push -u origin test-pr
gh pr create --title "テスト用PR" --body "動作確認用"
```

サンドボックスのディレクトリから `herdr-gh-review` を実行する。

確認項目:
- 追加行にコメントを2件付けて `S` → `Comment` → 「提出しました」が出ること
- GitHub上で該当行にコメントが付いていること（`gh pr view --web`）
- 提出後、下書きファイルが消えていること（`ls ~/.local/state/herdr/plugins/k-narusawa.gh-review/drafts`）
- 全体コメント無しで `Request changes` → 「全体コメントが必要です」が出て、**下書きが残っていること**
- 削除行（`side: LEFT`）へのコメントも通ること

- [ ] **Step 5: コミット**

```bash
git add src/ui/submit.rs src/ui/mod.rs src/main.rs
git commit -m "feat(review): レビュー提出ダイアログとGitHubへの送信を追加"
```

---

### Task 13: 残りのキー操作とエラー表示

仕様書に挙げた `o` / `r` / `?` と、取得失敗時にアプリを落とさない扱いを入れる。

**Files:**
- Create: `src/ui/help.rs`
- Modify: `src/main.rs`, `src/ui/mod.rs`, `src/gh.rs`

**Interfaces:**
- Produces: `ui::help::render(&mut Frame)`, `gh::log_error(&anyhow::Error)`

- [ ] **Step 1: ヘルプ画面を実装する**

`src/ui/help.rs`:

```rust
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const KEYS: &[(&str, &str)] = &[
    ("j / k", "移動"),
    ("Ctrl-d / Ctrl-u", "半画面移動"),
    ("g / G", "先頭 / 末尾"),
    ("} / {", "次 / 前のファイル"),
    ("Tab", "ファイルの折りたたみ"),
    ("c", "カーソル行にコメント"),
    ("d", "コメントを削除"),
    ("e", "レビュー全体コメントを編集"),
    ("S", "提出"),
    ("o", "ブラウザで開く"),
    ("r", "再読み込み"),
    ("q", "戻る"),
];

pub fn render(frame: &mut Frame) {
    let [area] = Layout::horizontal([Constraint::Length(48)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(KEYS.len() as u16 + 4)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|(key, desc)| Line::from(format!("  {key:<18}{desc}")))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from("  何かキーを押すと戻ります").style(Style::default().fg(Color::DarkGray)));

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}
```

`src/ui/mod.rs` に `pub mod help;` を追加する。

- [ ] **Step 2: エラーログを実装する**

`src/gh.rs` に追加する。ステータス行は1行しか出せないため、全文はファイルに残す。

```rust
/// ステータス行には1行しか出せないので、原因の全文はここに残す
pub fn log_error(error: &anyhow::Error) {
    let path = crate::review::state_dir().join("log");
    let Some(parent) = path.parent() else {
        return;
    };
    let _ = std::fs::create_dir_all(parent);
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(file, "{error:?}");
}
```

- [ ] **Step 3: main.rsに残りのキーを足す**

`handle_key` に追加する。

```rust
        (KeyCode::Char('o'), _) => {
            if let Err(e) = gh.open_in_browser(&app.draft.repo, app.draft.pr_number) {
                gh::log_error(&e);
                app.status = Some(format!(" {} ", first_line(&e.to_string())));
            }
        }
        (KeyCode::Char('r'), _) => reload(app, gh, pr)?,
        (KeyCode::Char('?'), _) => {
            terminal.draw(|f| ui::help::render(f))?;
            let _ = event::read()?;
        }
```

`handle_key` には `pr: &PrDetail` も渡す必要がある。シグネチャは次のようになる。

```rust
fn handle_key(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    app: &mut App,
    pr: &PrDetail,
    key: KeyEvent,
) -> Result<KeyOutcome>
```

再読み込みは、書きかけの下書きを保ったままdiffだけを取り直す。

```rust
/// 下書きは残したままdiffだけ取り直す。失敗しても今の画面は壊さない
fn reload(app: &mut App, gh: &Gh, pr: &PrDetail) -> Result<()> {
    match gh.pr_diff(&pr.repo, pr.number) {
        Ok(raw) => {
            app.diff = diff::parse(&raw);
            app.rebuild_rows();
            app.status = Some(" 再読み込みしました ".into());
        }
        Err(e) => {
            gh::log_error(&e);
            app.status = Some(format!(" {} ", first_line(&e.to_string())));
        }
    }
    Ok(())
}
```

- [ ] **Step 4: PR一覧の取得失敗でアプリを落とさない**

Task 10 の `run_pr_list` は `fetch(gh)?` で終了してしまう。取得失敗を画面に出して `r` で
やり直せるようにする。

```rust
fn run_pr_list(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    review_requested: bool,
) -> Result<()> {
    let (title, fetch): (&str, fn(&Gh) -> Result<Vec<PrSummary>>) = if review_requested {
        ("PRs awaiting my review", Gh::search_review_requested)
    } else {
        ("Open pull requests", Gh::list_prs)
    };

    let mut prs = Vec::new();
    let mut cursor = 0usize;
    let mut status: Option<String> = None;

    match fetch(gh) {
        Ok(v) => prs = v,
        Err(e) => {
            gh::log_error(&e);
            status = Some(fetch_error_message(&e, review_requested));
        }
    }

    loop {
        terminal.draw(|f| ui::prlist::render(&prs, cursor, title, status.as_deref(), f))?;
        // 以降のキー処理は Task 10 のまま。'r' の分岐だけ次に置き換える
    }
}
```

`r` の分岐:

```rust
            KeyCode::Char('r') => {
                status = None;
                match fetch(gh) {
                    Ok(v) => {
                        prs = v;
                        cursor = 0;
                    }
                    Err(e) => {
                        gh::log_error(&e);
                        status = Some(fetch_error_message(&e, review_requested));
                    }
                }
            }
```

カレントディレクトリがGitHubリポジトリでない場合が最も多いので、そのときだけ次の手を添える。

```rust
fn fetch_error_message(error: &anyhow::Error, review_requested: bool) -> String {
    let message = first_line(&error.to_string());
    if review_requested {
        return message;
    }
    format!("{message}（--review-requested なら任意の場所から使えます）")
}
```

`Enter` の分岐も、PRを開けなかったときに落ちないようにする。

```rust
            KeyCode::Enter => {
                if let Some(pr) = prs.get(cursor) {
                    if let Err(e) = open_pr(terminal, gh, pr.repo.as_deref(), pr.number) {
                        gh::log_error(&e);
                        status = Some(first_line(&e.to_string()));
                    }
                }
            }
```

`ui::prlist::render` に引数を1つ足す。

```rust
pub fn render(prs: &[PrSummary], cursor: usize, title: &str, status: Option<&str>, frame: &mut Frame) {
```

本文の空表示と最下行を、`status` があればそちらを優先するように変える。

```rust
    if let Some(message) = status {
        frame.render_widget(
            Paragraph::new(format!("  {message}")).style(Style::default().fg(Color::Red)),
            areas[1],
        );
    } else if prs.is_empty() {
```

- [ ] **Step 5: ビルドとテストが通ることを確認する**

Run: `cargo test && cargo build`
Expected: 全53件PASS、警告なし

- [ ] **Step 6: 動作を確認する**

確認項目:
- diff画面で `?` を押すとキー一覧が出て、何かキーを押すと戻ること
- `o` でブラウザにPRが開くこと
- `r` でdiffが取り直され、書きかけのコメントが消えていないこと
- ネットワークを切って一覧を開くと、赤字でエラーが出るがアプリは落ちないこと
  （`networksetup -setairportpower en0 off` などで確認し、終わったら戻す）
- `~/.local/state/herdr/plugins/k-narusawa.gh-review/log` にエラーの全文が残っていること

- [ ] **Step 7: コミット**

```bash
git add src/ui/help.rs src/ui/mod.rs src/gh.rs src/main.rs
git commit -m "feat(ui): ヘルプ・再読み込み・ブラウザ起動とエラー表示を追加"
```

---

### Task 14: herdrプラグインとして組み込む

**Files:**
- Create: `herdr-plugin.toml`, `herdr/install.sh`, `herdr/pane.sh`, `README.md`

**Interfaces:**
- Consumes: ビルド済みの `herdr-gh-review` バイナリ

- [ ] **Step 1: マニフェストを書く**

`herdr-plugin.toml`:

```toml
id = "k-narusawa.gh-review"
name = "gh-review"
version = "0.1.0"
min_herdr_version = "0.7.5"
platforms = ["macos", "linux"]
description = "Review GitHub pull requests in a herdr pane."

[[build]]
command = ["bash", "herdr/install.sh"]

# ペインのcwdはレビュー対象のリポジトリであってプラグインルートではないため、絶対パスで起動する
[[panes]]
id = "pane"
title = "gh-review"
placement = "tab"
command = ["sh", "-c", "exec \"$HERDR_PLUGIN_ROOT/bin/herdr-gh-review\""]

[[actions]]
id = "open"
title = "gh-review: open PR list"
contexts = ["pane", "workspace"]
command = ["bash", "herdr/pane.sh", "open"]

[[actions]]
id = "review-requested"
title = "gh-review: PRs awaiting my review"
contexts = ["pane", "workspace"]
command = ["bash", "herdr/pane.sh", "review-requested"]

[[actions]]
id = "open-url"
title = "gh-review: open PR URL"
contexts = ["workspace"]
command = ["bash", "herdr/pane.sh", "open-url"]

[[link_handlers]]
id = "pr-url"
title = "Open in gh-review"
pattern = "^https://github\\.com/[^/]+/[^/]+/pull/[0-9]+"
action = "open-url"
```

- [ ] **Step 2: install.shを書く**

`herdr/install.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# ponytail: ソースからビルドする（Rustツールチェーンが要る）。
# 他人に配るなら reviewr のように Releases のビルド済みバイナリを取りに行く形へ移す
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo が見つかりません。https://rustup.rs/ でRustを入れてください" >&2
  exit 1
fi

cargo build --release
mkdir -p bin
cp target/release/herdr-gh-review bin/
echo "installed: $(pwd)/bin/herdr-gh-review"
```

- [ ] **Step 3: pane.shを書く**

`herdr/pane.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
PLUGIN_ID="k-narusawa.gh-review"

mode="${1:-open}"

context="${HERDR_PLUGIN_CONTEXT_JSON:-{\}}"
cwd="$(printf '%s' "$context" | jq -r '.focused_pane_cwd // empty')"
: "${cwd:=$PWD}"

case "$mode" in
  open)             target="" ;;
  review-requested) target="review-requested" ;;
  open-url)
    # クリックされたURLは専用の環境変数で届く（公式browserプラグインの実装で確認済み）
    target="${HERDR_PLUGIN_CLICKED_URL:-}"
    if [ -z "$target" ]; then
      echo "PRのURLを受け取れませんでした" >&2
      exit 1
    fi
    ;;
  *)
    echo "usage: pane.sh {open|review-requested|open-url}" >&2
    exit 1
    ;;
esac

args=(plugin pane open --plugin "$PLUGIN_ID" --entrypoint pane --placement tab --cwd "$cwd")
if [ -n "$target" ]; then
  args+=(--env "GH_REVIEW_TARGET=$target")
fi

"$HERDR" "${args[@]}"
```

`open` と `review-requested` は `HERDR_PLUGIN_CONTEXT_JSON` の `focused_pane_cwd` を起点にする。
これは「ペインが起動されたときのcwd」であって現在のcwdではないため、ワークスペースを移動した直後は
意図しないリポジトリを見ることがある。実用上の問題が出たら `herdr pane list` の `foreground_cwd`
を読む方式に変える。

- [ ] **Step 4: ローカルリンクしてビルドする**

```bash
chmod +x herdr/install.sh herdr/pane.sh
bash herdr/install.sh
herdr plugin link /Users/narusawakohei/projects/herdr-gh-review
herdr plugin list | grep gh-review
```

Expected: `k-narusawa.gh-review` が一覧に出ること。

- [ ] **Step 5: herdr上で動作を確認する**

```bash
herdr plugin action invoke open --plugin k-narusawa.gh-review
herdr plugin action invoke review-requested --plugin k-narusawa.gh-review
herdr plugin log list --plugin k-narusawa.gh-review
```

確認項目:
- 新しいタブが開き、PR一覧が出ること
- `review-requested` で自分宛の一覧が出ること
- 端末に出したPR URLをクリックして「Open in gh-review」が出ること。ここで `pane.sh` が
  URLを拾えていなければ、`herdr plugin log list` で `HERDR_PLUGIN_CONTEXT_JSON` の中身を
  確認し、`jq` のパスを実際のキー名に合わせる
- **PATHが最小の状態で `gh` が見つかること**（`gh.rs` の `find_gh` が効いているか）。
  失敗する場合は `herdr plugin log list` にエラーが出る

- [ ] **Step 6: キーバインドを設定する**

`~/.config/herdr/config.toml` に追記する。

```toml
[[keys.command]]
key = "cmd+g"
type = "plugin_action"
command = "k-narusawa.gh-review.open"
```

```bash
herdr server reload-config
```

Expected: `cmd+g` でPR一覧が開くこと。

- [ ] **Step 7: READMEを書く**

`README.md`:

````markdown
# herdr-gh-review

GitHubのPull Requestを [herdr](https://herdr.dev) のペイン内でレビューするプラグイン。
diffを読み、行コメントを書き、レビューとして提出するところまでをターミナルで完結させる。

## 必要なもの

- herdr 0.7.5 以上
- [gh](https://cli.github.com/) 2.x（`gh auth login` 済みであること）
- Rust 1.97 以上（ビルドに必要）

## インストール

```bash
git clone https://github.com/k-narusawa/herdr-gh-review
herdr plugin link ./herdr-gh-review
bash ./herdr-gh-review/herdr/install.sh
```

## 使い方

| アクション | 内容 |
|---|---|
| `gh-review: open PR list` | カレントリポジトリのopen PR一覧 |
| `gh-review: PRs awaiting my review` | 自分がレビューを依頼されているPR |
| PR URLをクリック | そのPRを直接開く |

### キー操作

| キー | 動作 |
|---|---|
| `j` / `k` | 移動 |
| `Ctrl-d` / `Ctrl-u` | 半画面移動 |
| `g` / `G` | 先頭 / 末尾 |
| `}` / `{` | 次 / 前のファイル |
| `Tab` | ファイルの折りたたみ |
| `c` | カーソル行にコメント（`$EDITOR` が開く） |
| `d` | コメントを削除 |
| `e` | レビュー全体コメントを編集 |
| `S` | 提出（Comment / Approve / Request changes） |
| `q` | 戻る / 終了 |

書きかけのレビューは自動保存される。提出に成功したときだけ消える。
````

- [ ] **Step 8: コミット**

```bash
git add herdr-plugin.toml herdr/ README.md
git commit -m "feat(plugin): herdrプラグインとして登録できるようにする"
```

---

## 完了条件

すべてのタスクが終わった時点で、以下が満たされていること。

- [ ] `cargo test` が全件PASSする
- [ ] `cargo build --release` が警告なしで通る
- [ ] herdrから `cmd+g` でPR一覧が開く
- [ ] 自分宛のレビュー依頼一覧からPRを開き、行コメントを書いて提出できる
- [ ] 提出に失敗したとき、下書きが残っている
- [ ] ペインを閉じて開き直しても、書きかけの下書きが復元される

## v1に入れなかったもの

仕様書の「v1のスコープ」で意図的に外したもの。追加するとしたら次の順。

1. **既存の行コメントの表示** — 重複指摘を避けるため実用上いちばん効く
2. **AIエージェント連携** — `herdr agent list` でエージェントを選び、`herdr pane send-text` に
   下書きを整形して渡すアクションを追加する。`DraftComment` を変えずに済むよう設計してある
3. **シンタックスハイライト** — diffが読みづらいと感じたときに
4. **PR本文・会話スレッドの表示**
