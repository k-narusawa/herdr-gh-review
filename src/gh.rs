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
    let path_var = std::env::var("PATH").unwrap_or_default();
    gh_candidates(&path_var, &home)
        .into_iter()
        .find(|p| p.is_file())
}

/// PATH の空要素はカレントディレクトリを意味してしまうので落とす
fn gh_candidates(path_var: &str, home: &str) -> Vec<PathBuf> {
    let extra = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{home}/.local/bin"),
        format!("{home}/.local/share/mise/shims"),
    ];
    path_var
        .split(':')
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .chain(extra)
        .map(|d| PathBuf::from(d).join("gh"))
        .collect()
}

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
    fn gh_candidates_skip_empty_path_entries() {
        let candidates = gh_candidates("/usr/bin::/bin:", "/home/x");
        assert!(
            !candidates.iter().any(|p| p == std::path::Path::new("gh")),
            "相対パスの gh が候補に混じっている: {candidates:?}"
        );
        assert!(candidates.contains(&PathBuf::from("/usr/bin/gh")));
        assert!(candidates.contains(&PathBuf::from("/bin/gh")));
    }

    #[test]
    fn gh_candidates_fall_back_when_path_is_empty() {
        assert_eq!(
            gh_candidates("", "/home/x"),
            vec![
                PathBuf::from("/opt/homebrew/bin/gh"),
                PathBuf::from("/usr/local/bin/gh"),
                PathBuf::from("/home/x/.local/bin/gh"),
                PathBuf::from("/home/x/.local/share/mise/shims/gh"),
            ]
        );
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
