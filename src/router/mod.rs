pub mod classifier;

use std::path::PathBuf;

use tokio::process::Command;
use tokio::sync::mpsc;

use crate::config::Lobe;
use crate::monitor::AppEvent;
use crate::suppliers::Supplier;
use crate::suppliers::codex::CodexSupplier;
use crate::suppliers::gemini::GeminiSupplier;

pub struct Router {
    lobes: Vec<Lobe>,
    tx: mpsc::Sender<AppEvent>,
    global_agents_dir: Option<PathBuf>,
}

impl Router {
    pub fn new(lobes: Vec<Lobe>, tx: mpsc::Sender<AppEvent>) -> Self {
        Self {
            lobes,
            tx,
            global_agents_dir: None,
        }
    }

    pub fn with_global_agents_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.global_agents_dir = dir;
        self
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
                                let rel_path =
                                    path.join("inquiries").join(format!("{}.md", inquiry.id));
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
                            let rel_path =
                                path.join("inquiries").join(format!("{}.md", inquiry.id));
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
            AppEvent::PlanDetected { lobe, plan } => {
                let supplier = read_plan_supplier(&plan.path).await;
                let agent = read_plan_agent(&plan.path).await;

                if supplier.as_deref() == Some("codex") {
                    let _ = self
                        .tx
                        .send(AppEvent::PlanStarted {
                            lobe: lobe.clone(),
                            filename: plan.filename.clone(),
                        })
                        .await;

                    let lobe_agents_dir = self
                        .lobes
                        .iter()
                        .find(|l| l.name == lobe)
                        .map(|l| l.agents_dir());

                    let result = async {
                        let plan_content = tokio::fs::read_to_string(&plan.path).await?;
                        let prompt = if let Some(ref agent_name) = agent {
                            let agent_content = resolve_agent(
                                agent_name,
                                lobe_agents_dir.as_ref(),
                                self.global_agents_dir.as_ref(),
                            )
                            .await;
                            match agent_content {
                                Some(content) => format!("{}\n\n---\n\n{}", content, plan_content),
                                None => plan_content,
                            }
                        } else {
                            plan_content
                        };
                        CodexSupplier.run(&prompt).await
                    }
                    .await;

                    match result {
                        Ok(output) => {
                            let summary = output.chars().take(500).collect();
                            let _ = self
                                .tx
                                .send(AppEvent::PlanCompleted {
                                    lobe,
                                    filename: plan.filename,
                                    summary,
                                    diff: None,
                                })
                                .await;
                        }
                        Err(err) => {
                            let _ = self
                                .tx
                                .send(AppEvent::PlanFailed {
                                    lobe,
                                    filename: plan.filename,
                                    reason: err.to_string(),
                                })
                                .await;
                        }
                    }

                    return Ok(());
                }

                // Resolve the actual code directory: projects_root > lobe.path > ~/Stuff
                let lobe_config = self.lobes.iter().find(|l| l.name == lobe);
                let code_dir: PathBuf = lobe_config
                    .and_then(|l| l.projects_root.as_ref())
                    .cloned()
                    .or_else(|| lobe_config.map(|l| l.path.clone()))
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Stuff")
                    });

                let _ = self
                    .tx
                    .send(AppEvent::PlanStarted {
                        lobe: lobe.clone(),
                        filename: plan.filename.clone(),
                    })
                    .await;

                let agent_mention = agent
                    .as_deref()
                    .map(|name| format!("@{}", name))
                    .unwrap_or_else(|| "@synapse".to_string());
                let prompt = format!(
                    "{} New plan at {} for project '{}'.\
    Read the plan and follow this review workflow exactly:\n\
    1. Read the plan frontmatter — use the 'branch' field as the target branch. \
    If absent, derive it as 'cortex/{}'.\n\
    2. In the project's git repo (under {}/ — pick the right subdirectory), \
    checkout that branch (create from current HEAD if it doesn't exist).\n\
    3. Execute the plan — write files, never commit.\n\
    4. Run 'git diff HEAD' to get a summary of all changes.\n\
    5. If the plan has 'review: opus', or if any changed file is a migration, \
    auth module, or security-sensitive — spawn an Opus subagent to validate the diff.\n\
    6. Send a push notification: branch name + count of files changed + one-line summary.",
                    agent_mention,
                    plan.path,
                    plan.project,
                    plan.filename.trim_end_matches(".md"),
                    code_dir.display(),
                );

                let code_dir_str = code_dir.to_string_lossy().to_string();
                let output = Command::new("claude")
                    .args([
                        "-p",
                        &prompt,
                        "--allowedTools",
                        "Bash,Read,Edit,Write,Agent,WebSearch,WebFetch,PushNotification,mcp__obsidian__*,mcp__gemini__*",
                        "--add-dir",
                        &code_dir_str,
                        "--dangerouslySkipPermissions",
                    ])
                    .output()
                    .await?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    let diff = capture_diff(&code_dir).await;
                    let summary = stdout.chars().take(500).collect();
                    let _ = self
                        .tx
                        .send(AppEvent::PlanCompleted {
                            lobe,
                            filename: plan.filename,
                            summary,
                            diff,
                        })
                        .await;
                } else {
                    let reason = stderr.chars().take(200).collect();
                    let _ = self
                        .tx
                        .send(AppEvent::PlanFailed {
                            lobe,
                            filename: plan.filename,
                            reason,
                        })
                        .await;
                }
            }
            AppEvent::CommandReceived { lobe, text } => {
                eprintln!("[cortex] command received lobe={lobe} text={text:?}");
                let (use_codex, command_text) = strip_codex_marker(&text);
                let (agent_name, body) = extract_agent_and_body(command_text);
                let agent_name = agent_name.unwrap_or_else(|| "synapse".to_owned());
                eprintln!("[cortex] routing to agent={agent_name} use_codex={use_codex}");
                let body = body.trim();

                if body.is_empty() {
                    let _ = self
                        .tx
                        .send(AppEvent::CommandCompleted {
                            lobe,
                            output: "Empty command".to_owned(),
                        })
                        .await;
                    return Ok(());
                }

                let result = if use_codex {
                    let lobe_agents_dir = self
                        .lobes
                        .iter()
                        .find(|configured_lobe| configured_lobe.name == lobe)
                        .map(|configured_lobe| configured_lobe.agents_dir());

                    let prompt = match resolve_agent(
                        &agent_name,
                        lobe_agents_dir.as_ref(),
                        self.global_agents_dir.as_ref(),
                    )
                    .await
                    {
                        Some(content) => format!("{}\n\n---\n\n{}", content, body),
                        None => body.to_owned(),
                    };
                    CodexSupplier.run(&prompt).await
                } else {
                    let agent_mention = format!("@{}", agent_name);
                    let prompt = format!("{agent_mention} {body}");
                    run_claude_command(&prompt).await
                };

                let output = match result {
                    Ok(stdout) => stdout.chars().take(500).collect(),
                    Err(err) => format!("Command failed: {err}").chars().take(500).collect(),
                };

                let _ = self
                    .tx
                    .send(AppEvent::CommandCompleted { lobe, output })
                    .await;
            }
            _ => {}
        }

        Ok(())
    }
}

/// Run `git diff HEAD` in `dir` and, if that fails (not a git root), try each immediate subdir.
/// Returns the first non-empty diff found, or None.
async fn capture_diff(dir: &std::path::Path) -> Option<String> {
    // Try the dir itself first
    if let Some(diff) = git_diff_in(dir).await {
        return Some(diff);
    }
    // Walk immediate subdirs
    let Ok(entries) = std::fs::read_dir(dir) else { return None; };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if let Some(diff) = git_diff_in(&path).await {
                return Some(diff);
            }
        }
    }
    None
}

async fn git_diff_in(dir: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(dir)
        .output()
        .await
        .ok()?;
    if !out.status.success() { return None; }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() { None } else { Some(text) }
}

async fn run_claude_command(prompt: &str) -> anyhow::Result<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let output = Command::new("claude")
        .args([
            "-p",
            prompt,
            "--allowedTools",
            "Bash,Read,Edit,Write,Agent,WebSearch,WebFetch,PushNotification,mcp__obsidian__*,mcp__gemini__*",
            "--add-dir",
            &format!("{}/Stuff", home),
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "claude exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn strip_codex_marker(text: &str) -> (bool, &str) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("!codex") {
        (true, rest.trim_start())
    } else {
        (false, trimmed)
    }
}

fn extract_agent_and_body(text: &str) -> (Option<String>, &str) {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix('@') else {
        return (None, trimmed);
    };

    let agent_len = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .map(|(idx, c)| idx + c.len_utf8())
        .last()
        .unwrap_or(0);

    if agent_len == 0 {
        return (None, trimmed);
    }

    let agent = rest[..agent_len].to_owned();
    let body = rest[agent_len..].trim_start();
    (Some(agent), body)
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

async fn read_plan_supplier(path: &str) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    parse_plan_supplier(&content)
}

async fn read_plan_agent(path: &str) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    parse_plan_agent(&content)
}

fn parse_plan_supplier(content: &str) -> Option<String> {
    parse_frontmatter_field(content, "supplier:").map(|value| value.to_ascii_lowercase())
}

fn parse_plan_agent(content: &str) -> Option<String> {
    parse_frontmatter_field(content, "agent:")
}

fn parse_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let content = content.trim_start();
    let after_open = content.strip_prefix("---")?;
    let close_pos = after_open.find("\n---")?;
    let frontmatter = &after_open[..close_pos];

    frontmatter.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix(field)?.trim();
        let value = value.trim_matches(['"', '\'']);
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

/// Lookup order: lobe-specific → global cortex agents_dir.
/// Never reads from supplier dotfiles (~/.claude/agents, ~/.codex, etc.)
async fn resolve_agent(
    agent_name: &str,
    lobe_agents_dir: Option<&PathBuf>,
    global_agents_dir: Option<&PathBuf>,
) -> Option<String> {
    let filename = format!("{}.md", agent_name);
    for dir in [lobe_agents_dir, global_agents_dir].into_iter().flatten() {
        if let Ok(content) = tokio::fs::read_to_string(dir.join(&filename)).await {
            return Some(content);
        }
    }
    None
}
