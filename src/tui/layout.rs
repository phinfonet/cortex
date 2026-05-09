use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Widget, Wrap},
};

use super::theme;
use super::widgets::KeybindingBar;
use super::{
    ApprovalKind, ApprovalRequest, Focus, ReviewItem, ReviewKind, ReviewStatus, Section,
    TaskEntry, TaskKind,
};

#[derive(Debug, Clone)]
pub struct SidebarProject {
    pub name: String,
    pub path: std::path::PathBuf,
    pub components: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub lobe: String,
    pub is_active: bool,
    pub projects: Vec<SidebarProject>,
}

#[allow(clippy::too_many_arguments)]
pub fn main_screen(
    frame: &mut Frame,
    focus: Focus,
    active_lobe: &str,
    sidebar_items: &[SidebarItem],
    sidebar_selected_idx: usize,
    active_section: Section,
    pending_review_count: usize,
    daemon_connected: bool,
    tasks: &[TaskEntry],
    reviews: &[ReviewItem],
    review_selected_idx: usize,
    logs: &[String],
    log_scroll: u16,
    input_text: &str,
    active_lobe_name: &str,
    command_context: Option<&str>,
) {
    let area = frame.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(1)])
        .split(rows[0]);

    render_sidebar(frame, columns[0], sidebar_items, sidebar_selected_idx, tasks, focus, active_lobe_name);

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(columns[1]);

    render_header(frame, right_rows[0], active_lobe, daemon_connected);
    render_section_tabs(frame, right_rows[1], active_section, pending_review_count);

    match active_section {
        Section::Review => render_content_block(frame, right_rows[2], |frame, inner| {
            render_reviews(frame, inner, reviews, review_selected_idx);
        }),
        Section::Log => render_content_block(frame, right_rows[2], |frame, inner| {
            render_log(frame, inner, logs, log_scroll);
        }),
    }

    let sep = "─".repeat(right_rows[3].width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(theme::style_border()),
        right_rows[3],
    );
    render_input_bar(frame, right_rows[4], focus, input_text, command_context);

    let bindings: &[(&str, &str)] = if focus == Focus::Sidebar {
        &[("↑/↓", "nav"), ("enter", "ctx"), ("n", "idea"), ("i", "input"), ("spc", "lobes"), ("q", "quit")]
    } else {
        &[("↑/↓", "nav"), ("tab", "section"), ("enter", "open"), ("i", "input"), ("spc", "lobes"), ("q", "quit")]
    };
    KeybindingBar { bindings }.render(rows[1], frame.buffer_mut());
}

fn render_header(
    frame: &mut Frame,
    area: Rect,
    active_lobe: &str,
    daemon_connected: bool,
) {
    let spans = vec![
        Span::styled("  cortex", theme::style_accent()),
        Span::styled("  │  ", theme::style_dim()),
        Span::styled(active_lobe.to_owned(), theme::style_accent2()),
        Span::styled("  │  ", theme::style_dim()),
        Span::styled(
            if daemon_connected { "● connected" } else { "○ offline" },
            if daemon_connected { theme::style_success() } else { theme::style_dim() },
        ),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::style_normal()),
        area,
    );
}

fn render_sidebar(
    frame: &mut Frame,
    area: Rect,
    items: &[SidebarItem],
    selected_idx: usize,
    tasks: &[TaskEntry],
    focus: Focus,
    active_lobe_name: &str,
) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_type(theme::BORDER)
        .border_style(theme::style_border())
        .style(theme::style_sidebar_bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner);

    // ── Section 1: project/component tree (always expanded) ─────
    let Some(item) = items.first() else {
        return;
    };

    let w = inner.width as usize;
    // Flat list of all rendered rows. nav_rows[nav_idx] = flat row index for that navigable item.
    let mut flat_items: Vec<ListItem> = Vec::new();
    let mut nav_rows: Vec<usize> = Vec::new(); // navigable flat indices, in sidebar order

    // Lobe header (non-navigable)
    flat_items.push(ListItem::new(Line::from(Span::styled(
        format!(" ▾ {}", item.lobe),
        theme::style_sidebar_header(),
    ))));

    let is_focused = focus == Focus::Sidebar;
    let project_count = item.projects.len();

    for (proj_idx, project) in item.projects.iter().enumerate() {
        // nav_idx for this project = nav_rows.len() at the time we push
        let proj_nav_idx = nav_rows.len();
        let is_sel = proj_nav_idx == selected_idx;
        let is_last_proj = proj_idx + 1 == project_count;

        let proj_marker = if is_sel && is_focused {
            "●"
        } else if is_sel {
            "◦"
        } else if is_last_proj && project.components.is_empty() {
            "╰"
        } else {
            "├"
        };
        let proj_style = if is_sel && is_focused {
            theme::style_sidebar_selected()
        } else if is_sel {
            theme::style_accent2()
        } else {
            theme::style_sidebar_item()
        };

        let max_name = w.saturating_sub(4);
        let proj_name = truncate_name(&project.name, max_name);

        nav_rows.push(flat_items.len());
        flat_items.push(ListItem::new(Line::from(Span::styled(
            format!(" {proj_marker} {proj_name}"),
            proj_style,
        ))));

        // Component rows
        let comp_count = project.components.len();
        for (comp_idx, comp) in project.components.iter().enumerate() {
            let comp_nav_idx = nav_rows.len();
            let is_comp_sel = comp_nav_idx == selected_idx;
            let is_last_comp = comp_idx + 1 == comp_count;
            let is_last_in_tree = is_last_proj && is_last_comp;

            let comp_marker = if is_comp_sel && is_focused {
                "●"
            } else if is_comp_sel {
                "◦"
            } else if is_last_in_tree {
                "╰"
            } else {
                "├"
            };
            let comp_style = if is_comp_sel && is_focused {
                theme::style_sidebar_selected()
            } else if is_comp_sel {
                theme::style_accent2()
            } else {
                theme::style_sidebar_dim()
            };

            let max_comp = w.saturating_sub(6);
            let comp_name = truncate_name(comp, max_comp);

            nav_rows.push(flat_items.len());
            flat_items.push(ListItem::new(Line::from(Span::styled(
                format!("   {comp_marker} {comp_name}"),
                comp_style,
            ))));
        }
    }

    let highlight_row = nav_rows
        .get(selected_idx)
        .copied()
        .map(|r| r.min(flat_items.len().saturating_sub(1)));

    let mut tree_state = ListState::default();
    tree_state.select(highlight_row);
    let tree_list = List::new(flat_items)
        .style(theme::style_sidebar_bg())
        .highlight_style(theme::style_sidebar_selected());
    frame.render_stateful_widget(tree_list, sections[0], &mut tree_state);

    // ── Section 2: tasks ────────────────────────────────────────
    let sep_area = Rect {
        height: 1,
        ..sections[1]
    };
    let tasks_area = Rect {
        y: sections[1].y + 1,
        height: sections[1].height.saturating_sub(1),
        ..sections[1]
    };

    // single-render separator with embedded label
    let sep_total = sep_area.width as usize;
    let label = " Active ";
    let dashes_each = sep_total.saturating_sub(label.len()) / 2;
    let left_dashes = "─".repeat(dashes_each);
    let right_dashes = "─".repeat(sep_total.saturating_sub(dashes_each + label.len()));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left_dashes, theme::style_border()),
            Span::styled(label, theme::style_sidebar_header()),
            Span::styled(right_dashes, theme::style_border()),
        ])),
        sep_area,
    );

    if tasks.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("  idle", theme::style_sidebar_dim())),
            tasks_area,
        );
    } else {
        let task_items: Vec<ListItem> = tasks
            .iter()
            .map(|task| {
                let icon = match task.kind {
                    TaskKind::Plan => "⏳",
                    TaskKind::Inquiry => "◎",
                    TaskKind::Command => "▷",
                };
                let kind_label = match task.kind {
                    TaskKind::Plan => "plan",
                    TaskKind::Inquiry => "inquiry",
                    TaskKind::Command => "cmd",
                };
                let name = if task.filename.len() > 14 {
                    format!("{}…", &task.filename[..13])
                } else {
                    task.filename.clone()
                };
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(format!(" {icon} "), theme::style_orange()),
                        Span::styled(active_lobe_name.to_owned(), theme::style_accent2()),
                        Span::styled(format!(" · {kind_label}"), theme::style_sidebar_dim()),
                    ]),
                    Line::from(vec![
                        Span::styled(format!("   {name}"), theme::style_sidebar_item()),
                    ]),
                ]))
            })
            .collect();
        let list = List::new(task_items).style(theme::style_sidebar_bg());
        frame.render_widget(list, tasks_area);
    }
}

fn render_section_tabs(
    frame: &mut Frame,
    area: Rect,
    active_section: Section,
    pending_review_count: usize,
) {
    let mut spans = vec![Span::styled("──", theme::style_border())];
    push_section_spans(&mut spans, active_section, pending_review_count, Span::styled("──·──", theme::style_border()));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::style_normal()),
        area,
    );
}

fn push_section_spans(
    spans: &mut Vec<Span<'static>>,
    active_section: Section,
    pending_review_count: usize,
    separator: Span<'static>,
) {
    let sections = [
        (
            Section::Review,
            if pending_review_count > 0 {
                format!("Review·{pending_review_count}")
            } else {
                "Review".to_owned()
            },
        ),
        (Section::Log, "Log".to_owned()),
    ];

    for (idx, (section, label)) in sections.into_iter().enumerate() {
        if idx > 0 {
            spans.push(separator.clone());
        }
        if section == Section::Review && pending_review_count > 0 {
            let (prefix, badge) = label.split_once('·').unwrap_or((label.as_str(), ""));
            spans.push(Span::styled(
                prefix.to_owned(),
                if section == active_section {
                    theme::style_section_active()
                } else {
                    theme::style_section_inactive()
                },
            ));
            spans.push(Span::styled(format!("·{badge}"), theme::style_warning()));
        } else {
            spans.push(Span::styled(
                label,
                if section == active_section {
                    theme::style_section_active()
                } else {
                    theme::style_section_inactive()
                },
            ));
        }
    }
}


fn render_content_block(
    frame: &mut Frame,
    area: Rect,
    render_inner: impl FnOnce(&mut Frame, Rect),
) {
    let block = Block::default()
        .border_type(theme::BORDER)
        .borders(Borders::ALL)
        .border_style(theme::style_border())
        .style(theme::style_normal());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_inner(frame, inner);
}

fn render_input_bar(
    frame: &mut Frame,
    area: Rect,
    focus: Focus,
    input_text: &str,
    command_context: Option<&str>,
) {
    let focused = focus == Focus::Input;
    let border_style = if focused {
        theme::style_border_accent()
    } else {
        theme::style_border()
    };

    // Title shows context if set
    let title = if let Some(ctx) = command_context {
        Line::from(vec![
            Span::styled(" Message ", if focused { theme::style_accent() } else { theme::style_input_idle() }),
            Span::styled(format!("[{ctx}] "), theme::style_accent2()),
        ])
    } else {
        Line::from(Span::styled(
            " Message ",
            if focused { theme::style_accent() } else { theme::style_input_idle() },
        ))
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(border_style)
        .title(title)
        .title_bottom(
            Line::from(vec![
                Span::styled(" ←: ctx", theme::style_dim()),
                Span::styled("  esc: back", theme::style_dim()),
                Span::styled("  enter: send ", theme::style_dim()),
            ])
            .right_aligned(),
        )
        .style(theme::style_normal());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = if focused {
        Line::from(vec![
            Span::styled(" > ", theme::style_accent()),
            Span::styled(input_text.to_owned(), theme::style_input_focused()),
            Span::styled("_", theme::style_cursor()),
        ])
    } else if input_text.is_empty() {
        Line::from(Span::styled(
            "  i · type a message...",
            theme::style_input_idle(),
        ))
    } else {
        Line::from(Span::styled(
            format!("  {input_text}"),
            theme::style_input_idle(),
        ))
    };

    frame.render_widget(Paragraph::new(line).style(theme::style_normal()), inner);
}

fn render_reviews(frame: &mut Frame, area: Rect, reviews: &[ReviewItem], selected_idx: usize) {
    if reviews.is_empty() {
        render_empty(frame, area, "No review items");
        return;
    }

    let two_line = area.height as usize >= reviews.len().saturating_mul(2);
    let items: Vec<ListItem> = reviews
        .iter()
        .map(|item| {
            let dot_style = match item.status {
                ReviewStatus::Pending => theme::style_review_pending(),
                ReviewStatus::Accepted => theme::style_review_accepted(),
                ReviewStatus::Rejected => theme::style_review_rejected(),
            };
            let summary = snippet(&item.summary, 100);
            if two_line {
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled("● ", dot_style),
                        Span::styled(item.filename.as_str(), theme::style_normal()),
                    ]),
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(summary, theme::style_dim()),
                    ]),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled("● ", dot_style),
                    Span::styled(item.filename.as_str(), theme::style_normal()),
                    Span::styled("  ", theme::style_dim()),
                    Span::styled(summary, theme::style_dim()),
                ]))
            }
        })
        .collect();

    render_list(frame, area, items, selected_idx);
}

fn render_log(frame: &mut Frame, area: Rect, logs: &[String], scroll: u16) {
    if logs.is_empty() {
        render_empty(frame, area, "No log entries");
        return;
    }

    let lines: Vec<Line> = logs.iter().map(|line| log_line(line)).collect();
    let paragraph = Paragraph::new(Text::from(lines))
        .style(theme::style_normal())
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_list(frame: &mut Frame, area: Rect, items: Vec<ListItem>, selected_idx: usize) {
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(selected_idx.min(items.len().saturating_sub(1))));
    }

    let list = List::new(items)
        .style(theme::style_normal())
        .highlight_style(theme::style_selected());

    frame.render_stateful_widget(list, area, &mut state);
}

pub fn lobe_switcher(frame: &mut Frame, lobes: &[&str], selected_idx: usize) {
    let area = frame.area();

    let height = (lobes.len() as u16 + 4).min(area.height.saturating_sub(4));
    let width = lobes
        .iter()
        .map(|l| l.len() as u16)
        .max()
        .unwrap_or(10)
        .max(20)
        + 6;

    let modal = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };

    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Line::from(Span::styled(" Switch lobe ", theme::style_accent())))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::style_border_accent())
        .style(theme::style_normal());
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = lobes
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let style = if idx == selected_idx {
                theme::style_sidebar_selected()
            } else {
                theme::style_normal()
            };
            let marker = if idx == selected_idx { "▶ " } else { "  " };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{name}"),
                style,
            )))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected_idx.min(lobes.len().saturating_sub(1))));
    let list = List::new(items)
        .style(theme::style_normal())
        .highlight_style(theme::style_sidebar_selected());
    frame.render_stateful_widget(list, rows[0], &mut state);

    KeybindingBar {
        bindings: &[("↑/↓", "nav"), ("enter", "switch"), ("esc", "cancel")],
    }
    .render(rows[1], frame.buffer_mut());
}

pub fn review_detail(frame: &mut Frame, item: &ReviewItem, diff_scroll: u16) {
    let area = frame.area();
    let modal = wide_modal(area);
    frame.render_widget(Clear, modal);

    let kind = match item.kind {
        ReviewKind::Plan => "Plan",
        ReviewKind::Inquiry => "Inquiry",
    };
    let status = match item.status {
        ReviewStatus::Pending => "Pending",
        ReviewStatus::Accepted => "Accepted",
        ReviewStatus::Rejected => "Rejected",
    };
    let title = format!("{}  {kind}  {status}", item.filename);

    let block = Block::default()
        .title(Line::from(Span::styled(title, theme::style_accent())))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::style_border_accent())
        .style(theme::style_normal());
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(inner);

    let summary_block = Block::default()
        .title(Span::styled("Summary", theme::style_dim()))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::style_border())
        .style(theme::style_normal());
    let summary = Paragraph::new(item.summary.as_str())
        .style(theme::style_normal())
        .block(summary_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(summary, rows[0]);

    let diff_text = item
        .diff
        .as_deref()
        .map(truncated_diff)
        .unwrap_or_else(|| "No diff captured".to_owned());
    let diff = Paragraph::new(diff_lines(&diff_text))
        .style(theme::style_normal())
        .block(
            Block::default()
                .title(Span::styled("Diff", theme::style_dim()))
                .borders(Borders::ALL)
                .border_type(theme::BORDER)
                .border_style(theme::style_border())
                .style(theme::style_normal()),
        )
        .scroll((diff_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(diff, rows[1]);

    KeybindingBar {
        bindings: &[("a", "Accept"), ("r", "Reject"), ("esc", "Back")],
    }
    .render(rows[2], frame.buffer_mut());
}

pub fn approval(frame: &mut Frame, request: &ApprovalRequest) {
    let area = frame.area();
    let modal = wide_modal(area);
    frame.render_widget(Clear, modal);

    let (kind_label, title_style, border_style) = match request.kind {
        ApprovalKind::CodeReview => (
            "Code Review",
            theme::style_accent(),
            theme::style_border_accent(),
        ),
        ApprovalKind::Permission => (
            "Permission",
            theme::style_orange().add_modifier(Modifier::BOLD),
            theme::style_orange(),
        ),
    };
    let title = format!("{}: {}", kind_label, request.title);

    let block = Block::default()
        .title(Line::from(Span::styled(title, title_style)))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(border_style)
        .style(theme::style_normal());

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body = Paragraph::new(request.body.as_str())
        .style(theme::style_normal())
        .wrap(Wrap { trim: false });
    frame.render_widget(body, sections[0]);

    KeybindingBar {
        bindings: &[("a", "Accept"), ("r", "Reject"), ("esc", "Cancel")],
    }
    .render(sections[1], frame.buffer_mut());
}

pub fn new_idea_modal(frame: &mut Frame, project_name: &str, input: &str) {
    let area = frame.area();
    let width = (area.width * 60 / 100).max(40).min(area.width);
    let height = 5u16;
    let modal = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" New idea — ", theme::style_accent()),
            Span::styled(project_name.to_owned(), theme::style_accent2()),
            Span::styled(" ", theme::style_accent()),
        ]))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::style_border_accent())
        .style(theme::style_normal());
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let line = Line::from(vec![
        Span::styled(" > ", theme::style_accent()),
        Span::styled(input.to_owned(), theme::style_input_focused()),
        Span::styled("_", theme::style_cursor()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::style_normal()), rows[0]);

    KeybindingBar {
        bindings: &[("enter", "create"), ("esc", "cancel")],
    }
    .render(rows[1], frame.buffer_mut());
}

fn render_empty(frame: &mut Frame, area: Rect, message: &str) {
    let paragraph = Paragraph::new(message)
        .style(theme::style_dim())
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn log_line(value: &str) -> Line<'static> {
    let lower = value.to_lowercase();
    let event_style = if lower.contains("error") || lower.contains("failed") {
        theme::style_error()
    } else if lower.contains("completed") {
        theme::style_success()
    } else if lower.contains("started") {
        theme::style_orange()
    } else {
        theme::style_dim()
    };

    if let Some(rest) = value.strip_prefix('[') {
        if let Some((lobe, message)) = rest.split_once("] ") {
            return Line::from(vec![
                Span::styled(format!("[{lobe}]"), theme::style_accent2()),
                Span::styled(format!(" {message}"), event_style),
            ]);
        }
    }

    Line::from(Span::styled(value.to_owned(), event_style))
}

fn diff_lines(diff: &str) -> Text<'static> {
    Text::from(
        diff.lines()
            .map(|line| {
                let style = if line.starts_with('+') {
                    theme::style_success()
                } else if line.starts_with('-') {
                    theme::style_error()
                } else if line.starts_with("@@") {
                    theme::style_accent()
                } else {
                    theme::style_dim()
                };
                Line::from(Span::styled(line.to_owned(), style))
            })
            .collect::<Vec<_>>(),
    )
}

fn wide_modal(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(5),
            Constraint::Percentage(90),
            Constraint::Percentage(5),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(2),
            Constraint::Percentage(96),
            Constraint::Percentage(2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

fn truncate_name(name: &str, max: usize) -> String {
    if max == 0 { return String::new(); }
    if name.len() <= max { name.to_owned() } else { format!("{}…", &name[..max.saturating_sub(1)]) }
}

fn snippet(value: &str, max_chars: usize) -> String {
    let mut chars = value.trim().chars();
    let snippet: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{snippet}...")
    } else {
        snippet
    }
}

fn truncated_diff(diff: &str) -> String {
    let mut lines: Vec<&str> = diff.lines().take(200).collect();
    if diff.lines().count() > 200 {
        lines.push("... truncated to 200 lines");
    }
    lines.join("\n")
}
