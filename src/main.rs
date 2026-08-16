mod app;
mod delta;
mod diff;
mod editor;
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
        Target::RepoPrList => run_pr_list(terminal, gh, ListKind::Repo),
        Target::ReviewRequested => run_pr_list(terminal, gh, ListKind::ReviewRequested),
        Target::Authored => run_pr_list(terminal, gh, ListKind::Authored),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Repo,
    ReviewRequested,
    Authored,
}

impl ListKind {
    fn title(self) -> &'static str {
        match self {
            ListKind::Repo => "Open pull requests",
            ListKind::ReviewRequested => "PRs awaiting my review",
            ListKind::Authored => "My pull requests",
        }
    }

    fn fetch(self, gh: &Gh) -> Result<Vec<PrSummary>> {
        match self {
            ListKind::Repo => gh.list_prs(),
            ListKind::ReviewRequested => gh.search_review_requested(),
            ListKind::Authored => gh.search_authored(),
        }
    }

    /// カレントディレクトリのリモートに依存するのはリポジトリ一覧だけ
    fn needs_repo(self) -> bool {
        self == ListKind::Repo
    }
}

fn run_pr_list(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    kind: ListKind,
) -> Result<()> {
    let title = kind.title();

    let mut prs = Vec::new();
    let mut cursor = 0usize;
    let mut status: Option<String> = None;

    match kind.fetch(gh) {
        Ok(v) => prs = v,
        Err(e) => {
            gh::log_error(&e);
            status = Some(fetch_error_message(&e, kind));
        }
    }

    loop {
        terminal.draw(|f| ui::prlist::render(&prs, cursor, title, status.as_deref(), f))?;

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
                status = None;
                match kind.fetch(gh) {
                    Ok(v) => {
                        prs = v;
                        cursor = 0;
                    }
                    Err(e) => {
                        gh::log_error(&e);
                        status = Some(fetch_error_message(&e, kind));
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(pr) = prs.get(cursor) {
                    if let Err(e) = open_pr(terminal, gh, pr.repo.as_deref(), pr.number) {
                        gh::log_error(&e);
                        status = Some(first_line(&e.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
}

/// カレントディレクトリがGitHubリポジトリでない場合が最も多いので、そのときだけ次の手を添える
fn fetch_error_message(error: &anyhow::Error, kind: ListKind) -> String {
    let message = first_line(&error.to_string());
    if !kind.needs_repo() {
        return message;
    }
    format!("{message}（--review-requested / --authored なら任意の場所から使えます）")
}

fn open_pr(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    repo: Option<&str>,
    number: u32,
) -> Result<()> {
    let pr = gh.pr_detail(repo, number)?;
    let raw = gh.pr_diff(&pr.repo, pr.number)?;
    let state = review::state_dir();
    let draft = review::load(&state, &pr.repo, pr.number)?
        .unwrap_or_else(|| review::Draft::new(&pr.repo, pr.number, &pr.head_sha));

    let mut app = App::new(&raw, draft);
    if app.draft.head_sha != pr.head_sha {
        app.status = Some(
            " 保存されていた下書きは古いコミットのものです。行の位置を確認してください ".into(),
        );
    }
    warn_unmatched(&mut app);
    run_diff_view(terminal, gh, &mut app, &pr)
}

enum KeyOutcome {
    Continue,
    Leave,
}

fn run_diff_view(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
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
        match handle_key(terminal, gh, app, pr, key)? {
            KeyOutcome::Leave => return Ok(()),
            KeyOutcome::Continue => {}
        }
    }
}

fn handle_key(
    terminal: &mut ratatui::DefaultTerminal,
    gh: &Gh,
    app: &mut App,
    pr: &PrDetail,
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
        (KeyCode::Char('S'), _) => submit(terminal, app, gh)?,
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
        _ => {}
    }
    Ok(KeyOutcome::Continue)
}

/// 下書きは残したままdiffだけ取り直す。失敗しても今の画面は壊さない
fn reload(app: &mut App, gh: &Gh, pr: &PrDetail) -> Result<()> {
    match gh.pr_diff(&pr.repo, pr.number) {
        Ok(raw) => {
            app.set_diff(&raw);
            app.status = Some(" 再読み込みしました ".into());
            warn_unmatched(app);
        }
        Err(e) => {
            gh::log_error(&e);
            app.status = Some(format!(" {} ", first_line(&e.to_string())));
        }
    }
    Ok(())
}

/// 画面に出ないコメントは、提出時にGitHubがレビュー全体を422で拒否する原因になる
fn warn_unmatched(app: &mut App) {
    let n = app.unmatched_comments();
    if n > 0 {
        app.status = Some(format!(
            " 現在のdiffに一致しないコメントが{n}件あります。このまま提出すると失敗します ",
        ));
    }
}

fn submit(terminal: &mut ratatui::DefaultTerminal, app: &mut App, gh: &Gh) -> Result<()> {
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
                        app.draft.comments.clear();
                        app.draft.body.clear();
                        app.rebuild_rows();

                        // GitHubは受理済み。下書きの後片付けが失敗しても提出の失敗として
                        // 扱うと、ユーザーが再提出して重複レビューになりかねない
                        let cleanup = review::delete(
                            &review::state_dir(),
                            &app.draft.repo,
                            app.draft.pr_number,
                        );
                        app.status = Some(match cleanup {
                            Ok(()) => format!(" {} で提出しました ", event.label()),
                            Err(e) => {
                                gh::log_error(&e);
                                format!(
                                    " {} で提出しました（下書きファイルは削除できませんでした） ",
                                    event.label()
                                )
                            }
                        });
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
            None => {
                app.status = Some(" 削除するコメントがありません ".into());
                return Ok(());
            }
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
    if body == initial {
        return Ok(());
    }

    app.draft.body = body;
    review::save(&review::state_dir(), &app.draft)?;
    app.status = Some(if app.draft.body.is_empty() {
        " 全体コメントを空にしました ".into()
    } else {
        " 全体コメントを更新しました ".into()
    });
    Ok(())
}
