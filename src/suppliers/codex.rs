use anyhow::Result;
use tokio::process::Command;

pub struct CodexSupplier;

impl CodexSupplier {
    pub async fn run(&self, prompt: &str) -> Result<String> {
        let output = Command::new("codex")
            .args(["-q", "--approval-mode", "full-auto", prompt])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            anyhow::bail!("codex failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
