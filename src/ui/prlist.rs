use crate::gh::PrSummary;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(prs: &[PrSummary], cursor: usize, title: &str, status: Option<&str>, frame: &mut Frame) {
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

    if let Some(message) = status {
        frame.render_widget(
            Paragraph::new(format!("  {message}")).style(Style::default().fg(Color::Red)),
            areas[1],
        );
    } else if prs.is_empty() {
        frame.render_widget(
            Paragraph::new("  no matching pull requests")
                .style(Style::default().fg(Color::DarkGray)),
            areas[1],
        );
    } else {
        let lines: Vec<Line> = prs
            .iter()
            .enumerate()
            .map(|(i, pr)| {
                let counts = match (pr.additions, pr.deletions) {
                    (Some(a), Some(d)) => format!("  +{a} -{d}"),
                    _ => String::new(),
                };
                let draft = if pr.is_draft { " [draft]" } else { "" };
                // The marker takes the row's leading blank, so the columns stay put
                let lead = if i == cursor { super::CURSOR } else { " " };
                let line = Line::from(vec![
                    Span::raw(lead),
                    Span::styled(format!("#{:<6}", pr.number), Style::default().fg(Color::Cyan)),
                    Span::raw(pr.title.clone()),
                    Span::styled(format!("  @{}{counts}{draft}", pr.author), Style::default().fg(Color::DarkGray)),
                ]);
                if i == cursor {
                    line.style(Style::default().add_modifier(Modifier::BOLD))
                } else {
                    line
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), areas[1]);
    }

    frame.render_widget(
        Paragraph::new(" j/k:move Enter:open r:reload q:quit ")
            .style(Style::default().add_modifier(Modifier::REVERSED)),
        areas[2],
    );
}
