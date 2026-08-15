mod app;
mod diff;
mod gh;
mod review;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gh::{Gh, PrDetail};

fn main() -> Result<()> {
    let gh = Gh::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (repo, number) = match args.as_slice() {
        [flag, n] if flag == "--pr" => (None, n.parse()?),
        _ => anyhow::bail!("usage: herdr-gh-review --pr <number>"),
    };

    let pr = gh.pr_detail(repo, number)?;
    let parsed = diff::parse(&gh.pr_diff(&pr.repo, pr.number)?);
    let state = review::state_dir();
    let draft = review::load(&state, &pr.repo, pr.number)?
        .unwrap_or_else(|| review::Draft::new(&pr.repo, pr.number, &pr.head_sha));
    let mut app = App::new(parsed, draft);

    let mut terminal = ratatui::init();
    let result = run_diff_view(&mut terminal, &mut app, &pr);
    ratatui::restore();
    result
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
