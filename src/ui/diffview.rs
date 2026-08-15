use crate::app::{App, Row};
use crate::diff::LineKind;
use crate::gh::PrDetail;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(app: &mut App, pr: &PrDetail, frame: &mut Frame) {
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(pr, frame, areas[0]);
    render_body(app, frame, areas[1]);
    render_status(app, frame, areas[2]);
}

fn render_header(pr: &PrDetail, frame: &mut Frame, area: Rect) {
    let text = format!(
        " #{} {}   {}  +{} -{} ",
        pr.number, pr.title, pr.author, pr.additions, pr.deletions
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let text = app.status.clone().unwrap_or_else(|| {
        format!(
            " j/k:移動 }}:次ファイル c:コメント S:提出 q:戻る ?:ヘルプ    comments: {} ",
            app.draft.comments.len()
        )
    });
    frame.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn render_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    app.scroll = clamp_scroll(app.scroll, app.cursor, height);

    // app.rows の走査と row_to_line の両方が app を借りるため、ここで共有参照に落とす
    let app: &App = app;
    let lines: Vec<Line> = app
        .rows
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(height)
        .map(|(i, row)| row_to_line(app, row, i == app.cursor))
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// カーソルが画面外に出ないところまでだけスクロールを動かす
fn clamp_scroll(scroll: usize, cursor: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if cursor < scroll {
        return cursor;
    }
    if cursor >= scroll + height {
        return cursor + 1 - height;
    }
    scroll
}

fn row_to_line<'a>(app: &'a App, row: &'a Row, is_cursor: bool) -> Line<'a> {
    let base = match row {
        Row::FileHeader { file_idx } => {
            let file = &app.diff.files[*file_idx];
            let marker = if app.collapsed.contains(file_idx) { "▶" } else { "▼" };
            let extra = if file.is_binary {
                "  (binary)".to_string()
            } else if file.is_rename() {
                format!("  (renamed from {})", file.old_path.as_deref().unwrap_or(""))
            } else {
                String::new()
            };
            Line::from(vec![Span::styled(
                format!(
                    "{marker} {}  +{} -{}{extra}",
                    file.display_path(),
                    file.additions(),
                    file.deletions()
                ),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )])
        }
        Row::HunkHeader { file_idx, hunk_idx } => Line::from(Span::styled(
            app.diff.files[*file_idx].hunks[*hunk_idx].header.clone(),
            Style::default().fg(Color::DarkGray),
        )),
        Row::Line { file_idx, hunk_idx, line_idx } => {
            let line = &app.diff.files[*file_idx].hunks[*hunk_idx].lines[*line_idx];
            let (marker, color) = match line.kind {
                LineKind::Added => ('+', Color::Green),
                LineKind::Removed => ('-', Color::Red),
                LineKind::Context => (' ', Color::Reset),
            };
            let number = line
                .new_lineno
                .or(line.old_lineno)
                .map(|n| format!("{n:>5}"))
                .unwrap_or_else(|| "     ".to_string());
            Line::from(vec![
                Span::styled(format!("{number} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{marker}{}", line.text), Style::default().fg(color)),
            ])
        }
        Row::Comment { body_line, .. } => Line::from(Span::styled(
            format!("      💬 {body_line}"),
            Style::default().fg(Color::Yellow),
        )),
    };

    if is_cursor {
        base.style(Style::default().add_modifier(Modifier::REVERSED))
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_scroll;

    #[test]
    fn keeps_scroll_when_cursor_is_visible() {
        assert_eq!(clamp_scroll(10, 15, 20), 10);
    }

    #[test]
    fn scrolls_up_when_cursor_is_above() {
        assert_eq!(clamp_scroll(10, 3, 20), 3);
    }

    #[test]
    fn scrolls_down_just_enough_when_cursor_is_below() {
        assert_eq!(clamp_scroll(0, 25, 20), 6);
    }

    #[test]
    fn handles_zero_height() {
        assert_eq!(clamp_scroll(5, 5, 0), 0);
    }
}
