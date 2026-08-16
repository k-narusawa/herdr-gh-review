use std::io::Write;
use std::process::{Command, Stdio};

/// Pipe the raw diff through `delta --color-only`, returning ANSI lines one-for-one with the
/// input. `None` if delta is missing, failed, or changed the line count — the caller then
/// falls back to its own colors
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

    // On a large diff the stdout pipe fills first, so the write goes to its own thread
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

/// delta's trailing erase-to-end-of-line. ratatui does not need it and ansi-to-tui ignores it
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

    /// Only meaningful where delta is installed; without it, bailing on `None` is correct
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
        // Word-level emphasis injects ANSI mid-line, so match fragments, not whole strings
        assert!(lines[5].contains("let x = "), "the removed line is off: {:?}", lines[5]);
        assert!(lines[5].contains('\x1b'), "no color was applied: {:?}", lines[5]);
        assert!(lines[3].starts_with("@@"), "the hunk header is off: {:?}", lines[3]);
    }
}
