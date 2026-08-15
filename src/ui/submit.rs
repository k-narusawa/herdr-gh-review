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

pub fn render(draft: &Draft, cursor: usize, frame: &mut Frame) {
    let [area] = Layout::horizontal([Constraint::Length(56)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(10)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(format!(
            "  行コメント {} 件 / 全体コメント {}",
            draft.comments.len(),
            if draft.body.trim().is_empty() { "なし" } else { "あり" }
        )),
        Line::from(""),
    ];
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
        "  j/k:選択 Enter:提出 Esc:やめる",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Submit review ")),
        area,
    );
}
