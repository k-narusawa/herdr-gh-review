use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const KEYS: &[(&str, &str)] = &[
    ("j / k", "move"),
    ("Ctrl-d / Ctrl-u", "half-page move"),
    ("g / G", "top / bottom"),
    ("} / {", "next / previous file"),
    ("Tab", "collapse a file"),
    ("T", "toggle the file tree"),
    ("s", "toggle split / unified"),
    ("h / l", "move between cells in split view"),
    ("c", "comment on the line under the cursor"),
    ("d", "delete a comment"),
    ("D", "discard comments no longer in the diff"),
    ("e", "edit the review summary"),
    ("S", "submit"),
    ("A", "ask the AI to review this PR"),
    ("o", "open in a browser"),
    ("r", "reload"),
    ("q", "back"),
];

pub fn render(frame: &mut Frame) {
    let [area] = Layout::horizontal([Constraint::Length(52)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(KEYS.len() as u16 + 4)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|(key, desc)| Line::from(format!("  {key:<18}{desc}")))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from("  press any key to go back").style(Style::default().fg(Color::DarkGray)));

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}
