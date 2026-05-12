pub mod layout;
pub mod theme;
pub mod widgets;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEventKind},
    execute,
    style::{ResetColor, SetBackgroundColor},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::{mpsc, oneshot};

use crate::config::Lobe;
use crate::ipc::TuiMessage;

pub struct ApprovalRequest {
    pub kind: ApprovalKind,
    pub title: String,
    pub body: String,
    pub tx: oneshot::Sender<ApprovalResponse>,
}

pub enum ApprovalKind {
    CodeReview,
    Permission,
}

pub enum ApprovalResponse {
    Accepted,
    Rejected,
}

enum Screen {
    Main,
    LobeSwitcher { selected_idx: usize },
    ReviewDetail(ReviewItem),
    PendingApproval(ApprovalRequest),
    NewIdea,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    Agents,
    Review,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    Content,
    Input,
}

#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub id: String,
    pub lobe: String,
    pub filename: String,
    pub kind: ReviewKind,
    pub summary: String,
    pub diff: Option<String>,
    pub status: ReviewStatus,
}

#[derive(Debug, Clone)]
pub enum ReviewKind {
    Plan,
    Inquiry,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewStatus {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskKind {
    Plan,
    Inquiry,
    Command,
}

#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub kind: TaskKind,
    pub filename: String,
}

struct NewIdeaState {
    vault_path: std::path::PathBuf,
    project_name: String,
    input: String,
}

pub struct Tui {
    requests: mpsc::Receiver<ApprovalRequest>,
    lobes: Vec<Lobe>,
    pub active_lobe_idx: usize,
    pub active_section: Section,
    pub review_items: Vec<ReviewItem>,
    pub log_by_lobe: HashMap<String, Vec<String>>,
    tasks_by_lobe: HashMap<String, Vec<TaskEntry>>,
    focus: Focus,
    input_text: String,
    /// Index within the active lobe's navigable items (components)
    sidebar_selected_idx: usize,
    /// Currently selected component/project context for commands
    command_context: Option<String>,
    review_selected_idx: usize,
    log_scroll: u16,
    detail_scroll: u16,
    events_rx: Option<mpsc::Receiver<String>>,
    review_rx: Option<mpsc::Receiver<ReviewItem>>,
    command_output_rx: Option<mpsc::Receiver<(String, String)>>,
    msg_tx: Option<mpsc::Sender<TuiMessage>>,
    daemon_connected: bool,
    new_idea_state: Option<NewIdeaState>,
    tick: u64,
}

impl Tui {
    pub fn new(
        requests: mpsc::Receiver<ApprovalRequest>,
        lobes: Vec<Lobe>,
        events_rx: Option<mpsc::Receiver<String>>,
        review_rx: Option<mpsc::Receiver<ReviewItem>>,
        command_output_rx: Option<mpsc::Receiver<(String, String)>>,
        msg_tx: Option<mpsc::Sender<TuiMessage>>,
    ) -> Self {
        let daemon_connected = events_rx.is_some();
        let log_by_lobe = lobes.iter().map(|l| (l.name.clone(), Vec::new())).collect();
        let tasks_by_lobe = lobes.iter().map(|l| (l.name.clone(), Vec::new())).collect();

        Self {
            requests,
            lobes,
            active_lobe_idx: 0,
            active_section: Section::Agents,
            review_items: Vec::new(),
            log_by_lobe,
            tasks_by_lobe,
            focus: Focus::Content,
            input_text: String::new(),
            sidebar_selected_idx: 0,
            command_context: None,
            review_selected_idx: 0,
            log_scroll: 0,
            detail_scroll: 0,
            events_rx,
            review_rx,
            command_output_rx,
            msg_tx,
            daemon_connected,
            new_idea_state: None,
            tick: 0,
        }
    }

    pub async fn run(mut self) -> Result<()> {

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            SetBackgroundColor(theme::crossterm_bg()),
            Clear(ClearType::All)
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            ResetColor
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let mut screen = Screen::Main;

        loop {
            let active_lobe = self.active_lobe_name();
            let sidebar_items = self.sidebar_items(); // filesystem read — do once per frame
            self.clamp_selection_with(&sidebar_items);
            let active_tasks = self.active_tasks();
            let active_reviews = self.active_reviews();
            let active_logs = self.active_logs();
            let pending_review_count = active_reviews
                .iter()
                .filter(|item| item.status == ReviewStatus::Pending)
                .count();
            let active_agent_count = active_tasks.len();
            self.tick = self.tick.wrapping_add(1);

            terminal.draw(|frame| {
                let area = frame.area();
                frame.render_widget(
                    ratatui::widgets::Block::default().style(theme::style_normal()),
                    area,
                );

                match &screen {
                    Screen::LobeSwitcher { selected_idx } => {
                        layout::main_screen(
                            frame,
                            self.focus,
                            &active_lobe,
                            &sidebar_items,
                            self.sidebar_selected_idx,
                            self.active_section,
                            pending_review_count,
                            active_agent_count,
                            self.daemon_connected,
                            &active_tasks,
                            &active_reviews,
                            self.review_selected_idx,
                            &active_logs,
                            self.log_scroll,
                            &self.input_text,
                            &active_lobe,
                            self.command_context.as_deref(),
                            self.tick,
                        );
                        let lobe_names: Vec<&str> =
                            self.lobes.iter().map(|l| l.name.as_str()).collect();
                        layout::lobe_switcher(frame, &lobe_names, *selected_idx);
                    }
                    Screen::Main => {
                        layout::main_screen(
                            frame,
                            self.focus,
                            &active_lobe,
                            &sidebar_items,
                            self.sidebar_selected_idx,
                            self.active_section,
                            pending_review_count,
                            active_agent_count,
                            self.daemon_connected,
                            &active_tasks,
                            &active_reviews,
                            self.review_selected_idx,
                            &active_logs,
                            self.log_scroll,
                            &self.input_text,
                            &active_lobe,
                            self.command_context.as_deref(),
                            self.tick,
                        );
                    }
                    Screen::ReviewDetail(item) => {
                        layout::review_detail(frame, item, self.detail_scroll);
                    }
                    Screen::PendingApproval(request) => {
                        layout::approval(frame, request);
                    }
                    Screen::NewIdea => {
                        layout::main_screen(
                            frame,
                            self.focus,
                            &active_lobe,
                            &sidebar_items,
                            self.sidebar_selected_idx,
                            self.active_section,
                            pending_review_count,
                            active_agent_count,
                            self.daemon_connected,
                            &active_tasks,
                            &active_reviews,
                            self.review_selected_idx,
                            &active_logs,
                            self.log_scroll,
                            &self.input_text,
                            &active_lobe,
                            self.command_context.as_deref(),
                            self.tick,
                        );
                        if let Some(ref state) = self.new_idea_state {
                            layout::new_idea_modal(frame, &state.project_name, &state.input);
                        }
                    }
                }
            })?;

            if crossterm::event::poll(Duration::from_millis(16))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if self.handle_key(key.code, &mut screen, &sidebar_items).await? {
                        return Ok(());
                    }
                }
            }

            self.drain_events();
            self.drain_command_outputs();
            if self.drain_review_items() && matches!(screen, Screen::Main) {
                self.active_section = Section::Review;
                let count = self.active_reviews().len();
                if count > 0 {
                    self.review_selected_idx = count - 1;
                }
            }

            if matches!(screen, Screen::Main) {
                if let Ok(request) = self.requests.try_recv() {
                    screen = Screen::PendingApproval(request);
                }
            }
        }
    }

    async fn handle_key(&mut self, code: KeyCode, screen: &mut Screen, sidebar: &[layout::SidebarItem]) -> Result<bool> {
        match screen {
            Screen::LobeSwitcher { selected_idx } => {
                match code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        *screen = Screen::Main;
                    }
                    KeyCode::Up => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected_idx + 1 < self.lobes.len() {
                            *selected_idx += 1;
                        }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let idx = *selected_idx;
                        self.active_lobe_idx = idx;
                        self.sidebar_selected_idx = idx;
                        self.reset_cursors();
                        *screen = Screen::Main;
                    }
                    _ => {}
                }
                Ok(false)
            }
            Screen::Main => self.handle_main_key(code, screen, sidebar).await,
            Screen::ReviewDetail(item) => {
                match code {
                    KeyCode::Esc => {
                        *screen = Screen::Main;
                    }
                    KeyCode::Up => {
                        self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        self.detail_scroll = self.detail_scroll.saturating_add(1);
                    }
                    KeyCode::Char('a') => {
                        item.status = ReviewStatus::Accepted;
                        self.set_review_status(&item.id, ReviewStatus::Accepted);
                    }
                    KeyCode::Char('r') => {
                        item.status = ReviewStatus::Rejected;
                        self.set_review_status(&item.id, ReviewStatus::Rejected);
                    }
                    _ => {}
                }
                Ok(false)
            }
            Screen::NewIdea => {
                match code {
                    KeyCode::Esc => {
                        self.new_idea_state = None;
                        *screen = Screen::Main;
                    }
                    KeyCode::Backspace => {
                        if let Some(ref mut state) = self.new_idea_state {
                            state.input.pop();
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(state) = self.new_idea_state.take() {
                            let title = state.input.trim().to_owned();
                            if !title.is_empty() {
                                let lobe = self.active_lobe_name();
                                match create_idea_file(&state.vault_path, &title) {
                                    Ok(path) => {
                                        self.push_log(&lobe, format!("✓ idea: {}", path.file_name().unwrap_or_default().to_string_lossy()));
                                    }
                                    Err(e) => {
                                        self.push_log(&lobe, format!("⚠ idea error: {e}"));
                                    }
                                }
                            }
                            *screen = Screen::Main;
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Some(ref mut state) = self.new_idea_state {
                            state.input.push(c);
                        }
                    }
                    _ => {}
                }
                Ok(false)
            }
            Screen::PendingApproval(_) => {
                let response = match code {
                    KeyCode::Char('a') => Some(ApprovalResponse::Accepted),
                    KeyCode::Char('r') | KeyCode::Char('q') | KeyCode::Esc => {
                        Some(ApprovalResponse::Rejected)
                    }
                    _ => None,
                };
                if let Some(response) = response {
                    if let Screen::PendingApproval(request) =
                        std::mem::replace(screen, Screen::Main)
                    {
                        let _ = request.tx.send(response);
                    }
                }
                Ok(false)
            }
        }
    }

    async fn handle_main_key(&mut self, code: KeyCode, screen: &mut Screen, sidebar: &[layout::SidebarItem]) -> Result<bool> {
        if self.focus == Focus::Input {
            match code {
                KeyCode::Esc => self.focus = Focus::Content,
                KeyCode::Backspace => {
                    self.input_text.pop();
                }
                KeyCode::Enter => {
                    self.submit_input_command();
                    self.focus = Focus::Content;
                }
                KeyCode::Char(c) => {
                    self.input_text.push(c);
                }
                _ => {}
            }
            return Ok(false);
        }

        match code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char(' ') => {
                *screen = Screen::LobeSwitcher {
                    selected_idx: self.active_lobe_idx,
                };
            }
            KeyCode::Char('i') | KeyCode::Char('/') => {
                self.focus = Focus::Input;
            }
            KeyCode::Left => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Right => {
                self.focus = Focus::Content;
            }
            KeyCode::Esc => {
                self.focus = Focus::Content;
            }
            KeyCode::Enter if self.focus == Focus::Sidebar => {
                let names = Self::nav_names_from_sidebar(sidebar);
                if let Some(name) = names.get(self.sidebar_selected_idx) {
                    let ctx = name.clone();
                    if self.command_context.as_deref() == Some(&ctx) {
                        self.command_context = None;
                    } else {
                        self.command_context = Some(ctx);
                    }
                }
                self.focus = Focus::Input;
            }
            KeyCode::Up if self.focus == Focus::Sidebar => {
                self.move_sidebar_selection(-1, sidebar);
            }
            KeyCode::Down if self.focus == Focus::Sidebar => {
                self.move_sidebar_selection(1, sidebar);
            }
            KeyCode::Tab if self.focus == Focus::Content => {
                self.active_section = next_section(self.active_section);
            }
            KeyCode::BackTab if self.focus == Focus::Content => {
                self.active_section = previous_section(self.active_section);
            }
            KeyCode::Up if self.focus == Focus::Content => {
                self.move_selection(-1);
            }
            KeyCode::Down if self.focus == Focus::Content => {
                self.move_selection(1);
            }
            KeyCode::Enter => {
                if self.focus == Focus::Content && self.active_section == Section::Review {
                    if let Some(item) = self.selected_review().cloned() {
                        self.detail_scroll = 0;
                        *screen = Screen::ReviewDetail(item);
                    }
                }
            }
            KeyCode::Char('a') if self.focus == Focus::Content && self.active_section == Section::Review => {
                self.set_selected_review_status(ReviewStatus::Accepted);
            }
            KeyCode::Char('r') if self.focus == Focus::Content && self.active_section == Section::Review => {
                self.set_selected_review_status(ReviewStatus::Rejected);
            }
            KeyCode::Char('n') if self.focus == Focus::Sidebar => {
                if let Some((project_name, vault_path)) = Self::selected_project_info(sidebar, self.sidebar_selected_idx) {
                    self.new_idea_state = Some(NewIdeaState { vault_path, project_name, input: String::new() });
                    *screen = Screen::NewIdea;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn drain_events(&mut self) {
        let mut lines = Vec::new();
        if let Some(ref mut rx) = self.events_rx {
            while let Ok(line) = rx.try_recv() {
                lines.push(line);
            }
        }

        for line in lines {
            let (lobe, message) = split_lobe_prefix(&line);
            self.update_tasks(&lobe, &message);
            self.push_log(&lobe, line);
        }
    }

    fn drain_review_items(&mut self) -> bool {
        let mut found_new = false;
        let mut items = Vec::new();
        if let Some(ref mut rx) = self.review_rx {
            while let Ok(item) = rx.try_recv() {
                items.push(item);
            }
        }

        for item in items {
            if let Some(existing) = self.review_items.iter_mut().find(|r| r.id == item.id) {
                *existing = item;
            } else {
                self.review_items.push(item);
                found_new = true;
            }
        }
        found_new
    }

    fn drain_command_outputs(&mut self) {
        let mut outputs = Vec::new();
        if let Some(ref mut rx) = self.command_output_rx {
            while let Ok(output) = rx.try_recv() {
                outputs.push(output);
            }
        }

        for (lobe, text) in outputs {
            self.push_log(&lobe, format!("Command output: {text}"));
        }
    }

    fn active_lobe_name(&self) -> String {
        self.lobes
            .get(self.active_lobe_idx)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    fn sidebar_items(&self) -> Vec<layout::SidebarItem> {
        let active_name = self.lobes.get(self.active_lobe_idx).map(|l| l.name.as_str()).unwrap_or("");
        self.lobes.iter().map(|lobe| {
            let projects = if let Some(ref root) = lobe.projects_root {
                discover_projects_for_lobe(root, &lobe.name)
                    .into_iter()
                    .map(|(name, path)| layout::SidebarProject {
                        components: parse_components_from_index(&path),
                        name,
                        path,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            layout::SidebarItem {
                lobe: lobe.name.clone(),
                path: lobe.path.clone(),
                is_active: lobe.name == active_name,
                projects,
            }
        }).collect()
    }

    fn active_tasks(&self) -> Vec<TaskEntry> {
        self.tasks_by_lobe
            .get(&self.active_lobe_name())
            .cloned()
            .unwrap_or_default()
    }

    fn active_reviews(&self) -> Vec<ReviewItem> {
        let lobe = self.active_lobe_name();
        self.review_items
            .iter()
            .filter(|item| item.lobe == lobe)
            .cloned()
            .collect()
    }

    fn active_logs(&self) -> Vec<String> {
        self.log_by_lobe
            .get(&self.active_lobe_name())
            .cloned()
            .unwrap_or_default()
    }

    fn selected_review(&self) -> Option<&ReviewItem> {
        let lobe = self.active_lobe_name();
        self.review_items
            .iter()
            .filter(|item| item.lobe == lobe)
            .nth(self.review_selected_idx)
    }

    fn selected_review_index(&self) -> Option<usize> {
        let lobe = self.active_lobe_name();
        self.review_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.lobe == lobe)
            .nth(self.review_selected_idx)
            .map(|(idx, _)| idx)
    }

    fn set_selected_review_status(&mut self, status: ReviewStatus) {
        if let Some(idx) = self.selected_review_index() {
            self.review_items[idx].status = status;
        }
    }

    fn set_review_status(&mut self, id: &str, status: ReviewStatus) {
        if let Some(item) = self.review_items.iter_mut().find(|item| item.id == id) {
            item.status = status;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.active_section {
            Section::Agents => {}
            Section::Review => {
                self.review_selected_idx =
                    move_idx(self.review_selected_idx, self.active_reviews().len(), delta);
            }
            Section::Log => {
                if delta < 0 {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                } else {
                    self.log_scroll = self.log_scroll.saturating_add(delta as u16);
                }
            }
        }
    }

    fn move_sidebar_selection(&mut self, delta: isize, sidebar: &[layout::SidebarItem]) {
        let len = Self::nav_names_from_sidebar(sidebar).len();
        self.sidebar_selected_idx = move_idx(self.sidebar_selected_idx, len, delta);
        if let Some(lobe_idx) = Self::lobe_idx_for_nav(sidebar, self.sidebar_selected_idx) {
            self.active_lobe_idx = lobe_idx;
        }
    }

    /// Flat navigable list: lobe → projects → components, across all lobes.
    fn nav_names_from_sidebar(sidebar: &[layout::SidebarItem]) -> Vec<String> {
        let mut names = Vec::new();
        for item in sidebar {
            names.push(item.lobe.clone());
            for project in &item.projects {
                names.push(project.name.clone());
                names.extend(project.components.iter().cloned());
            }
        }
        names
    }

    /// Returns the lobe index that owns the item at `nav_idx`.
    fn lobe_idx_for_nav(sidebar: &[layout::SidebarItem], nav_idx: usize) -> Option<usize> {
        let mut counter = 0usize;
        for (lobe_idx, item) in sidebar.iter().enumerate() {
            let lobe_size = 1 + item.projects.iter().map(|p| 1 + p.components.len()).sum::<usize>();
            if nav_idx < counter + lobe_size {
                return Some(lobe_idx);
            }
            counter += lobe_size;
        }
        None
    }

    /// Returns (display_name, ideas_path) for the item at `nav_idx`.
    /// Lobe → lobe path. Project/component → project path.
    fn selected_project_info(sidebar: &[layout::SidebarItem], nav_idx: usize) -> Option<(String, std::path::PathBuf)> {
        let mut counter = 0usize;
        for item in sidebar {
            if counter == nav_idx {
                return Some((item.lobe.clone(), item.path.clone()));
            }
            counter += 1;
            for project in &item.projects {
                if counter == nav_idx {
                    return Some((project.name.clone(), project.path.clone()));
                }
                counter += 1;
                for _ in &project.components {
                    if counter == nav_idx {
                        return Some((project.name.clone(), project.path.clone()));
                    }
                    counter += 1;
                }
            }
        }
        None
    }

    fn clamp_selection_with(&mut self, sidebar: &[layout::SidebarItem]) {
        let count = Self::nav_names_from_sidebar(sidebar).len();
        self.sidebar_selected_idx = clamp_idx(self.sidebar_selected_idx, count);
        self.review_selected_idx = clamp_idx(self.review_selected_idx, self.active_reviews().len());
    }

    fn submit_input_command(&mut self) {
        let text = self.input_text.trim().to_owned();
        if text.is_empty() {
            self.input_text.clear();
            return;
        }

        let lobe = self.active_lobe_name();
        let full_text = if let Some(ref ctx) = self.command_context {
            format!("[{ctx}] {text}")
        } else {
            text.clone()
        };

        if let Some(ref tx) = self.msg_tx {
            match tx.try_send(TuiMessage::Command {
                lobe: lobe.clone(),
                text: full_text.clone(),
            }) {
                Ok(()) => {
                    self.push_log(&lobe, format!("→ {full_text}"));
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    self.push_log(&lobe, "⚠ send queue full — daemon busy, try again".to_owned());
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    self.push_log(&lobe, "⚠ daemon disconnected".to_owned());
                }
            }
        } else {
            self.push_log(&lobe, "⚠ daemon offline — command not sent".to_owned());
        }
        self.input_text.clear();
        self.active_section = Section::Agents;
    }

    fn reset_cursors(&mut self) {
        self.review_selected_idx = 0;
        self.sidebar_selected_idx = 0;
        self.command_context = None;
        self.log_scroll = 0;
    }

    fn update_tasks(&mut self, lobe: &str, message: &str) {
        if let Some(filename) = message.strip_prefix("Plan started: ") {
            self.add_task(lobe, TaskKind::Plan, filename);
        } else if let Some(filename) = message.strip_prefix("Inquiry started: ") {
            self.add_task(lobe, TaskKind::Inquiry, filename);
        } else if let Some(text) = message.strip_prefix("Command received: ") {
            self.add_task(lobe, TaskKind::Command, text);
        } else if let Some(filename) = message.strip_prefix("Plan completed: ") {
            self.remove_task(lobe, TaskKind::Plan, filename);
        } else if let Some(filename) = message.strip_prefix("Plan failed: ") {
            self.remove_task(lobe, TaskKind::Plan, filename);
        } else if let Some(filename) = message.strip_prefix("Plan needs permission: ") {
            self.remove_task(lobe, TaskKind::Plan, filename);
        } else if let Some(filename) = message.strip_prefix("Inquiry completed: ") {
            self.remove_task(lobe, TaskKind::Inquiry, filename);
        } else if let Some(filename) = message.strip_prefix("Inquiry failed: ") {
            self.remove_task(lobe, TaskKind::Inquiry, filename);
        } else if message == "Command completed" || message.starts_with("Command failed") {
            self.clear_tasks_of_kind(lobe, TaskKind::Command);
        }
    }

    fn add_task(&mut self, lobe: &str, kind: TaskKind, filename: &str) {
        let tasks = self.tasks_by_lobe.entry(lobe.to_owned()).or_default();
        if !tasks
            .iter()
            .any(|task| task.kind == kind && task.filename == filename)
        {
            tasks.push(TaskEntry {
                kind,
                filename: filename.to_owned(),
            });
        }
    }

    fn remove_task(&mut self, lobe: &str, kind: TaskKind, filename: &str) {
        if let Some(tasks) = self.tasks_by_lobe.get_mut(lobe) {
            tasks.retain(|task| !(task.kind == kind && task.filename == filename));
        }
    }

    fn clear_tasks_of_kind(&mut self, lobe: &str, kind: TaskKind) {
        if let Some(tasks) = self.tasks_by_lobe.get_mut(lobe) {
            tasks.retain(|task| task.kind != kind);
        }
    }

    fn push_log(&mut self, lobe: &str, line: String) {
        let log = self.log_by_lobe.entry(lobe.to_owned()).or_default();
        log.push(line);
        let overflow = log.len().saturating_sub(500);
        if overflow > 0 {
            log.drain(..overflow);
        }
    }

}


fn create_idea_file(vault_path: &Path, title: &str) -> anyhow::Result<std::path::PathBuf> {
    let date_out = std::process::Command::new("date").arg("+%Y-%m-%d").output()?;
    let date = String::from_utf8_lossy(&date_out.stdout).trim().to_owned();

    let slug = {
        let mut s = String::new();
        let mut last_dash = true;
        for c in title.chars() {
            if c.is_alphanumeric() {
                s.push(c.to_ascii_lowercase());
                last_dash = false;
            } else if !last_dash {
                s.push('-');
                last_dash = true;
            }
        }
        let trimmed = s.trim_end_matches('-').to_owned();
        if trimmed.is_empty() { "idea".to_owned() } else { trimmed }
    };

    let ideas_dir = vault_path.join("ideas");
    std::fs::create_dir_all(&ideas_dir)?;

    let filename = format!("{date}-{slug}.md");
    let file_path = ideas_dir.join(&filename);
    std::fs::write(&file_path, format!("# {title}\n\ndate: {date}\n\n---\n\n"))?;
    Ok(file_path)
}

fn next_section(section: Section) -> Section {
    match section {
        Section::Agents => Section::Review,
        Section::Review => Section::Log,
        Section::Log => Section::Agents,
    }
}

fn previous_section(section: Section) -> Section {
    match section {
        Section::Agents => Section::Log,
        Section::Review => Section::Agents,
        Section::Log => Section::Review,
    }
}

fn move_idx(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        (current + delta as usize).min(len - 1)
    }
}

fn clamp_idx(current: usize, len: usize) -> usize {
    if len == 0 { 0 } else { current.min(len - 1) }
}

fn split_lobe_prefix(line: &str) -> (String, String) {
    if let Some(rest) = line.strip_prefix('[') {
        if let Some((lobe, message)) = rest.split_once("] ") {
            return (lobe.to_owned(), message.to_owned());
        }
    }
    ("_global".to_owned(), line.to_owned())
}

/// Returns (name, path) pairs for project subdirs under root.
/// If a subdir has a `.lobe` marker, it must match lobe_name. If absent, the subdir is included.
fn discover_projects_for_lobe(root: &Path, lobe_name: &str) -> Vec<(String, std::path::PathBuf)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut projects: Vec<(String, std::path::PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let ft = e.file_type().ok()?;
            if !ft.is_dir() { return None; }
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { return None; }
            let lobe_file = path.join(".lobe");
            if lobe_file.exists() {
                let content = std::fs::read_to_string(&lobe_file).unwrap_or_default();
                if content.trim() != lobe_name { return None; }
            }
            Some((name, path))
        })
        .collect();
    projects.sort_by(|a, b| a.0.cmp(&b.0));
    projects
}

/// Parses the `## Componentes` or `## Components` table in `<path>/index.md`.
fn parse_components_from_index(path: &Path) -> Vec<String> {
    let content = std::fs::read_to_string(path.join("index.md")).unwrap_or_default();
    let mut in_section = false;
    let mut header_seen = false;
    let mut components = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("## Componentes") || t.starts_with("## Components") {
            in_section = true;
            header_seen = false;
            continue;
        }
        if in_section {
            if t.starts_with('|') {
                if !header_seen { header_seen = true; continue; } // table header
                if t.contains("---") { continue; }                // separator row
                if let Some(name) = t.split('|').nth(1) {
                    let name = name.trim();
                    if !name.is_empty() {
                        components.push(name.to_owned());
                    }
                }
            } else if header_seen && !t.is_empty() {
                break; // end of table
            }
        }
    }
    components
}

