use async_trait::async_trait;
use tokio::process::Command;

use super::Supplier;

pub struct GeminiSupplier;

impl GeminiSupplier {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Supplier for GeminiSupplier {
    async fn run(&self, prompt: &str) -> anyhow::Result<String> {
        let output = Command::new("gemini")
            .args(["-p", prompt, "--approval-mode", "yolo", "-o", "text"])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "gemini exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8(output.stdout)?)
    }
}
