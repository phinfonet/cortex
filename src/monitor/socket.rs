use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use super::event::AppEvent;

pub struct SocketReceiver {
    socket_path: PathBuf,
}

impl SocketReceiver {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub async fn run(self, tx: mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        loop {
            let (stream, _) = listener.accept().await?;
            let tx = tx.clone();

            tokio::spawn(async move {
                let reader = BufReader::new(stream);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    let event = parse_hook_payload(&line);
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
            });
        }
    }
}

fn parse_hook_payload(line: &str) -> AppEvent {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return AppEvent::HookRaw {
            payload: serde_json::Value::String(line.to_owned()),
        };
    };

    match value.get("type").and_then(|v| v.as_str()) {
        Some("agent_started") => {
            let description = value
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            AppEvent::AgentStarted { description }
        }
        Some("agent_completed") => {
            let description = value
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            AppEvent::AgentCompleted { description }
        }
        Some("session_ended") => AppEvent::SessionEnded,
        _ => AppEvent::HookRaw { payload: value },
    }
}
