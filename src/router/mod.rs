pub mod classifier;

use tokio::sync::mpsc;
use tokio::process::Command;

use crate::config::Lobe;
use crate::monitor::AppEvent;
use crate::suppliers::gemini::GeminiSupplier;
use crate::suppliers::Supplier;

pub struct Router {
    lobes: Vec<Lobe>,
    tx: mpsc::Sender<AppEvent>,
}

impl Router {
    pub fn new(lobes: Vec<Lobe>, tx: mpsc::Sender<AppEvent>) -> Self {
        Self { lobes, tx }
    }

    pub async fn handle(&self, event: AppEvent) -> anyhow::Result<()> {
        match event {
            AppEvent::InquiryDetected { lobe, inquiry } => {
                let lobe_path = self
                    .lobes
                    .iter()
                    .find(|l| l.name == lobe)
                    .map(|l| l.path.clone());

                let _ = self
                    .tx
                    .send(AppEvent::InquiryStarted {
                        lobe: lobe.clone(),
                        id: inquiry.id.clone(),
                    })
                    .await;

                if let Some(ref path) = lobe_path {
                    let rel_path = path.join("inquiries").join(format!("{}.md", inquiry.id));
                    let _ = set_inquiry_status(&rel_path.to_string_lossy(), "in_progress").await;
                }

                let prompt = format!("{}\n\n{}", inquiry.title, inquiry.body);
                let supplier = GeminiSupplier::new();

                match supplier.run(&prompt).await {
                    Ok(doc_content) => {
                        if let Some(ref path) = lobe_path {
                            let output_name = &inquiry.output;
                            let output_full = path.join("docs").join(format!("{}.md", output_name));

                            let _ = Command::new("obsidian")
                                .args([
                                    "create",
                                    &format!("name={}", output_name),
                                    &format!("content={}", doc_content),
                                    "silent",
                                ])
                                .output()
                                .await;

                            if let Some(ref path) = lobe_path {
                                let rel_path = path
                                    .join("inquiries")
                                    .join(format!("{}.md", inquiry.id));
                                let _ =
                                    set_inquiry_status(&rel_path.to_string_lossy(), "done").await;
                            }

                            let _ = self
                                .tx
                                .send(AppEvent::InquiryCompleted {
                                    lobe: lobe.clone(),
                                    id: inquiry.id.clone(),
                                    output_path: output_full.to_string_lossy().into_owned(),
                                })
                                .await;
                        }
                    }
                    Err(err) => {
                        if let Some(ref path) = lobe_path {
                            let rel_path = path
                                .join("inquiries")
                                .join(format!("{}.md", inquiry.id));
                            let _ =
                                set_inquiry_status(&rel_path.to_string_lossy(), "pending").await;
                        }

                        let _ = self
                            .tx
                            .send(AppEvent::InquiryFailed {
                                lobe: lobe.clone(),
                                id: inquiry.id.clone(),
                                reason: err.to_string(),
                            })
                            .await;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

async fn set_inquiry_status(path: &str, status: &str) -> anyhow::Result<()> {
    let output = Command::new("obsidian")
        .args([
            "property:set",
            "name=status",
            &format!("value={}", status),
            &format!("path={}", path),
            "silent",
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "obsidian property:set failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}
