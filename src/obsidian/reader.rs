use anyhow::{Result, bail};
use tokio::process::Command;

pub struct SearchResult {
    pub path: String,
}

pub async fn read(path: &str) -> Result<String> {
    let output = Command::new("obsidian")
        .args(["read", &format!("path={}", path)])
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "obsidian read failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?)
}

pub async fn search(query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let output = Command::new("obsidian")
        .args([
            "search",
            &format!("query={}", query),
            &format!("limit={}", limit),
        ])
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "obsidian search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    let results = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| SearchResult {
            path: l.to_string(),
        })
        .collect();

    Ok(results)
}
