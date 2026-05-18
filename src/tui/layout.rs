use std::collections::HashSet;

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
    ApprovalKind, ApprovalRequest, Focus, InputMode, PlanFileEntry, PlanFileKind, PlanFileState,
    ReviewItem, ReviewKind, ReviewStatus, Section, TaskEntry, TaskKind,
};

#[derive(Debug, Clone)]
pub struct SidebarProject {
    pub name: String,
    pub path: std::path::PathBuf,
    pub folders: Vec<String>, // plans, inquiries, ideas, tasks — exist on disk
    pub components: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub lobe: String,
    pub path: std::path::PathBuf,
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
    active_agent_count: usize,
    daemon_connected: bool,
    tasks: &[TaskEntry],
    plan_files: &[PlanFileEntry],
    plan_selected_idx: usize,
    inquiry_files: &[PlanFileEntry],
    inquiry_selected_idx: usize,
    reviews: &[ReviewItem],
    review_selected_idx: usize,
    logs: &[String],
    log_selected_idx: usize,
    input_text: &str,
    active_lobe_name: &str,
    command_context: Option<&str>,
    input_mode: InputMode,
    tick: u64,
    marked_components: &HashSet<String>,
) {
    let area = frame.area();

    let input_height = input_bar_height(input_text, area.width);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(1)])
        .split(rows[0]);

    render_sidebar(
        frame,
        columns[0],
        sidebar_items,
        sidebar_selected_idx,
        active_agent_count,
        focus,
        active_lobe_name,
        tick,
        marked_components,
    );

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height),
        ])
        .split(columns[1]);

    render_header(frame, right_rows[0], active_lobe, daemon_connected);
    render_section_tabs(
        frame,
        right_rows[1],
        active_section,
        pending_review_count,
        active_agent_count,
    );

    match active_section {
        Section::Agents => render_content_block(frame, right_rows[2], |frame, inner| {
            render_agents(frame, inner, tasks, tick);
        }),
        Section::Plans => render_content_block(frame, right_rows[2], |frame, inner| {
            render_work_files(
                frame,
                inner,
                plan_files,
                plan_selected_idx,
                "No plan files found",
            );
        }),
        Section::Inquiries => render_content_block(frame, right_rows[2], |frame, inner| {
            render_work_files(
                frame,
                inner,
                inquiry_files,
                inquiry_selected_idx,
                "No inquiry files found",
            );
        }),
        Section::Review => render_content_block(frame, right_rows[2], |frame, inner| {
            render_reviews(frame, inner, reviews, review_selected_idx);
        }),
        Section::Log => render_content_block(frame, right_rows[2], |frame, inner| {
            render_log(frame, inner, logs, log_selected_idx);
        }),
    }

    let sep = "─".repeat(right_rows[3].width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(theme::style_border()),
        right_rows[3],
    );
    render_input_bar(
        frame,
        right_rows[4],
        focus,
        input_text,
        command_context,
        input_mode,
    );

    let bindings: &[(&str, &str)] = if focus == Focus::Sidebar {
        &[
            ("↑/↓", "nav"),
            ("enter", "ctx"),
            ("spc", "mark"),
            ("n", "idea"),
            ("p/I", "plan/inq"),
            ("q", "quit"),
        ]
    } else if active_section == Section::Review {
        &[
            ("↑/↓", "nav"),
            ("tab", "section"),
            ("enter", "open"),
            ("a/r", "accept/reject"),
            ("i", "input"),
            ("q", "quit"),
        ]
    } else if active_section == Section::Plans {
        &[
            ("↑/↓", "nav"),
            ("tab", "section"),
            ("x", "execute"),
            ("i", "input"),
            ("←", "sidebar"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("↑/↓", "nav"),
            ("tab", "section"),
            ("i", "input"),
            ("←", "sidebar"),
            ("spc", "lobes"),
            ("q", "quit"),
        ]
    };
    KeybindingBar { bindings }.render(rows[1], frame.buffer_mut());
}

fn render_header(frame: &mut Frame, area: Rect, active_lobe: &str, daemon_connected: bool) {
    let conn_symbol = if daemon_connected { "●" } else { "○" };
    let conn_style = if daemon_connected {
        theme::style_success()
    } else {
        theme::style_dim()
    };

    let left = vec![
        Span::styled("  ◆ ", theme::style_accent()),
        Span::styled("cortex", theme::style_accent()),
        Span::styled("  ╱  ", theme::style_border()),
        Span::styled(active_lobe.to_owned(), theme::style_accent2()),
    ];
    let right = vec![
        Span::styled(conn_symbol, conn_style),
        Span::styled("  ", theme::style_dim()),
    ];

    let left_width: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let right_width: usize = right.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(left_width + right_width);

    let mut spans = left;
    spans.push(Span::styled(" ".repeat(pad), theme::style_normal()));
    spans.extend(right);

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
    running_count: usize,
    focus: Focus,
    active_lobe_name: &str,
    tick: u64,
    marked_components: &HashSet<String>,
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
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let Some(item) = items.first() else {
        return;
    };
    let w = inner.width as usize;
    let is_focused = focus == Focus::Sidebar;
    let mut flat_items: Vec<ListItem> = Vec::new();
    let mut nav_rows: Vec<usize> = Vec::new();

    flat_items.push(ListItem::new(Line::from(Span::styled(
        " WORKSPACE",
        theme::style_sidebar_dim(),
    ))));

    let is_lobe_sel = nav_rows.len() == selected_idx;
    let lobe_marker = if is_lobe_sel { "›" } else { " " };
    let lobe_style = if is_lobe_sel && is_focused {
        theme::style_sidebar_selected()
    } else {
        theme::style_sidebar_header()
    };
    nav_rows.push(flat_items.len());
    flat_items.push(ListItem::new(Line::from(Span::styled(
        format!(
            " {lobe_marker} ◆ {}",
            truncate_name(&item.lobe, w.saturating_sub(6))
        ),
        lobe_style,
    ))));

    flat_items.push(ListItem::new(Line::from(Span::styled(
        "",
        theme::style_sidebar_dim(),
    ))));
    flat_items.push(ListItem::new(Line::from(Span::styled(
        " PROJECTS",
        theme::style_sidebar_dim(),
    ))));

    for project in &item.projects {
        let is_proj_sel = nav_rows.len() == selected_idx;
        let proj_marker = if is_proj_sel { "›" } else { " " };
        let proj_style = if is_proj_sel && is_focused {
            theme::style_sidebar_selected()
        } else if is_proj_sel {
            theme::style_accent2()
        } else {
            theme::style_sidebar_item()
        };

        nav_rows.push(flat_items.len());
        flat_items.push(ListItem::new(Line::from(Span::styled(
            format!(
                " {proj_marker} ▣ {}",
                truncate_name(&project.name, w.saturating_sub(6))
            ),
            proj_style,
        ))));

        if !project.components.is_empty() {
            flat_items.push(ListItem::new(Line::from(Span::styled(
                "     components",
                theme::style_sidebar_dim(),
            ))));
        }

        for comp in &project.components {
            let is_comp_sel = nav_rows.len() == selected_idx;
            let is_marked = marked_components.contains(comp.as_str());
            let cursor = if is_comp_sel { "›" } else { " " };
            let mark = if is_marked { "◆" } else { "◇" };
            let comp_style = if is_comp_sel && is_focused {
                theme::style_sidebar_selected()
            } else if is_marked {
                theme::style_accent()
            } else if is_comp_sel {
                theme::style_accent2()
            } else {
                theme::style_sidebar_dim()
            };

            nav_rows.push(flat_items.len());
            flat_items.push(ListItem::new(Line::from(Span::styled(
                format!(
                    " {cursor}   {mark} {}",
                    truncate_name(comp, w.saturating_sub(9))
                ),
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
    frame.render_stateful_widget(
        List::new(flat_items)
            .style(theme::style_sidebar_bg())
            .highlight_style(theme::style_sidebar_selected()),
        sections[0],
        &mut tree_state,
    );

    // Status footer
    let footer = if running_count > 0 {
        let spin = spinner_char(tick);
        Line::from(vec![
            Span::styled(format!(" {spin} "), theme::style_accent()),
            Span::styled(
                format!("{running_count} running · {active_lobe_name}"),
                theme::style_orange(),
            ),
        ])
    } else {
        Line::from(Span::styled(
            format!("  ○  idle · {active_lobe_name}"),
            theme::style_sidebar_dim(),
        ))
    };
    frame.render_widget(Paragraph::new(footer), sections[1]);
}

fn render_section_tabs(
    frame: &mut Frame,
    area: Rect,
    active_section: Section,
    pending_review_count: usize,
    active_agent_count: usize,
) {
    let mut spans = vec![Span::styled("──", theme::style_border())];
    push_section_spans(
        &mut spans,
        active_section,
        pending_review_count,
        active_agent_count,
        Span::styled("──·──", theme::style_border()),
    );

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::style_normal()),
        area,
    );
}

fn push_section_spans(
    spans: &mut Vec<Span<'static>>,
    active_section: Section,
    pending_review_count: usize,
    active_agent_count: usize,
    separator: Span<'static>,
) {
    struct Tab {
        section: Section,
        label: String,
        badge: Option<String>,
    }

    let tabs = [
        Tab {
            section: Section::Agents,
            label: "Agents".to_owned(),
            badge: if active_agent_count > 0 {
                Some(active_agent_count.to_string())
            } else {
                None
            },
        },
        Tab {
            section: Section::Plans,
            label: "Plans".to_owned(),
            badge: None,
        },
        Tab {
            section: Section::Inquiries,
            label: "Inquiries".to_owned(),
            badge: None,
        },
        Tab {
            section: Section::Review,
            label: "Review".to_owned(),
            badge: if pending_review_count > 0 {
                Some(pending_review_count.to_string())
            } else {
                None
            },
        },
        Tab {
            section: Section::Log,
            label: "Log".to_owned(),
            badge: None,
        },
    ];

    for (idx, tab) in tabs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(separator.clone());
        }
        let is_active = tab.section == active_section;
        let label_style = if is_active {
            theme::style_section_active()
        } else {
            theme::style_section_inactive()
        };
        spans.push(Span::styled(tab.label, label_style));
        if let Some(badge) = tab.badge {
            spans.push(Span::styled(format!("·{badge}"), theme::style_warning()));
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
    input_mode: InputMode,
) {
    let focused = focus == Focus::Input;
    let border_style = if focused {
        theme::style_border_accent()
    } else {
        theme::style_border()
    };

    let mode_style = if focused {
        match input_mode {
            InputMode::Command => theme::style_accent2(),
            InputMode::NewPlan => theme::style_orange(),
            InputMode::NewInquiry => theme::style_accent(),
        }
    } else {
        theme::style_dim()
    };

    let mut title_spans = vec![Span::styled(
        format!(" {} ", input_mode.label()),
        mode_style,
    )];
    if let Some(ctx) = command_context {
        title_spans.push(Span::styled("╱ ", theme::style_border()));
        title_spans.push(Span::styled(ctx.to_owned(), theme::style_accent2()));
        title_spans.push(Span::styled(" ", theme::style_dim()));
    }

    let action_hint = match input_mode {
        InputMode::Command => "send",
        InputMode::NewPlan => "create plan",
        InputMode::NewInquiry => "create inquiry",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(border_style)
        .title(Line::from(title_spans))
        .title_bottom(
            Line::from(vec![
                Span::styled(" tab: mode", theme::style_dim()),
                Span::styled("  ←: ctx", theme::style_dim()),
                Span::styled("  esc: back", theme::style_dim()),
                Span::styled(format!("  enter: {action_hint} "), theme::style_dim()),
            ])
            .right_aligned(),
        )
        .style(theme::style_normal());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = if focused {
        let placeholder = match input_mode {
            InputMode::Command => "type a message...",
            InputMode::NewPlan => "plan title...",
            InputMode::NewInquiry => "inquiry title...",
        };
        if input_text.is_empty() {
            Line::from(vec![
                Span::styled(" > ", theme::style_accent()),
                Span::styled(placeholder, theme::style_dim()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" > ", theme::style_accent()),
                Span::styled(input_text.to_owned(), theme::style_input_focused()),
                Span::styled("_", theme::style_cursor()),
            ])
        }
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

    frame.render_widget(
        Paragraph::new(line)
            .style(theme::style_normal())
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn input_bar_height(input_text: &str, width: u16) -> u16 {
    let usable_width = width.saturating_sub(10).max(24) as usize;
    let lines = input_text.chars().count().div_ceil(usable_width).max(1);
    (lines as u16 + 2).clamp(3, 7)
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

fn render_work_files(
    frame: &mut Frame,
    area: Rect,
    entries: &[PlanFileEntry],
    selected_idx: usize,
    empty_msg: &str,
) {
    if entries.is_empty() {
        render_empty(frame, area, empty_msg);
        return;
    }

    let selected_idx = selected_idx.min(entries.len().saturating_sub(1));
    let split_direction = if area.width >= 100 {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let sections = Layout::default()
        .direction(split_direction)
        .constraints(match split_direction {
            Direction::Horizontal => [Constraint::Percentage(42), Constraint::Percentage(58)],
            Direction::Vertical => [Constraint::Percentage(42), Constraint::Percentage(58)],
        })
        .split(area);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let (dot, dot_style) = match entry.state {
                PlanFileState::Active => ("⠿", theme::style_accent()),
                PlanFileState::Completed => ("●", theme::style_success()),
                PlanFileState::Pending => ("○", theme::style_dim()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {dot}  "), dot_style),
                Span::styled(entry.project.as_str(), theme::style_dim()),
                Span::styled("  ", theme::style_dim()),
                Span::styled(entry.filename.as_str(), theme::style_normal()),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected_idx));
    let list = List::new(items)
        .style(theme::style_normal())
        .highlight_style(theme::style_selected());
    frame.render_stateful_widget(list, sections[0], &mut state);

    if let Some(entry) = entries.get(selected_idx) {
        let content = std::fs::read_to_string(&entry.path)
            .unwrap_or_else(|err| format!("Could not read {}: {err}", entry.path.display()));
        let title = format!("{} / {}", entry.project, entry.filename);
        let preview = Paragraph::new(markdown_lines(&content))
            .block(
                Block::default()
                    .title(Line::from(vec![
                        Span::styled(" Preview ", theme::style_accent2()),
                        Span::styled(title, theme::style_dim()),
                    ]))
                    .borders(Borders::LEFT)
                    .border_type(theme::BORDER)
                    .border_style(theme::style_border_accent())
                    .style(theme::style_normal()),
            )
            .style(theme::style_normal())
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, sections[1]);
    }
}

fn render_log(frame: &mut Frame, area: Rect, logs: &[String], selected_idx: usize) {
    if logs.is_empty() {
        render_empty(frame, area, "No log entries");
        return;
    }

    let items: Vec<ListItem> = logs
        .iter()
        .map(|line| ListItem::new(log_line(line)))
        .collect();
    let clamped = selected_idx.min(items.len().saturating_sub(1));
    let mut state = ListState::default();
    state.select(Some(clamped));

    let list = List::new(items)
        .style(theme::style_normal())
        .highlight_style(theme::style_selected());

    frame.render_stateful_widget(list, area, &mut state);
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
        .title(Line::from(Span::styled(
            " Switch lobe ",
            theme::style_accent(),
        )))
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
            ListItem::new(Line::from(Span::styled(format!("{marker}{name}"), style)))
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

fn spinner_char(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick / 3) as usize % FRAMES.len()]
}

fn render_agents(frame: &mut Frame, area: Rect, tasks: &[TaskEntry], tick: u64) {
    if tasks.is_empty() {
        render_pipeline(frame, area);
        return;
    }

    let spin = spinner_char(tick);

    let items: Vec<ListItem> = tasks
        .iter()
        .map(|task| {
            let (kind_label, kind_style) = match task.kind {
                TaskKind::Plan => ("plan   ", theme::style_accent2()),
                TaskKind::Inquiry => ("inquiry", theme::style_accent()),
                TaskKind::Command => ("cmd    ", theme::style_orange()),
            };
            let name = truncate_name(&task.filename, area.width.saturating_sub(16) as usize);
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {spin}  "), theme::style_accent()),
                Span::styled(kind_label, kind_style),
                Span::styled("  ", theme::style_dim()),
                Span::styled(name, theme::style_normal()),
            ]))
        })
        .collect();

    let list = List::new(items).style(theme::style_normal());
    frame.render_widget(list, area);
}

fn render_pipeline(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  01 ", theme::style_teal()),
            Span::styled("watch", theme::style_accent2()),
            Span::styled("  vault files and socket events", theme::style_dim()),
        ]),
        Line::from(vec![
            Span::styled("  02 ", theme::style_blue()),
            Span::styled("route", theme::style_accent()),
            Span::styled("  choose Claude, Codex or Gemini", theme::style_dim()),
        ]),
        Line::from(vec![
            Span::styled("  03 ", theme::style_orange()),
            Span::styled("execute", theme::style_orange()),
            Span::styled(" provider task through Cortex router", theme::style_dim()),
        ]),
        Line::from(vec![
            Span::styled("  04 ", theme::style_success()),
            Span::styled("review", theme::style_success()),
            Span::styled("  inspect output, approve or reroute", theme::style_dim()),
        ]),
    ];

    let paragraph = Paragraph::new(Text::from(lines))
        .alignment(ratatui::layout::Alignment::Left)
        .style(theme::style_normal());
    frame.render_widget(paragraph, center_vertically(area, 4));
}

/// Returns a horizontally-full, vertically-centered rect of `lines` height within `area`.
fn center_vertically(area: Rect, lines: u16) -> Rect {
    let offset = area.height.saturating_sub(lines) / 2;
    Rect {
        y: area.y + offset,
        height: lines,
        ..area
    }
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

pub fn new_work_file_modal(
    frame: &mut Frame,
    project_name: &str,
    kind: &PlanFileKind,
    active_field: usize,
    title: &str,
    description: &str,
) {
    let area = frame.area();
    let width = (area.width * 65 / 100).max(45).min(area.width);
    let height = 7u16;
    let modal = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);

    let kind_label = match kind {
        super::PlanFileKind::Plan => "New plan",
        super::PlanFileKind::Inquiry => "New inquiry",
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(format!(" {} — ", kind_label), theme::style_accent()),
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
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let make_field = |label: &'static str,
                      value: &str,
                      is_active: bool,
                      placeholder: &'static str|
     -> Line<'static> {
        let prefix_style = if is_active {
            theme::style_accent()
        } else {
            theme::style_dim()
        };
        let value = value.to_owned();
        if value.is_empty() {
            Line::from(vec![
                Span::styled(format!(" {label}: "), prefix_style),
                Span::styled(placeholder, theme::style_dim()),
            ])
        } else if is_active {
            Line::from(vec![
                Span::styled(format!(" {label}: "), prefix_style),
                Span::styled(value, theme::style_input_focused()),
                Span::styled("_", theme::style_cursor()),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!(" {label}: "), prefix_style),
                Span::styled(value, theme::style_normal()),
            ])
        }
    };

    frame.render_widget(
        Paragraph::new(make_field("title", title, active_field == 0, "required"))
            .style(theme::style_normal()),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(make_field(
            "desc ",
            description,
            active_field == 1,
            "optional",
        ))
        .style(theme::style_normal()),
        rows[2],
    );

    KeybindingBar {
        bindings: &[
            ("tab", "next field"),
            ("enter", "create"),
            ("esc", "cancel"),
        ],
    }
    .render(rows[4], frame.buffer_mut());
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

fn markdown_lines(content: &str) -> Text<'static> {
    let mut in_frontmatter = false;
    let mut lines = Vec::new();

    for raw in content.lines().take(240) {
        let line = raw.trim_end();

        if line == "---" {
            in_frontmatter = !in_frontmatter;
            lines.push(Line::from(Span::styled(
                line.to_owned(),
                theme::style_dim(),
            )));
            continue;
        }

        if in_frontmatter {
            lines.push(Line::from(Span::styled(
                line.to_owned(),
                theme::style_dim(),
            )));
        } else if line.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches("# ").to_owned(),
                theme::style_accent(),
            )));
        } else if line.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                line.trim_start_matches("## ").to_owned(),
                theme::style_accent2(),
            )));
        } else if line.starts_with("- [ ]") {
            lines.push(Line::from(vec![
                Span::styled("  □ ", theme::style_warning()),
                Span::styled(
                    line.trim_start_matches("- [ ]").trim().to_owned(),
                    theme::style_normal(),
                ),
            ]));
        } else if line.starts_with("- [x]") || line.starts_with("- [X]") {
            lines.push(Line::from(vec![
                Span::styled("  ■ ", theme::style_success()),
                Span::styled(line[5..].trim().to_owned(), theme::style_dim()),
            ]));
        } else if let Some(item) = line.strip_prefix("- ") {
            lines.push(Line::from(vec![
                Span::styled("  • ", theme::style_orange()),
                Span::styled(item.to_owned(), theme::style_normal()),
            ]));
        } else if let Some(quote) = line.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("  │ ", theme::style_border_accent()),
                Span::styled(quote.to_owned(), theme::style_dim()),
            ]));
        } else if line.trim().is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(Span::styled(
                line.to_owned(),
                theme::style_normal(),
            )));
        }
    }

    Text::from(lines)
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
    if max == 0 {
        return String::new();
    }
    if name.len() <= max {
        name.to_owned()
    } else {
        format!("{}…", &name[..max.saturating_sub(1)])
    }
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
