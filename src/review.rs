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
