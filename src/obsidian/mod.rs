mod reader;
mod writer;

pub use reader::SearchResult;

use anyhow::Result;

pub struct Client;

impl Client {
    pub fn new() -> Self {
        Self
    }

    pub async fn read(&self, path: &str) -> Result<String> {
        reader::read(path).await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        reader::search(query, limit).await
    }

    pub async fn append(&self, path: &str, content: &str) -> Result<()> {
        writer::append(path, content).await
    }

    pub async fn create(&self, name: &str, content: &str) -> Result<()> {
        writer::create(name, content).await
    }
}
