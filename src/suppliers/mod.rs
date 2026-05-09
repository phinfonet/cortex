pub mod codex;
pub mod gemini;

#[async_trait::async_trait]
pub trait Supplier: Send + Sync {
    async fn run(&self, prompt: &str) -> anyhow::Result<String>;
}
