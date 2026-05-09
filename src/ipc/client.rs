use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use super::protocol::{DaemonMessage, TuiMessage};

pub struct UiClient {
    socket_path: PathBuf,
}

impl UiClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub async fn connect(
        self,
    ) -> anyhow::Result<(mpsc::Receiver<DaemonMessage>, mpsc::Sender<TuiMessage>)> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (read_half, write_half) = stream.into_split();

        let (daemon_msg_tx, daemon_msg_rx) = mpsc::channel::<DaemonMessage>(256);
        let (tui_msg_tx, mut tui_msg_rx) = mpsc::channel::<TuiMessage>(64);

        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let Ok(msg) = serde_json::from_str::<DaemonMessage>(line.trim()) else {
                            continue;
                        };
                        if daemon_msg_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut write_half = write_half;
            while let Some(msg) = tui_msg_rx.recv().await {
                let Ok(mut line) = serde_json::to_string(&msg) else {
                    continue;
                };
                line.push('\n');
                if write_half.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
        });

        Ok((daemon_msg_rx, tui_msg_tx))
    }
}
