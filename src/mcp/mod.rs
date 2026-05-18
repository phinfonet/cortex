use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::config::{Config, Lobe};
use crate::monitor::{AppEvent, PlanMeta};
use crate::router::Router;

pub async fn run_stdio(config: Config) -> anyhow::Result<()> {
    run_stdio_bridge(config.monitor.mcp_socket_path).await
}

pub async fn run_daemon(config: Config, event_tx: mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
    let socket_path = config.monitor.mcp_socket_path.clone();
    McpServer::new(config)
        .with_event_tx(event_tx)
        .run_unix(socket_path)
        .await
}

#[derive(Clone)]
struct McpServer {
    config: Config,
    event_tx: Option<mpsc::Sender<AppEvent>>,
}

impl McpServer {
    fn new(config: Config) -> Self {
        Self {
            config,
            event_tx: None,
        }
    }

    fn with_event_tx(mut self, tx: mpsc::Sender<AppEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    async fn run_unix(self, socket_path: PathBuf) -> anyhow::Result<()> {
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;

        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.clone();
            tokio::spawn(async move {
                let _ = server.handle_stream(stream).await;
            });
        }
    }

    async fn handle_stream(&self, stream: UnixStream) -> anyhow::Result<()> {
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half).lines();

        while let Some(line) = reader.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<Value>(line) {
                Ok(request) => self.handle_message(request).await,
                Err(err) => Some(error_response(Value::Null, -32700, &err.to_string())),
            };

            if let Some(response) = response {
                let mut encoded = serde_json::to_string(&response)?;
                encoded.push('\n');
                write_half.write_all(encoded.as_bytes()).await?;
                write_half.flush().await?;
            }
        }

        Ok(())
    }

    async fn handle_message(&self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        match (id, method) {
            (Some(id), "initialize") => Some(success_response(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "cortex", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            (None, "notifications/initialized") => None,
            (Some(id), "ping") => Some(success_response(id, json!({}))),
            (Some(id), "resources/list") => Some(success_response(id, json!({ "resources": [] }))),
            (Some(id), "tools/list") => Some(success_response(id, json!({ "tools": tools() }))),
            (Some(id), "tools/call") => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Some(success_response(id, self.call_tool(name, args).await))
            }
            (Some(id), _) => Some(error_response(id, -32601, "method not found")),
            (None, _) => None,
        }
    }

    async fn call_tool(&self, name: &str, args: Value) -> Value {
        match self.dispatch_tool(name, args).await {
            Ok(text) => tool_text(text, false),
            Err(err) => tool_text(format!("{err:#}"), true),
        }
    }

    async fn dispatch_tool(&self, name: &str, args: Value) -> anyhow::Result<String> {
        match name {
            "cortex.list_lobes" => self.list_lobes(),
            "cortex.read_vault_file" => self.read_vault_file(args).await,
            "cortex.search_vault" => self.search_vault(args).await,
            "cortex.create_plan" => self.create_plan(args).await,
            "cortex.append_backlog" => self.append_backlog(args).await,
            "cortex.dispatch_command" => self.dispatch_command(args).await,
            "cortex.dispatch_codex" => self.dispatch_codex(args).await,
            "cortex.dispatch_claude" => self.dispatch_claude(args).await,
            "cortex.dispatch_plan" => self.dispatch_plan(args).await,
            _ => anyhow::bail!("unknown tool: {name}"),
        }
    }

    fn list_lobes(&self) -> anyhow::Result<String> {
        let lobes: Vec<Value> = self
            .config
            .lobes
            .iter()
            .map(|lobe| {
                json!({
                    "name": lobe.name,
                    "path": lobe.path,
                    "projects_root": lobe.projects_root
                })
            })
            .collect();

        Ok(serde_json::to_string_pretty(&json!({ "lobes": lobes }))?)
    }

    async fn read_vault_file(&self, args: Value) -> anyhow::Result<String> {
        let lobe = self.required_lobe(&args)?;
        let path = required_string(&args, "path")?;
        let full_path = safe_join(&lobe.path, path)?;
        Ok(tokio::fs::read_to_string(full_path).await?)
    }

    async fn search_vault(&self, args: Value) -> anyhow::Result<String> {
        let lobe = self.required_lobe(&args)?;
        let query = required_string(&args, "query")?.to_ascii_lowercase();
        let limit = optional_usize(&args, "limit").unwrap_or(20).min(100);
        let mut results = Vec::new();
        search_dir(&lobe.path, &query, limit, &mut results)?;
        Ok(serde_json::to_string_pretty(
            &json!({ "results": results }),
        )?)
    }

    async fn create_plan(&self, args: Value) -> anyhow::Result<String> {
        let lobe = self.required_lobe(&args)?;
        let base_path = self.workspace_path(lobe, optional_string(&args, "project"))?;
        let title = required_string(&args, "title")?;
        let body = optional_string(&args, "body").unwrap_or("");
        let agent = optional_string(&args, "agent");
        let supplier = optional_string(&args, "supplier");
        let status = optional_string(&args, "status").unwrap_or("ready");
        let plan_path = create_plan_file(&base_path, title, body, agent, supplier, status).await?;

        Ok(serde_json::to_string_pretty(&json!({
            "path": plan_path,
            "status": status
        }))?)
    }

    async fn append_backlog(&self, args: Value) -> anyhow::Result<String> {
        let lobe = self.required_lobe(&args)?;
        let base_path = self.workspace_path(lobe, optional_string(&args, "project"))?;
        let entry = required_string(&args, "entry")?;
        let backlog_path = base_path.join("tasks").join("backlog.md");

        if let Some(parent) = backlog_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut content = String::new();
        if tokio::fs::try_exists(&backlog_path).await? {
            content = tokio::fs::read_to_string(&backlog_path).await?;
            if !content.ends_with('\n') {
                content.push('\n');
            }
        }
        content.push_str(entry);
        content.push('\n');
        tokio::fs::write(&backlog_path, content).await?;

        Ok(serde_json::to_string_pretty(
            &json!({ "path": backlog_path }),
        )?)
    }

    async fn dispatch_command(&self, args: Value) -> anyhow::Result<String> {
        let lobe = required_string(&args, "lobe")?.to_owned();
        let text = required_string(&args, "text")?.to_owned();
        let timeout_seconds = optional_u64(&args, "timeout_seconds").unwrap_or(600);
        self.run_router_command(lobe, text, timeout_seconds).await
    }

    async fn dispatch_codex(&self, args: Value) -> anyhow::Result<String> {
        let lobe = required_string(&args, "lobe")?.to_owned();
        let prompt = required_string(&args, "prompt")?;
        let agent = optional_string(&args, "agent");
        let timeout_seconds = optional_u64(&args, "timeout_seconds").unwrap_or(600);
        let text = match agent {
            Some(agent) => format!("!codex @{agent} {prompt}"),
            None => format!("!codex {prompt}"),
        };
        self.run_router_command(lobe, text, timeout_seconds).await
    }

    async fn dispatch_claude(&self, args: Value) -> anyhow::Result<String> {
        let lobe = required_string(&args, "lobe")?.to_owned();
        let prompt = required_string(&args, "prompt")?;
        let agent = optional_string(&args, "agent");
        let timeout_seconds = optional_u64(&args, "timeout_seconds").unwrap_or(600);
        let text = match agent {
            Some(agent) => format!("@{agent} {prompt}"),
            None => prompt.to_owned(),
        };
        self.run_router_command(lobe, text, timeout_seconds).await
    }

    async fn dispatch_plan(&self, args: Value) -> anyhow::Result<String> {
        let lobe = required_string(&args, "lobe")?.to_owned();
        let path = required_string(&args, "path")?.to_owned();
        let timeout_seconds = optional_u64(&args, "timeout_seconds").unwrap_or(600);
        let filename = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        let (router, mut rx) = self.router_for_call();
        let event = AppEvent::PlanDetected {
            lobe: lobe.clone(),
            plan: PlanMeta {
                filename: filename.clone(),
                path,
                project: lobe.clone(),
            },
        };

        let duration = Duration::from_secs(timeout_seconds);
        timeout(duration, router.handle(event))
            .await
            .map_err(|_| anyhow::anyhow!("timeout after {timeout_seconds} seconds"))??;

        while let Ok(Some(event)) = timeout(Duration::from_millis(250), rx.recv()).await {
            match event {
                AppEvent::PlanCompleted {
                    lobe: event_lobe,
                    filename: event_filename,
                    summary,
                    diff,
                } if event_lobe == lobe && event_filename == filename => {
                    return Ok(serde_json::to_string_pretty(&json!({
                        "status": "completed",
                        "summary": summary,
                        "diff": diff
                    }))?);
                }
                AppEvent::PlanFailed {
                    lobe: event_lobe,
                    filename: event_filename,
                    reason,
                } if event_lobe == lobe && event_filename == filename => {
                    anyhow::bail!(reason);
                }
                _ => {}
            }
        }

        Ok("Plan dispatched through router.".to_owned())
    }

    async fn run_router_command(
        &self,
        lobe: String,
        text: String,
        timeout_seconds: u64,
    ) -> anyhow::Result<String> {
        let (router, mut rx) = self.router_for_call();
        let event = AppEvent::CommandReceived {
            lobe: lobe.clone(),
            text,
        };

        let duration = Duration::from_secs(timeout_seconds);
        timeout(duration, router.handle(event))
            .await
            .map_err(|_| anyhow::anyhow!("timeout after {timeout_seconds} seconds"))??;

        while let Ok(Some(event)) = timeout(Duration::from_millis(250), rx.recv()).await {
            if let AppEvent::CommandCompleted {
                lobe: event_lobe,
                output,
            } = event
            {
                if event_lobe == lobe {
                    return Ok(output);
                }
            }
        }

        Ok("Command dispatched through router.".to_owned())
    }

    fn router_for_call(&self) -> (Router, mpsc::Receiver<AppEvent>) {
        let (router_tx, mut router_rx) = mpsc::channel::<AppEvent>(64);
        let (observer_tx, observer_rx) = mpsc::channel::<AppEvent>(64);
        let daemon_tx = self.event_tx.clone();

        tokio::spawn(async move {
            while let Some(event) = router_rx.recv().await {
                if let Some(tx) = &daemon_tx {
                    let _ = tx.send(event.clone()).await;
                }
                let _ = observer_tx.send(event).await;
            }
        });

        let router = Router::new(self.config.lobes.clone(), router_tx)
            .with_global_agents_dir(self.config.agents_dir.clone());

        (router, observer_rx)
    }

    fn required_lobe(&self, args: &Value) -> anyhow::Result<&Lobe> {
        let name = required_string(args, "lobe")?;
        self.config
            .lobes
            .iter()
            .find(|lobe| lobe.name == name)
            .ok_or_else(|| anyhow::anyhow!("unknown lobe: {name}"))
    }

    fn workspace_path(&self, lobe: &Lobe, project: Option<&str>) -> anyhow::Result<PathBuf> {
        let Some(project) = project else {
            return Ok(lobe.path.clone());
        };

        let root = lobe.projects_root.as_ref().unwrap_or(&lobe.path);
        safe_join(root, project)
    }
}

async fn run_stdio_bridge(socket_path: PathBuf) -> anyhow::Result<()> {
    let stream = UnixStream::connect(&socket_path)
        .await
        .map_err(|err| anyhow::anyhow!("connect {} failed: {err}", socket_path.display()))?;
    let (read_half, mut write_half) = stream.into_split();
    let mut daemon_reader = BufReader::new(read_half).lines();
    let stdin = tokio::io::stdin();
    let mut stdin_reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = stdin_reader.next_line().await? {
        let waits_response = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value.get("id").is_some(),
            Err(_) => true,
        };

        write_half.write_all(line.as_bytes()).await?;
        write_half.write_all(b"\n").await?;
        write_half.flush().await?;

        if waits_response {
            let Some(response) = daemon_reader.next_line().await? else {
                anyhow::bail!("daemon MCP socket closed");
            };
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

fn tools() -> Value {
    json!([
        {
            "name": "cortex.list_lobes",
            "description": "List configured Cortex lobes and their vault paths.",
            "inputSchema": object_schema([])
        },
        {
            "name": "cortex.read_vault_file",
            "description": "Read a file under a configured lobe path without invoking an AI supplier.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("path", "Relative file path inside the lobe.")
            ])
        },
        {
            "name": "cortex.search_vault",
            "description": "Search markdown files under a configured lobe path without invoking an AI supplier.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("query", "Case-insensitive query matched against path and content."),
                number_prop("limit", "Maximum result count.")
            ])
        },
        {
            "name": "cortex.create_plan",
            "description": "Create a Cortex plan file. This does not invoke an AI supplier by itself.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("project", "Optional project directory under projects_root."),
                string_prop("title", "Plan title."),
                string_prop("body", "Plan body."),
                string_prop("agent", "Optional Cortex agent name."),
                string_prop("supplier", "Optional supplier, e.g. codex."),
                string_prop("status", "Plan status, default ready.")
            ])
        },
        {
            "name": "cortex.append_backlog",
            "description": "Append a line to tasks/backlog.md without invoking an AI supplier.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("project", "Optional project directory under projects_root."),
                string_prop("entry", "Backlog line to append.")
            ])
        },
        {
            "name": "cortex.dispatch_command",
            "description": "Dispatch a command through the existing Cortex router. Prefix text with !codex to route to Codex; otherwise it routes to Claude.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("text", "Router command text."),
                number_prop("timeout_seconds", "Timeout in seconds.")
            ])
        },
        {
            "name": "cortex.dispatch_codex",
            "description": "Dispatch a command through the Cortex router using Codex.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("prompt", "Prompt body."),
                string_prop("agent", "Optional Cortex agent name."),
                number_prop("timeout_seconds", "Timeout in seconds.")
            ])
        },
        {
            "name": "cortex.dispatch_claude",
            "description": "Dispatch a command through the Cortex router using Claude Code.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("prompt", "Prompt body."),
                string_prop("agent", "Optional Cortex agent name."),
                number_prop("timeout_seconds", "Timeout in seconds.")
            ])
        },
        {
            "name": "cortex.dispatch_plan",
            "description": "Dispatch an existing plan file through the Cortex router, preserving supplier selection from plan frontmatter.",
            "inputSchema": object_schema([
                string_prop("lobe", "Configured lobe name."),
                string_prop("path", "Absolute or existing plan path."),
                number_prop("timeout_seconds", "Timeout in seconds.")
            ])
        }
    ])
}

fn object_schema(properties: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in properties {
        map.insert(name.to_owned(), value);
    }

    json!({
        "type": "object",
        "properties": map,
        "additionalProperties": false
    })
}

fn string_prop(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": "string", "description": description }),
    )
}

fn number_prop(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": "number", "description": description }),
    )
}

fn tool_text(text: String, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn required_string<'a>(args: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required string argument: {field}"))
}

fn optional_string<'a>(args: &'a Value, field: &str) -> Option<&'a str> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn optional_u64(args: &Value, field: &str) -> Option<u64> {
    args.get(field).and_then(Value::as_u64)
}

fn optional_usize(args: &Value, field: &str) -> Option<usize> {
    optional_u64(args, field).map(|value| value as usize)
}

fn safe_join(base: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() {
        anyhow::bail!("path must be relative");
    }

    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        anyhow::bail!("path cannot contain parent directory components");
    }

    Ok(base.join(path))
}

fn search_dir(
    dir: &Path,
    query: &str,
    limit: usize,
    results: &mut Vec<Value>,
) -> anyhow::Result<()> {
    if results.len() >= limit {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        if results.len() >= limit {
            break;
        }

        let entry = entry?;
        let path = entry.path();
        let name = path.to_string_lossy().to_ascii_lowercase();

        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == ".git" || name == "target")
            {
                continue;
            }
            search_dir(&path, query, limit, results)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let content_match = content.to_ascii_lowercase().contains(query);
            if name.contains(query) || content_match {
                results.push(json!({ "path": path }));
            }
        }
    }

    Ok(())
}

async fn create_plan_file(
    base_path: &Path,
    title: &str,
    body: &str,
    agent: Option<&str>,
    supplier: Option<&str>,
    status: &str,
) -> anyhow::Result<PathBuf> {
    let dir = base_path.join("plans");
    tokio::fs::create_dir_all(&dir).await?;
    let date = current_date();
    let slug = slugify(title);
    let path = dir.join(format!("{date}-{slug}.md"));

    let mut frontmatter = vec![
        "---".to_owned(),
        "type: plan".to_owned(),
        format!("title: {}", yaml_string(title)),
        format!("status: {status}"),
    ];

    if let Some(agent) = agent {
        frontmatter.push(format!("agent: {agent}"));
    }
    if let Some(supplier) = supplier {
        frontmatter.push(format!("supplier: {supplier}"));
    }

    frontmatter.push("---".to_owned());
    let content = format!("{}\n\n{}\n", frontmatter.join("\n"), body.trim());
    tokio::fs::write(&path, content).await?;
    Ok(path)
}

fn current_date() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|date| date.trim().to_owned())
        .filter(|date| !date.is_empty())
        .unwrap_or_else(|| "undated".to_owned())
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;

    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        slug.to_owned()
    }
}

fn yaml_string(text: &str) -> String {
    format!("{:?}", text)
}
