use crate::app::{App, Row};
use crate::diff::{DiffLine, Hunk, LineKind, Side};
use crate::gh::PrDetail;
use ansi_to_tui::IntoText;
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
            " j/k:move }}:next file s:{} c:comment S:submit q:back ?:help    comments: {} ",
            if app.split { "unified" } else { "split" },
            app.draft.comments.len()
        )
    });
    frame.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn render_body(app: &mut App, frame: &mut Frame, area: Rect) {
    if !app.show_tree || area.width < super::filetree::MIN_TERMINAL_WIDTH {
        return render_diff(app, frame, area);
    }
    let [tree, diff] =
        Layout::horizontal([Constraint::Length(super::filetree::WIDTH), Constraint::Min(1)])
            .areas(area);
    super::filetree::render(app, frame, tree);
    render_diff(app, frame, diff);
}

fn render_diff(app: &mut App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    app.scroll = clamp_scroll(app.scroll, app.cursor, height);

    let lines: Vec<Line> = app
        .rows
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(height)
        .map(|(i, row)| row_to_line(app, row, i == app.cursor, area.width as usize))
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// Scroll only as far as it takes to keep the cursor on screen
pub(super) fn clamp_scroll(scroll: usize, cursor: usize, height: usize) -> usize {
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

fn row_to_line<'a>(app: &'a App, row: &'a Row, is_cursor: bool, width: usize) -> Line<'a> {
    // A Pair row reverses one cell, not the whole row, so it returns before the shared tail
    if let Row::Pair { file_idx, hunk_idx, left, right } = *row {
        let hunk = &app.diff.files[file_idx].hunks[hunk_idx];
        return pair_to_line(app, hunk, left, right, is_cursor, width);
    }

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
            Line::from(line_spans(app, line, line.new_lineno.or(line.old_lineno)))
        }
        Row::Pair { .. } => unreachable!("returned above"),
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

fn line_spans<'a>(app: &'a App, line: &'a DiffLine, number: Option<u32>) -> Vec<Span<'a>> {
    let number = number
        .map(|n| format!("{n:>5}"))
        .unwrap_or_else(|| "     ".to_string());
    let mut spans = vec![Span::styled(
        format!("{number} "),
        Style::default().fg(Color::DarkGray),
    )];
    match app.colored_line(line.raw_idx).and_then(ansi_spans) {
        Some(colored) => spans.extend(colored),
        None => {
            let (marker, color) = match line.kind {
                LineKind::Added => ('+', Color::Green),
                LineKind::Removed => ('-', Color::Red),
                LineKind::Context => (' ', Color::Reset),
            };
            spans.push(Span::styled(
                format!("{marker}{}", line.text),
                Style::default().fg(color),
            ));
        }
    }
    spans.into_iter().map(expand_tabs).collect()
}

/// ratatui's width calculation and the cells actually drawn disagree on tabs, which shifts the
/// split separator out of line.
/// ponytail: a flat 4 columns, not real tab stops. Identical for code that only indents with them
fn expand_tabs(span: Span<'_>) -> Span<'_> {
    if !span.content.contains('\t') {
        return span;
    }
    Span::styled(span.content.replace('\t', "    "), span.style)
}

fn pair_to_line<'a>(
    app: &'a App,
    hunk: &'a Hunk,
    left: Option<usize>,
    right: Option<usize>,
    is_cursor: bool,
    width: usize,
) -> Line<'a> {
    let cell = width.saturating_sub(1) / 2;
    let mut cells = [
        cell_spans(app, hunk, left, Side::Left, cell),
        cell_spans(app, hunk, right, Side::Right, width.saturating_sub(cell + 1)),
    ];

    if is_cursor {
        let active = match app.active_side(left, right) {
            Side::Left => 0,
            Side::Right => 1,
        };
        for span in &mut cells[active] {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }

    let [l, r] = cells;
    let mut spans = l;
    spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
    spans.extend(r);
    Line::from(spans)
}

fn cell_spans<'a>(
    app: &'a App,
    hunk: &'a Hunk,
    line_idx: Option<usize>,
    side: Side,
    width: usize,
) -> Vec<Span<'a>> {
    let Some(line) = line_idx.and_then(|i| hunk.lines.get(i)) else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let number = match side {
        Side::Left => line.old_lineno,
        Side::Right => line.new_lineno,
    };
    fit_spans(line_spans(app, line, number), width)
}

/// Cut the spans down to exactly `width` columns, padding with spaces if they fall short
fn fit_spans(spans: Vec<Span<'_>>, width: usize) -> Vec<Span<'_>> {
    let mut out: Vec<Span> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let room = width - used;
        if room == 0 {
            break;
        }
        if span.width() <= room {
            used += span.width();
            out.push(span);
            continue;
        }
        let mut content = span.content.into_owned();
        while Span::raw(content.as_str()).width() > room {
            content.pop();
        }
        used += Span::raw(content.as_str()).width();
        out.push(Span::styled(content, span.style));
        break;
    }
    if used < width {
        out.push(Span::raw(" ".repeat(width - used)));
    }
    out
}

/// Turn one ANSI line into ratatui spans. `None` if it is malformed, falling back to our colors
fn ansi_spans(raw: &str) -> Option<Vec<Span<'static>>> {
    let text = raw.as_bytes().into_text().ok()?;
    Some(text.lines.into_iter().next()?.spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Row;
    use ratatui::text::Span;

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

    #[test]
    fn splits_an_ansi_line_into_styled_spans() {
        let spans = ansi_spans("\x1b[32m+ok\x1b[0m").unwrap();
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "+ok");
        assert!(spans.iter().any(|s| s.style.fg.is_some()));
    }

    #[test]
    fn keeps_plain_text_as_one_span() {
        let spans = ansi_spans(" fn main() {").unwrap();
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, " fn main() {");
    }

    fn width_of(spans: &[Span]) -> usize {
        spans.iter().map(|s| s.width()).sum()
    }

    #[test]
    fn fit_spans_pads_a_short_cell_to_the_full_width() {
        let got = fit_spans(vec![Span::raw("ab")], 6);
        assert_eq!(width_of(&got), 6);
    }

    #[test]
    fn fit_spans_truncates_and_keeps_the_style_of_the_cut_span() {
        let style = Style::default().fg(Color::Green);
        let got = fit_spans(vec![Span::raw("ab"), Span::styled("cdef", style)], 4);
        assert_eq!(width_of(&got), 4);
        let text: String = got.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "abcd");
        assert_eq!(got.last().unwrap().style, style);
    }

    #[test]
    fn fit_spans_handles_a_zero_width_cell() {
        assert_eq!(width_of(&fit_spans(vec![Span::raw("abc")], 0)), 0);
    }

    const UNEVEN: &str = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,2 @@
-one
-two
+ONE
";

    fn split_app() -> App {
        let mut app = App::new(UNEVEN, crate::review::Draft::new("o/r", 1, "sha"));
        app.toggle_split();
        app
    }

    fn pair_line(app: &App, row_idx: usize, width: usize) -> Line<'_> {
        let Row::Pair { file_idx, hunk_idx, left, right } = app.rows[row_idx] else {
            panic!("not a Pair row: {:?}", app.rows[row_idx]);
        };
        let hunk = &app.diff.files[file_idx].hunks[hunk_idx];
        pair_to_line(app, hunk, left, right, false, width)
    }

    #[test]
    fn a_pair_row_fills_exactly_the_available_width() {
        let app = split_app();
        for width in [40usize, 41, 80] {
            assert_eq!(pair_line(&app, 2, width).width(), width, "width={width}");
        }
    }

    #[test]
    fn a_pair_row_shows_both_sides_with_their_own_line_numbers() {
        let app = split_app();
        let text = pair_line(&app, 2, 60).to_string();
        let (left, right) = text.split_once('│').expect("no separator");
        assert!(left.contains("-one"), "left is not the removed line: {left:?}");
        assert!(left.trim_start().starts_with('1'), "left number is not the old one: {left:?}");
        assert!(right.contains("+ONE"), "right is not the added line: {right:?}");
        assert!(right.trim_start().starts_with('1'), "right number is not the new one: {right:?}");
    }

    #[test]
    fn an_empty_cell_still_takes_its_column() {
        let app = split_app();
        // the second pair is (-two, none)
        let text = pair_line(&app, 3, 60).to_string();
        let (left, right) = text.split_once('│').expect("no separator");
        assert!(left.contains("-two"));
        assert!(right.trim().is_empty(), "right cell is not empty: {right:?}");
    }

    /// Tabs and CJK are where the width calculation and the cells drawn drift apart. One column
    /// of drift breaks the vertical line, so assert against what was actually rendered
    #[test]
    fn the_separator_stays_in_one_column() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        const TABS_AND_CJK: &str = "\
diff --git a/ai.go b/ai.go
--- a/ai.go
+++ b/ai.go
@@ -1,4 +1,4 @@
 // Command writes 内容 to a temp file — CJK on purpose, see above
-func Command(aiCmd string) (*exec.Cmd, error) {
+func Command(aiCmd string) (*exec.Cmd, string, error) {
 \treturn command(aiCmd)
 }
";
        const W: u16 = 100;
        let mut app = App::new(TABS_AND_CJK, crate::review::Draft::new("o/r", 1, "sha"));
        app.toggle_split();
        app.show_tree = false;

        let mut terminal = Terminal::new(TestBackend::new(W, 10)).unwrap();
        terminal
            .draw(|f| render_diff(&mut app, f, f.area()))
            .unwrap();

        let columns: Vec<Vec<u16>> = (0..10)
            .map(|row| {
                (0..W)
                    .filter(|c| terminal.backend().buffer()[(*c, row)].symbol() == "│")
                    .collect()
            })
            .filter(|cols: &Vec<u16>| !cols.is_empty())
            .collect();

        assert!(columns.len() >= 4, "no Pair rows were drawn: {columns:?}");
        assert!(
            columns.iter().all(|c| c == &columns[0]),
            "the separator column is not aligned: {columns:?}"
        );
    }
}

