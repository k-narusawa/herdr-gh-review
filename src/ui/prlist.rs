use crate::gh::PrSummary;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(prs: &[PrSummary], cursor: usize, title: &str, frame: &mut Frame) {
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    frame.render_widget(
        Paragraph::new(format!(" {title} ({}) ", prs.len()))
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        areas[0],
    );

    if prs.is_empty() {
        frame.render_widget(
            Paragraph::new("  該当するPRがありません")
                .style(Style::default().fg(Color::DarkGray)),
            areas[1],
        );
    } else {
        let lines: Vec<Line> = prs
            .iter()
            .enumerate()
            .map(|(i, pr)| {
                let repo = pr.repo.as_deref().map(|r| format!("{r} ")).unwrap_or_default();
                let counts = match (pr.additions, pr.deletions) {
                    (Some(a), Some(d)) => format!("  +{a} -{d}"),
                    _ => String::new(),
                };
                let draft = if pr.is_draft { " [draft]" } else { "" };
                let line = Line::from(vec![
                    Span::styled(format!(" {repo}#{:<6}", pr.number), Style::default().fg(Color::Cyan)),
                    Span::raw(pr.title.clone()),
                    Span::styled(format!("  @{}{counts}{draft}", pr.author), Style::default().fg(Color::DarkGray)),
                ]);
                if i == cursor {
                    line.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    line
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), areas[1]);
    }

    frame.render_widget(
        Paragraph::new(" j/k:移動 Enter:開く r:再読み込み q:終了 ")
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        areas[2],
    );
}
