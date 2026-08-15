use crate::diff::{CommentTarget, Side};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

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
    let parent = path.parent().expect("draft path has a parent");
    std::fs::create_dir_all(parent)
        .with_context(|| format!("下書きディレクトリを作れません: {}", path.display()))?;
    let json = serde_json::to_string_pretty(draft)?;

    // 書き込み途中で落ちても、path は完全な旧内容か完全な新内容のどちらかにしかならない
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .with_context(|| format!("下書きを書き込めません: {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("下書きを確定できません: {}", path.display()))
}

/// 壊れた下書きは無いものとして扱う。読めないファイルのために起動できない方が損失が大きい。
/// ただし次の save に上書きさせないよう、退避してから諦める
pub fn load(state_dir: &Path, repo: &str, pr_number: u32) -> Result<Option<Draft>> {
    let path = draft_path(state_dir, repo, pr_number);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    match serde_json::from_str(&raw) {
        Ok(draft) => Ok(Some(draft)),
        Err(_) => {
            let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
            Ok(None)
        }
    }
}

pub fn delete(state_dir: &Path, repo: &str, pr_number: u32) -> Result<()> {
    let path = draft_path(state_dir, repo, pr_number);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("下書きを削除できません: {}", path.display())),
    }
}

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

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &draft_with_two_comments()).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("drafts"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "一時ファイルが残っている: {leftovers:?}");
    }

    #[test]
    fn corrupt_draft_is_preserved_not_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = draft_path(dir.path(), "k-narusawa/app", 42);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ broken but precious").unwrap();

        assert_eq!(load(dir.path(), "k-narusawa/app", 42).unwrap(), None);

        let rescued = path.with_extension("json.corrupt");
        assert!(rescued.exists(), "壊れた下書きが退避されていない");
        assert_eq!(std::fs::read_to_string(&rescued).unwrap(), "{ broken but precious");
        assert!(!path.exists(), "壊れたファイルが元の場所に残っている");
    }
}
