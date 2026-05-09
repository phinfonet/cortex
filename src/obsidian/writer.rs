use anyhow::{Result, bail};
use tokio::process::Command;

pub async fn append(path: &str, content: &str) -> Result<()> {
    let output = Command::new("obsidian")
        .args([
            "append",
            &format!("path={}", path),
            &format!("content={}", content),
            "silent",
        ])
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "obsidian append failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

pub async fn create(name: &str, content: &str) -> Result<()> {
    let output = Command::new("obsidian")
        .args([
            "create",
            &format!("name={}", name),
            &format!("content={}", content),
            "silent",
        ])
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "obsidian create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}
