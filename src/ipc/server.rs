use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::monitor::AppEvent;

use super::protocol::{ApprovalKindDto, DaemonMessage, TuiMessage};

pub struct UiServer {
    socket_path: PathBuf,
    event_tx: broadcast::Sender<DaemonMessage>,
    app_event_tx: Option<mpsc::Sender<AppEvent>>,
    pending_approvals: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
}

impl UiServer {
    pub fn new(socket_path: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            socket_path,
            event_tx,
            app_event_tx: None,
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_app_event_sender(mut self, tx: mpsc::Sender<AppEvent>) -> Self {
        self.app_event_tx = Some(tx);
        self
    }

    pub fn sender(&self) -> broadcast::Sender<DaemonMessage> {
        self.event_tx.clone()
    }

    pub fn approval_handle(&self) -> ApprovalHandle {
        ApprovalHandle {
            pending: Arc::clone(&self.pending_approvals),
            event_tx: self.event_tx.clone(),
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .with_context(|| format!("remove stale socket {}", self.socket_path.display()))?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("bind {}", self.socket_path.display()))?;

        loop {
            let (stream, _) = listener.accept().await?;
            let event_rx = self.event_tx.subscribe();
            let pending = Arc::clone(&self.pending_approvals);
            let app_event_tx = self.app_event_tx.clone();

            tokio::spawn(async move {
                let (read_half, write_half) = stream.into_split();

                let pending_write = Arc::clone(&pending);
                let write_task = tokio::spawn(handle_write(write_half, event_rx));

                let read_task = tokio::spawn(handle_read(read_half, pending_write, app_event_tx));

                let _ = tokio::join!(write_task, read_task);
            });
        }
    }
}

async fn handle_write(
    mut write_half: tokio::net::unix::OwnedWriteHalf,
    mut event_rx: broadcast::Receiver<DaemonMessage>,
) {
    loop {
        match event_rx.recv().await {
            Ok(msg) => {
                let Ok(mut line) = serde_json::to_string(&msg) else {
                    continue;
                };
                line.push('\n');
                if write_half.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn handle_read(
    read_half: tokio::net::unix::OwnedReadHalf,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
    app_event_tx: Option<mpsc::Sender<AppEvent>>,
) {
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let Ok(msg) = serde_json::from_str::<TuiMessage>(line.trim()) else {
                    continue;
                };
                match msg {
                    TuiMessage::ApprovalResponse { id, accepted } => {
                        let mut map = pending.lock().await;
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(accepted);
                        }
                    }
                    TuiMessage::Command { lobe, text } => {
                        if let Some(tx) = &app_event_tx {
                            let _ = tx.send(AppEvent::CommandReceived { lobe, text }).await;
                        }
                    }
                }
            }
        }
    }
}

pub struct ApprovalHandle {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
    event_tx: broadcast::Sender<DaemonMessage>,
}

impl ApprovalHandle {
    pub async fn request(
        &self,
        title: String,
        body: String,
        kind: ApprovalKindDto,
    ) -> anyhow::Result<bool> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(id.clone(), tx);
        }

        let msg = DaemonMessage::ApprovalRequest {
            id,
            title,
            body,
            approval_kind: kind,
        };

        let _ = self.event_tx.send(msg);

        let accepted = rx.await.context("approval oneshot dropped")?;
        Ok(accepted)
    }

    pub fn sender_for_mpsc(&self) -> mpsc::Sender<DaemonMessage> {
        let (tx, mut rx) = mpsc::channel::<DaemonMessage>(16);
        let broadcast_tx = self.event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let _ = broadcast_tx.send(msg);
            }
        });
        tx
    }
}
