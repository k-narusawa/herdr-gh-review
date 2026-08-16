use std::io::Write;
use std::process::{Command, Stdio};

/// 生diffを `delta --color-only` に通し、ANSI付きの行を「入力と1行=1行」で返す。
/// deltaが無い・失敗した・行数が変わった場合は `None`（呼び出し側は自前の色に落ちる）
pub fn colorize(raw: &str) -> Option<Vec<String>> {
    let mut child = Command::new("delta")
        .args([
            "--color-only",
            "--paging=never",
            "--keep-plus-minus-markers",
            "--width=variable",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // 大きなdiffだとstdoutのパイプが先に詰まるので、書き込みは別スレッドに逃がす
    let mut stdin = child.stdin.take()?;
    let owned = raw.to_string();
    std::thread::spawn(move || stdin.write_all(owned.as_bytes()));

    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    align(raw, &String::from_utf8(out.stdout).ok()?)
}

fn align(raw: &str, colored: &str) -> Option<Vec<String>> {
    let lines: Vec<String> = colored.lines().map(strip_erase).collect();
    (lines.len() == raw.lines().count()).then_some(lines)
}

/// deltaが行末に付ける「行末まで消去」。ratatuiには不要で、ansi-to-tuiも解釈しない
fn strip_erase(line: &str) -> String {
    line.replace("\x1b[0K", "").replace("\x1b[K", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_lines_when_the_count_matches() {
        let got = align("a\nb\nc\n", "A\nB\nC\n").unwrap();
        assert_eq!(got, ["A", "B", "C"]);
    }

    #[test]
    fn rejects_output_that_changed_the_line_count() {
        assert!(align("a\nb\n", "A\nB\nC\n").is_none());
        assert!(align("a\nb\n", "A\n").is_none());
    }

    #[test]
    fn drops_erase_in_line_sequences() {
        let got = align("x\n", "\x1b[32m+x\x1b[0m\x1b[0K\n").unwrap();
        assert_eq!(got[0], "\x1b[32m+x\x1b[0m");
    }

    /// deltaが入っている環境でだけ意味のあるテスト。無ければ`None`で落ちるのが正しい
    #[test]
    fn colorize_lines_up_with_the_input() {
        const DIFF: &str = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1,3 +1,3 @@
 fn main() {
-    let x = 1;
+    let x = 2;
 }
";
        let Some(lines) = colorize(DIFF) else {
            return;
        };
        assert_eq!(lines.len(), DIFF.lines().count());
        // 語単位の強調でANSIが挟まるので、素の文字列一致ではなく断片で見る
        assert!(lines[5].contains("let x = "), "削除行がずれている: {:?}", lines[5]);
        assert!(lines[5].contains('\x1b'), "色が付いていない: {:?}", lines[5]);
        assert!(lines[3].starts_with("@@"), "ハンクヘッダがずれている: {:?}", lines[3]);
    }
}
