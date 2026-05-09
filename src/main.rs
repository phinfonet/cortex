mod app;
mod config;
mod tui;
mod router;
mod backends;
mod obsidian;
mod monitor;
mod mcp;
mod suppliers;

use clap::{Parser, Subcommand};
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "cortex")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Daemon,
    Tui,
    Event { payload: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Daemon => run_daemon().await,
        Command::Tui => run_tui().await,
        Command::Event { payload } => run_event(payload).await,
    }
}

async fn run_daemon() -> anyhow::Result<()> {
    let config = config::load()?;

    let (router_tx, mut router_rx) = mpsc::channel::<monitor::AppEvent>(256);

    let monitor = monitor::Monitor::new(config.lobes.clone(), config.monitor.socket_path.clone());
    let mut event_rx = monitor.run().await?;

    let router = router::Router::new(config.lobes.clone(), router_tx.clone());

    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let _ = router.handle(event).await;
        }
    });

    while let Some(_event) = router_rx.recv().await {}

    Ok(())
}

async fn run_tui() -> anyhow::Result<()> {
    let config = config::load()?;
    let (tx, rx) = mpsc::channel(32);
    drop(tx);
    tui::Tui::new(rx, config.lobes).run().await
}

async fn run_event(payload: String) -> anyhow::Result<()> {
    let config = config::load()?;
    let socket_path = &config.monitor.socket_path;

    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).await?;
    stream.write_all(payload.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    Ok(())
}
