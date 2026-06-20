use std::path::{Path, PathBuf};

use crate::util::canonical_lossy;

pub const DEFAULT_VAULT: &str = "~/Documents/Agents Wiki";
pub const CONFIG_PATH: &str = "~/.agents-wiki/config.yml";
pub const GITIGNORE_RULES: &[&str] = &[
    ".DS_Store",
    "bin/__pycache__/",
    "*.pyc",
    "**/.obsidian/workspace.json",
    "**/.obsidian/workspace-mobile.json",
    "**/.obsidian/cache/",
    "**/.obsidian/plugins/*/data.json",
    "target/",
];

#[derive(Clone)]
pub struct Ctx {
    pub vault: PathBuf,
}

impl Ctx {
    pub fn new(vault: PathBuf) -> Self {
        Self { vault }
    }

    pub fn raw(&self) -> PathBuf {
        self.vault.join("raw")
    }

    pub fn assets(&self) -> PathBuf {
        self.raw().join("assets")
    }

    pub fn wiki(&self) -> PathBuf {
        self.vault.join("wiki")
    }

    pub fn archive(&self) -> PathBuf {
        self.wiki().join("archive")
    }

    pub fn trash(&self) -> PathBuf {
        self.vault.join("trash")
    }

    pub fn trash_manifest(&self) -> PathBuf {
        self.trash().join("manifest.jsonl")
    }

    pub fn log(&self) -> PathBuf {
        self.wiki().join("log.md")
    }

    pub fn index(&self) -> PathBuf {
        self.wiki().join("index.md")
    }

    pub fn agents(&self) -> PathBuf {
        self.vault.join("AGENTS.md")
    }

    pub fn entrypoint(&self) -> PathBuf {
        self.vault.join("LLM Wiki.md")
    }

    pub fn gitignore(&self) -> PathBuf {
        self.vault.join(".gitignore")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.raw(),
            self.assets(),
            self.wiki(),
            self.wiki().join("sources"),
            self.wiki().join("entities"),
            self.wiki().join("concepts"),
            self.wiki().join("questions"),
            self.wiki().join("reviews"),
            self.archive(),
            self.trash(),
        ]
    }

    pub fn rel(&self, path: &Path) -> String {
        let abs = canonical_lossy(path);
        let vault = canonical_lossy(&self.vault);
        abs.strip_prefix(&vault)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or_else(|| path.to_str().unwrap_or(""))
            .trim_start_matches('/')
            .to_string()
    }
}
