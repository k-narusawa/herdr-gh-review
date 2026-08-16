use crate::review::{Draft, ReviewEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub const EVENTS: [ReviewEvent; 3] = [
    ReviewEvent::Comment,
    ReviewEvent::Approve,
    ReviewEvent::RequestChanges,
];

pub fn render(draft: &Draft, stale: usize, cursor: usize, frame: &mut Frame) {
    let mut lines = vec![Line::from(format!(
        "  {} line comments / summary: {}",
        draft.comments.len() - stale,
        if draft.body.trim().is_empty() { "none" } else { "yes" }
    ))];
    if stale > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ! {stale} not in the current diff, will not be sent"),
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(""));

    for (i, event) in EVENTS.iter().enumerate() {
        let text = format!("  {}  ", event.label());
        lines.push(if i == cursor {
            Line::from(Span::styled(text, Style::default().add_modifier(Modifier::REVERSED)))
        } else {
            Line::from(text)
        });
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  j/k:select Enter:submit Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let [area] = Layout::horizontal([Constraint::Length(56)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(lines.len() as u16 + 2)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Submit review ")),
        area,
    );
}
