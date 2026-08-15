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
