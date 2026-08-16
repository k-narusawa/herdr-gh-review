use crate::gh::repo_from_url;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    RepoPrList,
    ReviewRequested,
    Authored,
    Pr { repo: Option<String>, number: u32 },
}

impl Target {
    pub fn resolve(args: &[String], env: Option<&str>) -> Result<Self> {
        match args {
            [] => {}
            [flag] if flag == "--review-requested" => return Ok(Target::ReviewRequested),
            [flag] if flag == "--authored" => return Ok(Target::Authored),
            [flag, value] if flag == "--pr" => {
                let number = value
                    .parse()
                    .map_err(|_| anyhow!("PR番号として解釈できません: {value}"))?;
                return Ok(Target::Pr { repo: None, number });
            }
            [flag, value] if flag == "--url" => return Self::from_url(value),
            _ => return Err(anyhow!(
                "usage: herdr-gh-review [--review-requested | --authored | --pr <number> | --url <url>]"
            )),
        }

        match env {
            None => Ok(Target::RepoPrList),
            Some("review-requested") => Ok(Target::ReviewRequested),
            Some("authored") => Ok(Target::Authored),
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
    fn flag_selects_authored() {
        assert_eq!(
            Target::resolve(&args(["--authored"].as_slice()), None).unwrap(),
            Target::Authored
        );
    }

    #[test]
    fn env_selects_authored() {
        assert_eq!(Target::resolve(&[], Some("authored")).unwrap(), Target::Authored);
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
