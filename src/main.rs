mod app;
mod backends;
mod config;
mod ipc;
mod mcp;
mod monitor;
mod obsidian;
mod router;
mod suppliers;
mod tui;

use clap::{Parser, Subcommand};
use tokio::sync::mpsc;

use ipc::{AppEventDto, DaemonMessage, TuiMessage, UiClient, UiServer};
use tui::{ReviewItem, ReviewKind, ReviewStatus};

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
    let (ui_event_tx, mut ui_event_rx) = mpsc::channel::<monitor::AppEvent>(64);

    let ui_server =
        UiServer::new(config.monitor.ui_socket_path.clone()).with_app_event_sender(ui_event_tx);
    let broadcast_tx = ui_server.sender();
    let router_broadcast_tx = broadcast_tx.clone();
    let _approval_handle = ui_server.approval_handle();

    let ui_server_task = tokio::spawn(async move {
        let _ = ui_server.run().await;
    });

    let monitor = monitor::Monitor::new(config.lobes.clone(), config.monitor.socket_path.clone());
    let mut event_rx = monitor.run().await?;

    let router = router::Router::new(config.lobes.clone(), router_tx.clone())
        .with_global_agents_dir(config.agents_dir.clone());

    tokio::spawn(async move {
        loop {
            let Some(event) = (tokio::select! {
                Some(event) = event_rx.recv() => Some(event),
                Some(event) = ui_event_rx.recv() => Some(event),
                else => None,
            }) else {
                break;
            };

            let dto: Option<AppEventDto> = event.clone().into();
            if let Some(data) = dto {
                let _ = broadcast_tx.send(DaemonMessage::Event { data });
            }
            let _ = router.handle(event).await;
        }
    });

    while let Some(event) = router_rx.recv().await {
        if let monitor::AppEvent::CommandCompleted { lobe, output } = &event {
            let _ = router_broadcast_tx.send(DaemonMessage::CommandOutput {
                lobe: lobe.clone(),
                text: output.clone(),
            });
        }
        let dto: Option<AppEventDto> = event.into();
        if let Some(data) = dto {
            let _ = router_broadcast_tx.send(DaemonMessage::Event { data });
        }
    }

    ui_server_task.abort();

    Ok(())
}

async fn run_tui() -> anyhow::Result<()> {
    let config = config::load()?;

    let (approval_tx, approval_rx) = mpsc::channel(32);

    let (events_rx, review_rx, command_output_rx, tui_msg_tx_opt) =
        match UiClient::new(config.monitor.ui_socket_path.clone())
            .connect()
            .await
        {
            Ok((daemon_msg_rx, tui_msg_tx)) => {
                let (events_line_tx, events_line_rx) = mpsc::channel::<String>(256);
                let (review_tx, review_rx) = mpsc::channel::<ReviewItem>(256);
                let (command_output_tx, command_output_rx) = mpsc::channel::<(String, String)>(64);

                tokio::spawn(bridge_daemon_messages(
                    daemon_msg_rx,
                    events_line_tx,
                    review_tx,
                    command_output_tx,
                    approval_tx,
                    tui_msg_tx.clone(),
                ));

                (
                    Some(events_line_rx),
                    Some(review_rx),
                    Some(command_output_rx),
                    Some(tui_msg_tx),
                )
            }
            Err(_) => {
                drop(approval_tx);
                (None, None, None, None)
            }
        };

    tui::Tui::new(
        approval_rx,
        config.lobes,
        events_rx,
        review_rx,
        command_output_rx,
        tui_msg_tx_opt,
    )
    .run()
    .await
}

async fn bridge_daemon_messages(
    mut daemon_msg_rx: mpsc::Receiver<DaemonMessage>,
    events_line_tx: mpsc::Sender<String>,
    review_tx: mpsc::Sender<ReviewItem>,
    command_output_tx: mpsc::Sender<(String, String)>,
    approval_tx: mpsc::Sender<tui::ApprovalRequest>,
    tui_msg_tx: mpsc::Sender<TuiMessage>,
) {
    while let Some(msg) = daemon_msg_rx.recv().await {
        match msg {
            DaemonMessage::Event { data } => {
                if let Some(review_item) = review_item_from_event(&data) {
                    let _ = review_tx.send(review_item).await;
                }
                if let Some(request) = approval_request_from_event(&data) {
                    let _ = approval_tx.send(request).await;
                }
                let line = format_event_line(&data);
                let _ = events_line_tx.send(line).await;
            }
            DaemonMessage::CommandOutput { lobe, text } => {
                let _ = command_output_tx.send((lobe, text)).await;
            }
            DaemonMessage::ApprovalRequest {
                id,
                title,
                body,
                approval_kind,
            } => {
                let kind = match approval_kind {
                    ipc::ApprovalKindDto::CodeReview => tui::ApprovalKind::CodeReview,
                    ipc::ApprovalKindDto::Permission => tui::ApprovalKind::Permission,
                };
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                let request = tui::ApprovalRequest {
                    kind,
                    title,
                    body,
                    tx: response_tx,
                };
                let tui_msg_tx = tui_msg_tx.clone();
                let approval_id = id.clone();
                tokio::spawn(async move {
                    if let Ok(response) = response_rx.await {
                        let accepted = matches!(response, tui::ApprovalResponse::Accepted);
                        let _ = tui_msg_tx
                            .send(TuiMessage::ApprovalResponse {
                                id: approval_id,
                                accepted,
                            })
                            .await;
                    }
                });
                let _ = approval_tx.send(request).await;
            }
        }
    }
}

fn review_item_from_event(data: &AppEventDto) -> Option<ReviewItem> {
    match data {
        AppEventDto::PlanCompleted {
            lobe,
            filename,
            summary,
            diff,
        } => Some(ReviewItem {
            id: format!("plan:{lobe}:{filename}"),
            lobe: lobe.clone(),
            filename: filename.clone(),
            kind: ReviewKind::Plan,
            summary: summary.clone(),
            diff: diff.clone(),
            status: ReviewStatus::Pending,
        }),
        AppEventDto::InquiryCompleted {
            lobe,
            id,
            output_path,
        } => Some(ReviewItem {
            id: format!("inquiry:{lobe}:{id}"),
            lobe: lobe.clone(),
            filename: output_path.clone(),
            kind: ReviewKind::Inquiry,
            summary: format!("Inquiry {id} completed: {output_path}"),
            diff: None,
            status: ReviewStatus::Pending,
        }),
        _ => None,
    }
}

fn approval_request_from_event(data: &AppEventDto) -> Option<tui::ApprovalRequest> {
    match data {
        AppEventDto::PlanNeedsPermission { filename, .. } => {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            Some(tui::ApprovalRequest {
                kind: tui::ApprovalKind::Permission,
                title: "Needs permission".to_owned(),
                body: filename.clone(),
                tx,
            })
        }
        _ => None,
    }
}

fn format_event_line(data: &AppEventDto) -> String {
    match data {
        AppEventDto::FileChanged { lobe, path } => format!("[{lobe}] File changed: {path}"),
        AppEventDto::InquiryDetected { lobe, id, title } => {
            format!("[{lobe}] Inquiry detected: {id} {title}")
        }
        AppEventDto::InquiryStarted { lobe, id } => format!("[{lobe}] Inquiry started: {id}"),
        AppEventDto::InquiryCompleted {
            lobe,
            id,
            output_path: _,
        } => format!("[{lobe}] Inquiry completed: {id}"),
        AppEventDto::InquiryFailed {
            lobe,
            id,
            reason: _,
        } => {
            format!("[{lobe}] Inquiry failed: {id}")
        }
        AppEventDto::PlanDetected { lobe, filename } => {
            format!("[{lobe}] Plan detected: {filename}")
        }
        AppEventDto::PlanStarted { lobe, filename } => {
            format!("[{lobe}] Plan started: {filename}")
        }
        AppEventDto::PlanCompleted { lobe, filename, .. } => {
            format!("[{lobe}] Plan completed: {filename}")
        }
        AppEventDto::PlanFailed {
            lobe,
            filename,
            reason: _,
        } => format!("[{lobe}] Plan failed: {filename}"),
        AppEventDto::PlanNeedsPermission { lobe, filename } => {
            format!("[{lobe}] Plan needs permission: {filename}")
        }
        AppEventDto::CommandReceived { lobe, text } => {
            format!("[{lobe}] Command received: {text}")
        }
        AppEventDto::CommandCompleted { lobe, output: _ } => {
            format!("[{lobe}] Command completed")
        }
        AppEventDto::AgentStarted { description } => {
            format!("[_global] Agent started: {description}")
        }
        AppEventDto::AgentCompleted { description } => {
            format!("[_global] Agent completed: {description}")
        }
        AppEventDto::SessionEnded => "[_global] Session ended".to_owned(),
    }
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
