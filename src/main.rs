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
