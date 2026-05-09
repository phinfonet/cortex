mod event;
mod socket;
mod watcher;

pub use event::AppEvent;
pub use socket::SocketReceiver;
pub use watcher::FileWatcher;

use std::path::PathBuf;

use crate::config::Lobe;
use tokio::sync::mpsc;

pub struct Monitor {
    watcher: FileWatcher,
    socket: SocketReceiver,
}

impl Monitor {
    pub fn new(lobes: Vec<Lobe>, socket_path: PathBuf) -> Self {
        Self {
            watcher: FileWatcher::new(lobes),
            socket: SocketReceiver::new(socket_path),
        }
    }

    pub async fn run(self) -> anyhow::Result<mpsc::Receiver<AppEvent>> {
        let (tx, rx) = mpsc::channel(256);

        tokio::spawn(self.watcher.run(tx.clone()));
        tokio::spawn(self.socket.run(tx));

        Ok(rx)
    }
}
