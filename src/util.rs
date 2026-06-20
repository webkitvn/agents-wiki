use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use url::Url;

use crate::context::Ctx;

pub fn today() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

pub fn expand_home(input: &str) -> PathBuf {
    if input == "~" {
        return PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string()));
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string())).join(rest);
    }
    PathBuf::from(input)
}

pub fn canonical_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

pub fn stable_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn source_id_for(ctx: &Ctx, path: &Path) -> String {
    format!("src-{}", stable_hash(&ctx.rel(path)))
}

fn normalize_url(input: &str) -> String {
    if let Ok(mut url) = Url::parse(input.trim()) {
        url.set_fragment(None);
        let mut pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        pairs.sort();
        url.set_query(None);
        if !pairs.is_empty() {
            let query = pairs
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("&");
            url.set_query(Some(&query));
        }
        let path = url.path().trim_end_matches('/').to_string();
        if path.is_empty() {
            url.set_path("/");
        } else {
            url.set_path(&path);
        }
        url.to_string()
    } else {
        input.trim().to_string()
    }
}

pub fn canonical_id_from_url(url: &str) -> String {
    format!("can-url-{}", stable_hash(&normalize_url(url)))
}

pub fn canonical_id_from_content(text: &str) -> String {
    format!("can-text-{}", stable_hash(text.trim()))
}

pub fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub fn frontmatter(path: &Path) -> BTreeMap<String, String> {
    if path.extension() != Some(OsStr::new("md")) || !path.exists() {
        return BTreeMap::new();
    }
    let text = read_text(path);
    if !text.starts_with("---\n") {
        return BTreeMap::new();
    }
    let Some(end) = text[4..].find("\n---") else {
        return BTreeMap::new();
    };
    let mut fields = BTreeMap::new();
    for line in text[4..4 + end].lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

pub fn canonical_id_for_existing(path: &Path) -> String {
    let fields = frontmatter(path);
    if let Some(id) = fields.get("canonical_id").filter(|value| !value.is_empty()) {
        return id.clone();
    }
    if let Some(url) = fields.get("url").filter(|value| !value.is_empty()) {
        return canonical_id_from_url(url);
    }
    canonical_id_from_content(&read_text(path))
}

pub fn canonical_id_for_new(title: &str, url: Option<&String>, note: Option<&String>) -> String {
    if let Some(url) = url.filter(|value| !value.is_empty()) {
        canonical_id_from_url(url)
    } else {
        canonical_id_from_content(&format!("{}\n{}", title, note.cloned().unwrap_or_default()))
    }
}

pub fn canonical_id_for_file(path: &Path, url: Option<&String>) -> String {
    if let Some(url) = url.filter(|value| !value.is_empty()) {
        return canonical_id_from_url(url);
    }
    let fields = frontmatter(path);
    if let Some(id) = fields.get("canonical_id").filter(|value| !value.is_empty()) {
        return id.clone();
    }
    if let Some(url) = fields.get("url").filter(|value| !value.is_empty()) {
        return canonical_id_from_url(url);
    }
    canonical_id_from_content(&read_text(path))
}

pub fn source_files(ctx: &Ctx) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(ctx.raw()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name() != Some(OsStr::new("README.md")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

pub fn recursive_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(path: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let item = entry.path();
                if item.is_dir() {
                    walk(&item, out);
                } else if item.is_file() {
                    out.push(item);
                }
            }
        }
    }
    walk(root, &mut out);
    out.sort();
    out
}

pub fn markdown_files(root: &Path) -> Vec<PathBuf> {
    recursive_files(root)
        .into_iter()
        .filter(|path| path.extension() == Some(OsStr::new("md")))
        .collect()
}

pub fn page_path(ctx: &Ctx, kind: &str, title: &str) -> Result<PathBuf, String> {
    let folder = match kind {
        "source" => ctx.wiki().join("sources"),
        "entity" => ctx.wiki().join("entities"),
        "concept" => ctx.wiki().join("concepts"),
        "question" => ctx.wiki().join("questions"),
        "review" => ctx.wiki().join("reviews"),
        _ => return Err(format!("unsupported page kind: {kind}")),
    };
    Ok(folder.join(format!("{}.md", slugify(title))))
}

pub fn write_new(ctx: &Ctx, path: &Path, content: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("file already exists: {}", ctx.rel(path)));
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())?;
    println!("{}", ctx.rel(path));
    Ok(())
}

pub fn append_log(ctx: &Ctx, kind: &str, title: &str, lines: &[String]) -> Result<(), String> {
    fs::create_dir_all(ctx.log().parent().unwrap()).map_err(|err| err.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ctx.log())
        .map_err(|err| err.to_string())?;
    writeln!(file, "\n## [{}] {} | {}\n", today(), kind, title).map_err(|err| err.to_string())?;
    for line in lines {
        writeln!(file, "- {line}").map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn update_frontmatter_field(path: &Path, key: &str, value: &str) -> Result<(), String> {
    if path.extension() != Some(OsStr::new("md")) || !path.exists() {
        return Ok(());
    }
    let text = read_text(path);
    let line = format!("{key}: {value}");
    if text.starts_with("---\n") {
        if let Some(end_rel) = text[4..].find("\n---") {
            let end = 4 + end_rel;
            let mut header: Vec<String> =
                text[4..end].lines().map(|item| item.to_string()).collect();
            let mut replaced = false;
            for existing in &mut header {
                if existing.starts_with(&format!("{key}:")) {
                    *existing = line.clone();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                header.push(line);
            }
            let body = &text[end..];
            return fs::write(path, format!("---\n{}{}", header.join("\n"), body))
                .map_err(|err| err.to_string());
        }
    }
    fs::write(path, format!("---\n{line}\n---\n\n{text}")).map_err(|err| err.to_string())
}

pub fn resolve_vault_path(ctx: &Ctx, value: &str) -> Result<PathBuf, String> {
    let path = expand_home(value);
    let path = if path.is_absolute() {
        path
    } else {
        ctx.vault.join(path)
    };
    let path = canonical_lossy(&path);
    let vault = canonical_lossy(&ctx.vault);
    if !path.starts_with(&vault) {
        return Err(format!("path must be inside vault: {}", path.display()));
    }
    let rel = path.strip_prefix(&vault).unwrap_or(&path);
    if path == vault
        || rel
            .components()
            .any(|component| component.as_os_str() == ".git")
    {
        return Err(format!(
            "refusing to operate on protected path: {}",
            ctx.rel(&path)
        ));
    }
    Ok(path)
}

pub fn unique_destination(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 2.. {
        let candidate = path.with_file_name(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

pub fn move_path(src: &Path, dest: &Path) -> Result<PathBuf, String> {
    let dest = unique_destination(dest);
    fs::create_dir_all(dest.parent().unwrap()).map_err(|err| err.to_string())?;
    fs::rename(src, &dest)
        .or_else(|_| {
            if src.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "cross-device directory move unsupported",
                ))
            } else {
                fs::copy(src, &dest)?;
                fs::remove_file(src)?;
                Ok(())
            }
        })
        .map_err(|err| err.to_string())?;
    Ok(dest)
}

pub fn source_records(ctx: &Ctx) -> Vec<Value> {
    source_files(ctx)
        .into_iter()
        .map(|path| {
            json!({
                "path": ctx.rel(&path),
                "source_id": source_id_for(ctx, &path),
                "canonical_id": canonical_id_for_existing(&path),
            })
        })
        .collect()
}

pub fn summary_exists(ctx: &Ctx, raw_path: &Path) -> bool {
    let summaries = ctx.wiki().join("sources");
    if !summaries.exists() {
        return false;
    }
    let raw_rel = ctx.rel(raw_path);
    let raw_id = source_id_for(ctx, raw_path);
    let raw_canonical_id = canonical_id_for_existing(raw_path);
    markdown_files(&summaries).into_iter().any(|page| {
        let text = read_text(&page);
        text.contains(&raw_rel) || text.contains(&raw_id) || text.contains(&raw_canonical_id)
    })
}
