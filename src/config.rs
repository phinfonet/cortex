use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub monitor: MonitorConfig,
    pub lobes: Vec<Lobe>,
    pub suppliers: Vec<Supplier>,
    pub agents_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorConfig {
    pub socket_path: PathBuf,
    #[serde(default = "default_ui_socket_path")]
    pub ui_socket_path: PathBuf,
    #[serde(default = "default_mcp_socket_path")]
    pub mcp_socket_path: PathBuf,
}

fn default_ui_socket_path() -> PathBuf {
    PathBuf::from("/tmp/cortex-ui.sock")
}

fn default_mcp_socket_path() -> PathBuf {
    PathBuf::from("/tmp/cortex-mcp.sock")
}

#[derive(Debug, Deserialize, Clone)]
pub struct Lobe {
    pub name: String,
    pub path: PathBuf,
    /// Root dir scanned for project subdirectories. Uses `.lobe` marker to filter by lobe name.
    pub projects_root: Option<PathBuf>,
}

impl Lobe {
    pub fn agents_dir(&self) -> PathBuf {
        self.path.join("agents")
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Supplier {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: SupplierKind,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum SupplierKind {
    Gemini,
    ClaudeCode,
    Codex,
}

pub fn load() -> anyhow::Result<Config> {
    let xdg_path = dirs_path().join("cortex").join("cortex.toml");
    let local_path = Path::new("./cortex.toml");

    let path = if xdg_path.exists() {
        xdg_path
    } else if local_path.exists() {
        local_path.to_path_buf()
    } else {
        anyhow::bail!(
            "config not found: tried {} and ./cortex.toml",
            xdg_path.display()
        )
    };

    let raw = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&raw)?;
    Ok(config)
}

fn dirs_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_home()
                .map(|h| h.join(".config"))
                .unwrap_or_else(|| PathBuf::from("/tmp"))
        })
}

fn dirs_home() -> Option<PathBuf> {
    dirs::home_dir()
}
