use crate::app::App;
use crate::diff::{CommentTarget, Side};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// What the agent in the AI pane writes. Only the fields we can act on
#[derive(Debug, Deserialize)]
pub struct AiReview {
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub comments: Vec<AiComment>,
}

#[derive(Debug, Deserialize)]
pub struct AiComment {
    pub path: String,
    pub line: u32,
    #[serde(default = "right")]
    pub side: Side,
    pub body: String,
}

fn right() -> Side {
    Side::Right
}

pub struct Merged {
    pub added: usize,
    /// Not on the current diff, or a comment is already there
    pub skipped: usize,
    /// One entry per skipped comment, `path:line SIDE`, for logging
    pub skipped_targets: Vec<String>,
}

pub fn review_path(state_dir: &Path, repo: &str, pr_number: u32) -> PathBuf {
    state_dir
        .join("ai")
        .join(format!("{}-{}.json", repo.replace('/', "-"), pr_number))
}

/// Where ai.sh records the panes it opened, one id per line, so they can be closed again
pub fn panes_path(state_dir: &Path, repo: &str, pr_number: u32) -> PathBuf {
    review_path(state_dir, repo, pr_number).with_extension("panes")
}

/// Read and remove what the AI pane left. `Ok(None)` when nothing is waiting.
/// A corrupt file is moved aside, so a later look does not trip over it again
pub fn take(state_dir: &Path, repo: &str, pr_number: u32) -> Result<Option<AiReview>> {
    let path = review_path(state_dir, repo, pr_number);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    match serde_json::from_str(&raw) {
        Ok(review) => {
            let _ = std::fs::remove_file(&path);
            Ok(Some(review))
        }
        Err(e) => {
            let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
            Err(anyhow!("the AI review was not valid JSON: {e}"))
        }
    }
}

/// A comment already on the target wins, whoever wrote it — the AI never overwrites
pub fn merge(app: &mut App, review: AiReview) -> Merged {
    let mut merged = Merged { added: 0, skipped: 0, skipped_targets: Vec::new() };

    for c in review.comments {
        let label = target_label(&c.path, c.line, c.side);
        if !app.line_in_diff(&c.path, c.line, c.side) {
            merged.skipped += 1;
            merged.skipped_targets.push(label);
            continue;
        }
        let target = CommentTarget { path: c.path, line: c.line, side: c.side };
        if app.draft.comment_at(&target).is_some() {
            merged.skipped += 1;
            merged.skipped_targets.push(label);
            continue;
        }
        app.draft.upsert_comment(target, c.body, true);
        merged.added += 1;
    }

    if app.draft.body.trim().is_empty() {
        app.draft.body = review.body;
    }
    merged
}

fn target_label(path: &str, line: u32, side: Side) -> String {
    let side = match side {
        Side::Right => "RIGHT",
        Side::Left => "LEFT",
    };
    format!("{path}:{line} {side}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::diff::CommentTarget;
    use crate::review::Draft;

    const DIFF: &str = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-one
+two
 three
";

    fn app() -> App {
        App::new(DIFF, Draft::new("k-narusawa/app", 42, "abc123"))
    }

    #[test]
    fn parses_a_review() {
        let review: AiReview = serde_json::from_str(
            r#"{"body":"summary","comments":[{"path":"a.rs","line":1,"side":"RIGHT","body":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(review.body, "summary");
        assert_eq!(review.comments[0].path, "a.rs");
        assert_eq!(review.comments[0].line, 1);
        assert_eq!(review.comments[0].side, Side::Right);
    }

    #[test]
    fn side_defaults_to_right() {
        let review: AiReview =
            serde_json::from_str(r#"{"comments":[{"path":"a.rs","line":1,"body":"x"}]}"#).unwrap();
        assert_eq!(review.comments[0].side, Side::Right);
        assert_eq!(review.body, "");
    }

    #[test]
    fn merges_a_comment_that_is_on_the_diff() {
        let mut app = app();
        let review: AiReview = serde_json::from_str(
            r#"{"body":"","comments":[{"path":"a.rs","line":1,"side":"RIGHT","body":"x"}]}"#,
        )
        .unwrap();

        let merged = merge(&mut app, review);

        assert_eq!((merged.added, merged.skipped), (1, 0));
        let t = CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right };
        assert!(app.draft.comment_at(&t).unwrap().ai);
    }

    #[test]
    fn skips_a_comment_that_is_not_on_the_diff() {
        let mut app = app();
        let review: AiReview = serde_json::from_str(
            r#"{"body":"","comments":[{"path":"a.rs","line":900,"side":"RIGHT","body":"x"}]}"#,
        )
        .unwrap();

        let merged = merge(&mut app, review);

        assert_eq!((merged.added, merged.skipped), (0, 1));
        assert_eq!(merged.skipped_targets, vec!["a.rs:900 RIGHT"]);
        assert!(app.draft.comments.is_empty());
    }

    #[test]
    fn never_overwrites_an_existing_comment() {
        let mut app = app();
        let t = CommentTarget { path: "a.rs".into(), line: 1, side: Side::Right };
        app.draft.upsert_comment(t.clone(), "mine".into(), false);

        let review: AiReview = serde_json::from_str(
            r#"{"body":"","comments":[{"path":"a.rs","line":1,"side":"RIGHT","body":"theirs"}]}"#,
        )
        .unwrap();
        let merged = merge(&mut app, review);

        assert_eq!((merged.added, merged.skipped), (0, 1));
        assert_eq!(merged.skipped_targets, vec!["a.rs:1 RIGHT"]);
        assert_eq!(app.draft.comment_at(&t).unwrap().body, "mine");
        assert!(!app.draft.comment_at(&t).unwrap().ai);
    }

    #[test]
    fn takes_the_summary_only_when_the_draft_has_none() {
        let mut app = app();
        merge(&mut app, serde_json::from_str(r#"{"body":"from ai"}"#).unwrap());
        assert_eq!(app.draft.body, "from ai");

        merge(&mut app, serde_json::from_str(r#"{"body":"second"}"#).unwrap());
        assert_eq!(app.draft.body, "from ai");
    }

    #[test]
    fn take_removes_the_file_so_it_is_not_merged_twice() {
        let dir = tempfile::tempdir().unwrap();
        let path = review_path(dir.path(), "k-narusawa/app", 42);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"body":"x","comments":[]}"#).unwrap();

        assert!(take(dir.path(), "k-narusawa/app", 42).unwrap().is_some());
        assert!(!path.exists());
        assert!(take(dir.path(), "k-narusawa/app", 42).unwrap().is_none());
    }

    #[test]
    fn take_returns_none_when_nothing_is_waiting() {
        let dir = tempfile::tempdir().unwrap();
        assert!(take(dir.path(), "k-narusawa/app", 42).unwrap().is_none());
    }

    #[test]
    fn a_corrupt_file_errors_and_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = review_path(dir.path(), "k-narusawa/app", 42);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ broken").unwrap();

        assert!(take(dir.path(), "k-narusawa/app", 42).is_err());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.corrupt")).unwrap(),
            "{ broken"
        );
    }

    #[test]
    fn panes_path_sits_beside_the_handoff_file() {
        let p = panes_path(std::path::Path::new("/state"), "k-narusawa/app", 42);
        assert_eq!(p, std::path::Path::new("/state/ai/k-narusawa-app-42.panes"));
    }

    #[test]
    fn review_path_flattens_repo_slash() {
        let p = review_path(std::path::Path::new("/state"), "k-narusawa/app", 42);
        assert_eq!(p, std::path::Path::new("/state/ai/k-narusawa-app-42.json"));
    }
}
