use serde::Deserialize;
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

/// Reserved top-level names that taxonomy folders must not collide with.
const RESERVED_FOLDERS: &[&str] = &["raw", ".git", ".obsidian"];

/// One page kind in the wiki taxonomy: how a `kind` maps to a `wiki/<folder>/`
/// directory and a `## <section>` heading in `index.md`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PageKind {
    pub kind: String,
    pub folder: String,
    pub section: String,
}

/// The wiki taxonomy. Loaded from the `taxonomy:` frontmatter of the vault's
/// `AGENTS.md`, falling back to the built-in default so structure can be
/// co-evolved per domain without recompiling the CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct Taxonomy {
    kinds: Vec<PageKind>,
}

#[derive(Deserialize)]
struct AgentsFrontmatter {
    taxonomy: Option<Vec<PageKind>>,
}

/// Validate that a taxonomy folder value is safe to use as a path component
/// under `wiki/`. Rejects empty strings, absolute paths, parent traversal,
/// path separators, `.git`, and other reserved roots.
pub fn validate_taxonomy_folder(folder: &str) -> Result<(), String> {
    if folder.is_empty() {
        return Err("taxonomy folder must not be empty".to_string());
    }
    if folder.contains('/') || folder.contains('\\') {
        return Err(format!(
            "taxonomy folder must be a single path component, got: {folder:?}"
        ));
    }
    if folder == ".." || folder == "." {
        return Err(format!(
            "taxonomy folder must not be a relative path component: {folder:?}"
        ));
    }
    if folder.starts_with('/') {
        return Err(format!(
            "taxonomy folder must not be an absolute path: {folder:?}"
        ));
    }
    if RESERVED_FOLDERS.contains(&folder) {
        return Err(format!(
            "taxonomy folder conflicts with a reserved directory: {folder:?}"
        ));
    }
    Ok(())
}

impl Taxonomy {
    pub fn default_taxonomy() -> Self {
        let kinds = [
            ("source", "sources", "Sources"),
            ("entity", "entities", "Entities"),
            ("concept", "concepts", "Concepts"),
            ("question", "questions", "Questions"),
            ("review", "reviews", "Reviews"),
        ]
        .into_iter()
        .map(|(kind, folder, section)| PageKind {
            kind: kind.to_string(),
            folder: folder.to_string(),
            section: section.to_string(),
        })
        .collect();
        Self { kinds }
    }

    pub fn load(vault: &Path) -> Self {
        let text = std::fs::read_to_string(vault.join("AGENTS.md")).unwrap_or_default();
        Self::from_agents_text(&text).unwrap_or_else(Self::default_taxonomy)
    }

    fn from_agents_text(text: &str) -> Option<Self> {
        let body = text.strip_prefix("---\n")?;
        let end = body.find("\n---")?;
        let parsed: AgentsFrontmatter = serde_yaml::from_str(&body[..end]).ok()?;
        let kinds = parsed.taxonomy.filter(|kinds| !kinds.is_empty())?;
        // Silently drop any kind with an invalid folder so a bad entry in
        // AGENTS.md does not prevent the CLI from starting, but only keeps
        // entries that are structurally safe.
        let safe_kinds: Vec<PageKind> = kinds
            .into_iter()
            .filter(|kind| validate_taxonomy_folder(&kind.folder).is_ok())
            .collect();
        if safe_kinds.is_empty() {
            return None;
        }
        Some(Self { kinds: safe_kinds })
    }

    pub fn kinds(&self) -> &[PageKind] {
        &self.kinds
    }

    pub fn get(&self, kind: &str) -> Option<&PageKind> {
        self.kinds.iter().find(|item| item.kind == kind)
    }

    /// Return the folder for a given kind, or a sensible hardcoded default
    /// so callers that depend on `source`/`review` always get a valid answer
    /// even if the taxonomy was customised to omit those core kinds.
    pub fn folder_for<'a>(&'a self, kind: &'a str) -> &'a str {
        self.get(kind).map(|k| k.folder.as_str()).unwrap_or(kind)
    }

    /// Return the section heading for a given kind, with a capitalised fallback.
    pub fn section_for(&self, kind: &str) -> String {
        self.get(kind)
            .map(|k| k.section.clone())
            .unwrap_or_else(|| {
                let mut s = kind.to_string();
                if let Some(first) = s.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                s
            })
    }
}

#[derive(Clone)]
pub struct Ctx {
    pub vault: PathBuf,
    pub taxonomy: Taxonomy,
}

impl Ctx {
    pub fn new(vault: PathBuf) -> Self {
        let taxonomy = Taxonomy::load(&vault);
        Self { vault, taxonomy }
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
        let mut dirs = vec![self.raw(), self.assets(), self.wiki()];
        for kind in self.taxonomy.kinds() {
            // Folders have already been validated at taxonomy load time, so
            // joining them here is safe.
            dirs.push(self.wiki().join(&kind.folder));
        }
        dirs
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_default_taxonomy_without_frontmatter() {
        assert_eq!(Taxonomy::from_agents_text("# no frontmatter\n"), None);
    }

    #[test]
    fn loads_custom_taxonomy_from_frontmatter() {
        let text = "---\ntitle: Schema\ntaxonomy:\n  - kind: person\n    folder: people\n    section: People\n---\n\n# Body\n";
        let taxonomy = Taxonomy::from_agents_text(text).expect("custom taxonomy");
        assert_eq!(
            taxonomy.get("person"),
            Some(&PageKind {
                kind: "person".to_string(),
                folder: "people".to_string(),
                section: "People".to_string(),
            })
        );
        assert!(taxonomy.get("entity").is_none());
    }

    #[test]
    fn empty_taxonomy_list_falls_back_to_default() {
        let text = "---\ntaxonomy: []\n---\n\n# Body\n";
        assert_eq!(Taxonomy::from_agents_text(text), None);
    }

    #[test]
    fn validate_taxonomy_folder_rejects_unsafe_paths() {
        assert!(validate_taxonomy_folder("").is_err());
        assert!(validate_taxonomy_folder("..").is_err());
        assert!(validate_taxonomy_folder(".").is_err());
        assert!(validate_taxonomy_folder("a/b").is_err());
        assert!(validate_taxonomy_folder("/tmp").is_err());
        assert!(validate_taxonomy_folder(".git").is_err());
        assert!(validate_taxonomy_folder("raw").is_err());
    }

    #[test]
    fn validate_taxonomy_folder_accepts_safe_paths() {
        assert!(validate_taxonomy_folder("concepts").is_ok());
        assert!(validate_taxonomy_folder("my-notes").is_ok());
        assert!(validate_taxonomy_folder("people").is_ok());
    }

    #[test]
    fn taxonomy_silently_drops_unsafe_folder_entries() {
        let text = "---\ntaxonomy:\n  - kind: bad\n    folder: ../outside\n    section: Bad\n  - kind: good\n    folder: concepts\n    section: Concepts\n---\n\n# Schema\n";
        let taxonomy = Taxonomy::from_agents_text(text).expect("should have safe entries");
        assert!(
            taxonomy.get("bad").is_none(),
            "unsafe folder must be dropped"
        );
        assert!(taxonomy.get("good").is_some(), "safe folder must be kept");
    }

    #[test]
    fn taxonomy_all_unsafe_falls_back_to_default() {
        let text = "---\ntaxonomy:\n  - kind: bad\n    folder: .git/hooks\n    section: Bad\n---\n\n# Schema\n";
        // from_agents_text returns None when all entries are dropped
        assert_eq!(Taxonomy::from_agents_text(text), None);
    }
}
