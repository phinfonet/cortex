pub mod layout;
pub mod widgets;

use std::collections::HashMap;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::{mpsc, oneshot};

use crate::config::Lobe;

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
    Dashboard,
    PendingApproval(ApprovalRequest),
}

pub struct Tui {
    requests: mpsc::Receiver<ApprovalRequest>,
    lobes: Vec<Lobe>,
    active_lobe_idx: usize,
    events_by_lobe: HashMap<String, Vec<String>>,
    queue_len: usize,
}

impl Tui {
    pub fn new(requests: mpsc::Receiver<ApprovalRequest>, lobes: Vec<Lobe>) -> Self {
        let events_by_lobe = lobes
            .iter()
            .map(|l| (l.name.clone(), Vec::new()))
            .collect();

        Self {
            requests,
            lobes,
            active_lobe_idx: 0,
            events_by_lobe,
            queue_len: 0,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let mut screen = Screen::Dashboard;

        loop {
            let lobe_names: Vec<String> = self.lobes.iter().map(|l| l.name.clone()).collect();

            let active_events = if let Some(lobe) = self.lobes.get(self.active_lobe_idx) {
                self.events_by_lobe
                    .get(&lobe.name)
                    .cloned()
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            terminal.draw(|frame| match &screen {
                Screen::Dashboard => {
                    layout::dashboard(
                        frame,
                        &lobe_names,
                        self.active_lobe_idx,
                        &active_events,
                        self.queue_len,
                    );
                }
                Screen::PendingApproval(request) => {
                    layout::approval(frame, request);
                }
            })?;

            if crossterm::event::poll(Duration::from_millis(16))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match &mut screen {
                        Screen::Dashboard => {
                            match key.code {
                                KeyCode::Char('q') => return Ok(()),
                                KeyCode::Tab | KeyCode::Right => {
                                    if !self.lobes.is_empty() {
                                        self.active_lobe_idx =
                                            (self.active_lobe_idx + 1) % self.lobes.len();
                                    }
                                }
                                KeyCode::BackTab | KeyCode::Left => {
                                    if !self.lobes.is_empty() {
                                        self.active_lobe_idx = self
                                            .active_lobe_idx
                                            .checked_sub(1)
                                            .unwrap_or(self.lobes.len() - 1);
                                    }
                                }
                                KeyCode::Char('n') => {
                                    let (tx, _rx) = oneshot::channel();
                                    let request = ApprovalRequest {
                                        kind: ApprovalKind::CodeReview,
                                        title: "New inquiry".to_owned(),
                                        body: "Enter inquiry text".to_owned(),
                                        tx,
                                    };
                                    self.queue_len = self.queue_len.saturating_add(1);
                                    screen = Screen::PendingApproval(request);
                                }
                                _ => {}
                            }
                        }
                        Screen::PendingApproval(_) => {
                            let response = match key.code {
                                KeyCode::Char('a') => Some(ApprovalResponse::Accepted),
                                KeyCode::Char('r') => Some(ApprovalResponse::Rejected),
                                KeyCode::Char('q') => Some(ApprovalResponse::Rejected),
                                _ => None,
                            };
                            if let Some(response) = response {
                                if let Screen::PendingApproval(request) =
                                    std::mem::replace(&mut screen, Screen::Dashboard)
                                {
                                    let _ = request.tx.send(response);
                                }
                                if self.queue_len > 0 {
                                    self.queue_len -= 1;
                                }
                            }
                        }
                    }
                }
            }

            match screen {
                Screen::Dashboard => {
                    if let Ok(request) = self.requests.try_recv() {
                        self.queue_len = self.queue_len.saturating_add(1);
                        screen = Screen::PendingApproval(request);
                    } else {
                        screen = Screen::Dashboard;
                    }
                }
                Screen::PendingApproval(_) => {}
            }
        }
    }
}
