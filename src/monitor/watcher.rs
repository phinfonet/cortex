use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::config::Lobe;

use super::event::{AppEvent, InquiryKind, InquiryMeta, PlanMeta};

pub struct FileWatcher {
    pub lobes: Vec<Lobe>,
}

impl FileWatcher {
    pub fn new(lobes: Vec<Lobe>) -> Self {
        Self { lobes }
    }

    pub async fn run(self, tx: mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let mut watcher = RecommendedWatcher::new(notify_tx, notify::Config::default())?;

        for lobe in &self.lobes {
            if lobe.path.exists() {
                watcher.watch(&lobe.path, RecursiveMode::Recursive)?;
            }
        }

        let lobes = self.lobes;
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            for result in notify_rx {
                match result {
                    Ok(event) => {
                        let is_relevant =
                            matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_));

                        if !is_relevant {
                            continue;
                        }

                        for path in event.paths {
                            let lobe_name = lobes
                                .iter()
                                .find(|l| path.starts_with(&l.path))
                                .map(|l| l.name.clone())
                                .unwrap_or_default();
                            let tx = tx.clone();
                            let path_clone = path.clone();
                            let handle = handle.clone();
                            handle.spawn(async move {
                                let app_event = build_event(lobe_name, &path_clone).await;
                                let _ = tx.send(app_event).await;
                            });
                        }
                    }
                    Err(err) => {
                        eprintln!("file watcher error: {err}");
                    }
                }
            }
        });

        Ok(())
    }

    fn lobe_for_path(&self, path: &Path) -> String {
        self.lobes
            .iter()
            .find(|l| path.starts_with(&l.path))
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }
}

async fn build_event(lobe: String, path: &Path) -> AppEvent {
    if is_inquiry_file(path) {
        if let Some(meta) = parse_inquiry(path).await {
            return AppEvent::InquiryDetected {
                lobe,
                inquiry: meta,
            };
        }
    }
    if is_plan_file(path) {
        // Only trigger if status is ready/absent — skip if already in_progress, done, etc.
        let status = read_plan_status(path).await;
        let should_trigger = matches!(status.as_deref(), None | Some("") | Some("ready"));
        if should_trigger {
            return AppEvent::PlanDetected {
                lobe: lobe.clone(),
                plan: PlanMeta {
                    filename: path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path: path.to_string_lossy().to_string(),
                    project: lobe,
                },
            };
        }
    }
    AppEvent::FileChanged {
        lobe,
        path: path.to_string_lossy().into_owned(),
    }
}

/// Reads the `status:` field from a plan file's frontmatter. Returns None if absent or unreadable.
async fn read_plan_status(path: &Path) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let content = content.trim_start();
    let after_open = content.strip_prefix("---")?;
    let close_pos = after_open.find("\n---")?;
    let frontmatter = &after_open[..close_pos];
    frontmatter.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix("status:")?.trim();
        let value = value.trim_matches(['"', '\'']);
        Some(value.to_string())
    })
}

fn is_inquiry_file(path: &Path) -> bool {
    let in_inquiries = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n == "inquiries")
        .unwrap_or(false);

    let is_md = path.extension().map(|e| e == "md").unwrap_or(false);

    in_inquiries && is_md
}

fn is_plan_file(path: &Path) -> bool {
    let in_plans = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n == "plans")
        .unwrap_or(false);

    let is_md = path.extension().map(|e| e == "md").unwrap_or(false);

    in_plans && is_md
}

#[derive(Debug, Deserialize)]
struct InquiryFrontmatter {
    pub id: Option<String>,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub output: Option<String>,
}

async fn parse_inquiry(path: &Path) -> Option<InquiryMeta> {
    let content = tokio::fs::read_to_string(path).await.ok()?;

    let (frontmatter_raw, body) = split_frontmatter(&content)?;

    let fm: InquiryFrontmatter = serde_yaml::from_str(frontmatter_raw).ok()?;

    if fm.status.as_deref() != Some("pending") {
        return None;
    }

    let kind = match fm.kind.as_deref()? {
        "research" => InquiryKind::Research,
        "decision" => InquiryKind::Decision,
        "analysis" => InquiryKind::Analysis,
        _ => return None,
    };

    Some(InquiryMeta {
        id: fm.id?,
        title: fm.title?,
        kind,
        output: fm.output.unwrap_or_default(),
        body: body.trim().to_owned(),
    })
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let after_open = &content[3..];
    let close_pos = after_open.find("\n---")?;
    let frontmatter = &after_open[..close_pos];
    let body_start = close_pos + 4;
    let body = if body_start < after_open.len() {
        &after_open[body_start..]
    } else {
        ""
    };
    Some((frontmatter, body))
}
