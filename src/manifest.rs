use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    context::Ctx,
    util::{fs_err, today},
};

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub sources: BTreeMap<String, ManifestSource>,
    pub pages: BTreeMap<String, ManifestPage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ManifestSource {
    pub raw_path: String,
    pub canonical_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_path: Option<String>,
    pub updated: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ManifestPage {
    #[serde(rename = "type")]
    pub page_type: String,
    pub title: String,
    pub source_ids: Vec<String>,
    pub updated: String,
}

impl Manifest {
    pub fn empty() -> Self {
        Self {
            version: MANIFEST_VERSION,
            sources: BTreeMap::new(),
            pages: BTreeMap::new(),
        }
    }

    pub fn load(ctx: &Ctx) -> Result<Self, String> {
        let path = ctx.manifest();
        if !path.exists() {
            return Ok(Self::empty());
        }
        let text = fs::read_to_string(&path).map_err(|err| fs_err(&path, "read", err))?;
        let manifest: Self = serde_json::from_str(&text)
            .map_err(|err| format!("failed to parse {}: {err}", ctx.rel(&path)))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(format!(
                "unsupported manifest version {} in {}",
                manifest.version,
                ctx.rel(&path)
            ));
        }
        Ok(manifest)
    }

    pub fn save(&self, ctx: &Ctx) -> Result<(), String> {
        let path = ctx.manifest();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        fs::write(&path, format!("{text}\n")).map_err(|err| fs_err(&path, "write", err))
    }

    pub fn record_source(
        &mut self,
        ctx: &Ctx,
        source_id: &str,
        raw_path: &Path,
        canonical_id: &str,
        summary_path: Option<&Path>,
    ) {
        let existing_summary = self
            .sources
            .get(source_id)
            .and_then(|entry| entry.summary_path.clone());
        self.sources.insert(
            source_id.to_string(),
            ManifestSource {
                raw_path: ctx.rel(raw_path),
                canonical_id: canonical_id.to_string(),
                summary_path: summary_path.map(|path| ctx.rel(path)).or(existing_summary),
                updated: today(),
            },
        );
    }

    pub fn record_page(
        &mut self,
        ctx: &Ctx,
        path: &Path,
        page_type: &str,
        title: &str,
        source_ids: Vec<String>,
    ) {
        self.pages.insert(
            ctx.rel(path),
            ManifestPage {
                page_type: page_type.to_string(),
                title: title.to_string(),
                source_ids,
                updated: today(),
            },
        );
    }
}

pub fn ensure_exists(ctx: &Ctx) -> Result<bool, String> {
    let path = ctx.manifest();
    if path.exists() {
        return Ok(false);
    }
    Manifest::empty().save(ctx)?;
    Ok(true)
}

pub fn load_for_lint(ctx: &Ctx) -> Option<String> {
    Manifest::load(ctx).err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Ctx;
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_vault(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("agents-wiki-{name}-{nonce}"))
    }

    #[test]
    fn manifest_roundtrips_stable_records() {
        let vault = temp_vault("manifest-roundtrip");
        fs::create_dir_all(&vault).unwrap();
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(ctx.raw()).unwrap();
        fs::create_dir_all(ctx.wiki().join("sources")).unwrap();
        let raw = ctx.raw().join("source.md");
        let page = ctx.wiki().join("sources/source.md");
        fs::write(&raw, "raw").unwrap();
        fs::write(&page, "page").unwrap();

        let mut manifest = Manifest::empty();
        manifest.record_source(&ctx, "src-1", &raw, "can-1", Some(&page));
        manifest.record_page(
            &ctx,
            &page,
            "source-summary",
            "Source",
            vec!["src-1".to_string()],
        );
        manifest.save(&ctx).unwrap();

        let loaded = Manifest::load(&ctx).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.sources["src-1"].summary_path.as_deref(),
            Some("wiki/sources/source.md")
        );
        assert_eq!(loaded.pages["wiki/sources/source.md"].source_ids, ["src-1"]);
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn corrupt_manifest_is_reported() {
        let vault = temp_vault("manifest-corrupt");
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(&vault).unwrap();
        fs::write(ctx.manifest(), "{").unwrap();

        let err = Manifest::load(&ctx).unwrap_err();

        assert!(err.contains("failed to parse .manifest.json"));
        fs::remove_dir_all(vault).unwrap();
    }
}
