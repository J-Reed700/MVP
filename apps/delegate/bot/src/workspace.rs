use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Workspace manages the filesystem-as-memory directory.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    pub fn new(path: &str) -> Self {
        Self {
            root: PathBuf::from(path),
        }
    }

    /// Load a workspace file, returning empty string if it doesn't exist.
    pub async fn load(&self, filename: &str) -> String {
        let path = self.root.join(filename);
        fs::read_to_string(&path).await.unwrap_or_default()
    }

    /// Write a workspace file.
    pub async fn save(&self, filename: &str, content: &str) -> Result<()> {
        let path = self.root.join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, content).await?;
        Ok(())
    }

    /// Get the full path to the workspace root.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Load IDENTITY.md
    pub async fn identity(&self) -> String {
        self.load("IDENTITY.md").await
    }

    /// Load INTENTS.md
    pub async fn intents(&self) -> String {
        self.load("INTENTS.md").await
    }

    /// Load MEMORY.md
    pub async fn memory(&self) -> String {
        self.load("MEMORY.md").await
    }

    /// Load HEARTBEAT.md and extract watched channels.
    pub async fn watched_channels(&self) -> Vec<String> {
        let heartbeat = self.load("HEARTBEAT.md").await;
        let mut channels = Vec::new();
        let mut in_channels_section = false;

        for line in heartbeat.lines() {
            if line.contains("Watched Channels") {
                in_channels_section = true;
                continue;
            }
            if in_channels_section {
                if line.starts_with('#') || line.starts_with("##") {
                    break;
                }
                let trimmed = line.trim().trim_start_matches('-').trim();
                if trimmed.starts_with('#') {
                    // Extract channel name (remove any trailing description)
                    let channel = trimmed.split_whitespace().next().unwrap_or(trimmed);
                    channels.push(channel.trim_start_matches('#').to_string());
                }
            }
        }

        channels
    }
}
