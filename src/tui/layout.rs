use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Widget, Wrap},
};

use super::{ApprovalKind, ApprovalRequest};
use super::widgets::KeybindingBar;

pub fn dashboard(
    frame: &mut Frame,
    lobe_names: &[String],
    active_idx: usize,
    events_log: &[String],
    queue_len: usize,
) {
    let area = frame.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_tabs(frame, rows[0], lobe_names, active_idx);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(rows[1]);

    render_events_panel(frame, columns[0], events_log);
    render_queue_panel(frame, columns[1], queue_len);

    KeybindingBar {
        bindings: &[
            ("tab", "Switch"),
            ("n", "New inquiry"),
            ("q", "Quit"),
        ],
    }
    .render(rows[2], frame.buffer_mut());
}

fn render_tabs(frame: &mut Frame, area: Rect, lobe_names: &[String], active_idx: usize) {
    let titles: Vec<Line> = lobe_names
        .iter()
        .map(|n| Line::from(n.as_str()))
        .collect();

    let tabs = Tabs::new(titles)
        .select(active_idx)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .style(Style::default());

    frame.render_widget(tabs, area);
}

fn render_events_panel(frame: &mut Frame, area: Rect, events_log: &[String]) {
    let block = Block::default()
        .title(Span::styled("Events", Style::default().fg(Color::Cyan)))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    let start = events_log.len().saturating_sub(visible_height);
    let items: Vec<ListItem> = events_log[start..]
        .iter()
        .map(|line| ListItem::new(line.as_str()))
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_queue_panel(frame: &mut Frame, area: Rect, queue_len: usize) {
    let count_style = if queue_len > 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let block = Block::default()
        .title(Span::styled("Queue", Style::default().fg(Color::Cyan)))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Paragraph::new(Span::styled(
        format!("{} pending", queue_len),
        count_style,
    ))
    .alignment(Alignment::Center);

    frame.render_widget(text, inner);
}

pub fn approval(frame: &mut Frame, request: &ApprovalRequest) {
    let area = frame.area();
    let modal = centered_modal(area);

    let kind_label = match request.kind {
        ApprovalKind::CodeReview => "Code Review",
        ApprovalKind::Permission => "Permission",
    };
    let title = format!("{}: {}", kind_label, request.title);

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Cyan)))
        .borders(Borders::ALL);

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body = Paragraph::new(request.body.as_str()).wrap(Wrap { trim: false });
    frame.render_widget(body, sections[0]);

    KeybindingBar {
        bindings: &[("a", "Accept"), ("r", "Reject"), ("q", "Cancel")],
    }
    .render(sections[1], frame.buffer_mut());
}

fn centered_modal(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(vertical[1]);

    horizontal[1]
}
