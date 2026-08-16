use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const KEYS: &[(&str, &str)] = &[
    ("j / k", "移動"),
    ("Ctrl-d / Ctrl-u", "半画面移動"),
    ("g / G", "先頭 / 末尾"),
    ("} / {", "次 / 前のファイル"),
    ("Tab", "ファイルの折りたたみ"),
    ("T", "ファイルツリーの表示切替"),
    ("s", "split / unified の切替"),
    ("h / l", "split時のカーソル左右"),
    ("c", "カーソル行にコメント"),
    ("d", "コメントを削除"),
    ("D", "現在のdiffに無いコメントを破棄"),
    ("e", "レビュー全体コメントを編集"),
    ("S", "提出"),
    ("o", "ブラウザで開く"),
    ("r", "再読み込み"),
    ("q", "戻る"),
];

pub fn render(frame: &mut Frame) {
    let [area] = Layout::horizontal([Constraint::Length(48)])
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
    lines.push(Line::from("  何かキーを押すと戻ります").style(Style::default().fg(Color::DarkGray)));

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}
