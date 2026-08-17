use crate::app::App;
use crate::diff::ParsedDiff;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub const WIDTH: u16 = 32;

/// Narrower terminals drop the tree in favor of the diff itself
pub const MIN_TERMINAL_WIDTH: u16 = 80;

#[derive(Debug, PartialEq, Eq)]
enum Node<'a> {
    Dir { depth: usize, name: &'a str },
    File { depth: usize, name: &'a str, file_idx: usize },
}

/// Sort by path so a shared directory is emitted only once
fn build(diff: &ParsedDiff) -> Vec<Node<'_>> {
    let mut paths: Vec<(&str, usize)> = diff
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.display_path(), i))
        .collect();
    paths.sort_unstable();

    let mut nodes = Vec::new();
    let mut prev: Vec<&str> = Vec::new();
    for (path, file_idx) in paths {
        let segments: Vec<&str> = path.split('/').collect();
        let (name, dirs) = segments.split_last().expect("split never yields an empty slice");
        let shared = dirs
            .iter()
            .zip(&prev)
            .take_while(|(a, b)| a == b)
            .count();
        for (depth, dir) in dirs.iter().enumerate().skip(shared) {
            nodes.push(Node::Dir { depth, name: dir });
        }
        nodes.push(Node::File { depth: dirs.len(), name, file_idx });
        prev = dirs.to_vec();
    }
    nodes
}

pub fn render(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::RIGHT);
    let [gutter, inner] =
        Layout::horizontal([Constraint::Length(1), Constraint::Min(1)]).areas(block.inner(area));
    frame.render_widget(block, area);

    let nodes = build(&app.diff);
    let current = app.cursor_file_idx();
    let selected = current.and_then(|idx| {
        nodes
            .iter()
            .position(|n| matches!(n, Node::File { file_idx, .. } if *file_idx == idx))
    });

    let height = inner.height as usize;
    let scroll = match selected {
        Some(sel) => super::diffview::clamp_scroll(app.tree_scroll, sel, height),
        None => app.tree_scroll.min(nodes.len().saturating_sub(1)),
    };

    let lines: Vec<Line> = nodes
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height)
        .map(|(i, node)| node_to_line(app, node, inner.width as usize, Some(i) == selected))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
    if let Some(row) = selected.and_then(|sel| sel.checked_sub(scroll))
        && row < height
    {
        let mut marks = vec![Line::from(""); row];
        marks.push(Line::from(super::CURSOR));
        frame.render_widget(Paragraph::new(marks), gutter);
    }
    app.tree_scroll = scroll;
}

fn node_to_line<'a>(app: &'a App, node: &Node<'a>, width: usize, is_selected: bool) -> Line<'a> {
    let line = match *node {
        Node::Dir { depth, name } => Line::from(Span::styled(
            format!("{}{name}/", indent(depth)),
            Style::default().fg(Color::Blue),
        )),
        Node::File { depth, name, file_idx } => {
            let file = &app.diff.files[file_idx];
            let counts = format!("+{} -{}", file.additions(), file.deletions());
            let label = fit(&format!("{}{name}", indent(depth)), width, span_width(&counts) + 1);
            vec![
                Span::raw(label),
                Span::styled(counts, Style::default().fg(Color::DarkGray)),
            ]
            .into()
        }
    };

    if is_selected {
        line.style(Style::default().add_modifier(Modifier::BOLD))
    } else {
        line
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn span_width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Keep `reserved` columns free on the right; a name too long loses its head to `…`
fn fit(label: &str, width: usize, reserved: usize) -> String {
    let room = width.saturating_sub(reserved);
    let mut label = label.to_string();
    if span_width(&label) > room {
        while span_width(&label) > room.saturating_sub(1) && !label.is_empty() {
            label.remove(0);
        }
        label.insert(0, '…');
    }
    let pad = room.saturating_sub(span_width(&label));
    label.push_str(&" ".repeat(pad));
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(diff: &str) -> Vec<String> {
        build(&crate::diff::parse(diff))
            .iter()
            .map(|n| match *n {
                Node::Dir { depth, name } => format!("{}{name}/", indent(depth)),
                Node::File { depth, name, .. } => format!("{}{name}", indent(depth)),
            })
            .collect()
    }

    const DIFF: &str = "\
diff --git a/src/ui/diffview.rs b/src/ui/diffview.rs
--- a/src/ui/diffview.rs
+++ b/src/ui/diffview.rs
@@ -1,1 +1,1 @@
-a
+b
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,1 +1,1 @@
-a
+b
diff --git a/src/app.rs b/src/app.rs
--- a/src/app.rs
+++ b/src/app.rs
@@ -1,1 +1,1 @@
-a
+b
";

    #[test]
    fn nests_paths_and_emits_each_directory_once() {
        assert_eq!(
            tree(DIFF),
            vec!["README.md", "src/", "  app.rs", "  ui/", "    diffview.rs"]
        );
    }

    #[test]
    fn keeps_file_indices_pointing_at_the_original_diff_order() {
        let diff = crate::diff::parse(DIFF);
        let nodes = build(&diff);
        let Node::File { file_idx, .. } = nodes[0] else {
            panic!("README.md is not a file node");
        };
        assert_eq!(diff.files[file_idx].display_path(), "README.md");
    }

    #[test]
    fn truncates_a_long_name_and_pads_to_the_reserved_width() {
        let fitted = fit("very_long_file_name.rs", 12, 6);
        assert_eq!(span_width(&fitted), 6);
        assert!(fitted.starts_with('…'));
    }

    #[test]
    fn pads_a_short_name_so_the_counts_line_up() {
        assert_eq!(fit("a.rs", 12, 6), "a.rs  ");
    }
}
