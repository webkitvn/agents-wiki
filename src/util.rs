use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    env,
    ffi::OsStr,
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};
use url::Url;

use crate::context::Ctx;

/// Return today's date as `YYYY-MM-DD` using the system clock without
/// shelling out to `date`.
pub fn today() -> String {
    // Use std::time to get the unix timestamp and compute date components.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Days since epoch
    let day_number = secs.div_euclid(86400);
    civil_date(day_number)
}

/// Convert a count of days since 1970-01-01 (Unix epoch) to a `YYYY-MM-DD`
/// string using Howard Hinnant's proleptic Gregorian algorithm.
fn civil_date(z: i64) -> String {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 }.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
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

/// Join `relative` onto `base` and verify the result stays inside `base`.
/// Rejects absolute paths, `..` components, `.git` components, and symlinks
/// that escape the base directory.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return Err("path component must not be empty".to_string());
    }
    let joined = PathBuf::from(relative);
    if joined.is_absolute() {
        return Err(format!("path must not be absolute: {relative:?}"));
    }
    for component in joined.components() {
        let s = component.as_os_str().to_string_lossy();
        if s == ".." {
            return Err(format!(
                "path must not contain parent traversal: {relative:?}"
            ));
        }
        if s == ".git" {
            return Err(format!("path must not reference .git: {relative:?}"));
        }
    }
    let full = base.join(&joined);
    // After joining, check that the lexical prefix is still inside base.
    // We use a lexical check here (not canonicalize) because the path may not
    // exist yet (write paths). Symlink traversal is caught by using
    // symlink_metadata (not follow_symlinks) in walk routines.
    if !full.starts_with(base) {
        return Err(format!("path escapes base directory: {relative:?}"));
    }
    Ok(full)
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

pub fn stable_hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
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

pub fn canonical_id_from_bytes(data: &[u8]) -> String {
    format!("can-bytes-{}", stable_hash_bytes(data))
}

pub fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Emit an IO error message that includes the file path and the action.
pub fn fs_err(path: &Path, action: &str, err: std::io::Error) -> String {
    format!("failed to {action} {}: {err}", path.display())
}

pub fn frontmatter(path: &Path) -> BTreeMap<String, String> {
    if path.extension() != Some(OsStr::new("md")) || !path.exists() {
        return BTreeMap::new();
    }
    let Ok(file) = fs::File::open(path) else {
        return BTreeMap::new();
    };
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();

    if reader
        .read_line(&mut line)
        .ok()
        .filter(|_| {
            let trimmed = line.trim_end();
            trimmed == "---"
        })
        .is_none()
    {
        return BTreeMap::new();
    }

    let mut fields = BTreeMap::new();
    loop {
        line.clear();
        let Ok(n) = reader.read_line(&mut line) else {
            break;
        };
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed == "---" {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
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
    // Hash raw bytes so binary files (PDFs, images) each get a unique ID
    // instead of collapsing to the same empty-content hash.
    match fs::read(path) {
        Ok(bytes) => canonical_id_from_bytes(&bytes),
        Err(_) => canonical_id_from_content(""),
    }
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
    // Hash raw bytes for binary files so each distinct file gets a unique ID.
    match fs::read(path) {
        Ok(bytes) => canonical_id_from_bytes(&bytes),
        Err(_) => canonical_id_from_content(""),
    }
}

pub fn source_files(ctx: &Ctx) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(ctx.raw()) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Use symlink_metadata so we don't follow symlinks into unknown territory.
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_file() && path.file_name() != Some(OsStr::new("README.md")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Walk `root` collecting regular files, without following symlinked
/// directories, to prevent walking outside the vault.
pub fn recursive_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(path: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let item = entry.path();
            // Use symlink_metadata so symlinked directories are NOT followed.
            let Ok(meta) = fs::symlink_metadata(&item) else {
                continue;
            };
            if meta.is_dir() {
                walk(&item, out);
            } else if meta.is_file() {
                out.push(item);
            }
            // Symlinks to files or dirs are skipped intentionally.
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
    let folder = ctx
        .taxonomy
        .get(kind)
        .ok_or_else(|| format!("unsupported page kind: {kind}"))?
        .folder
        .as_str();
    // folder has already been validated at taxonomy load time, but we use
    // safe_join here as a defence-in-depth check.
    let wiki_subfolder = safe_join(&ctx.wiki(), folder)?;
    Ok(wiki_subfolder.join(format!("{}.md", slugify(title))))
}

pub fn write_new(ctx: &Ctx, path: &Path, content: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("file already exists: {}", ctx.rel(path)));
    }
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
    fs::write(path, content).map_err(|err| fs_err(path, "write", err))?;
    println!("{}", ctx.rel(path));
    Ok(())
}

pub fn append_log(ctx: &Ctx, kind: &str, title: &str, lines: &[String]) -> Result<(), String> {
    let log = ctx.log();
    let parent = log.parent().unwrap();
    fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|err| fs_err(&log, "open", err))?;
    writeln!(file, "\n## [{}] {} | {}\n", today(), kind, title)
        .map_err(|err| fs_err(&log, "write", err))?;
    for line in lines {
        writeln!(file, "- {line}").map_err(|err| fs_err(&log, "write", err))?;
    }
    Ok(())
}

/// Validate that `value` is a vault-relative path for an **existing** file or
/// directory. Expands `~`, resolves symlinks, and rejects paths outside the
/// vault or referencing `.git`.
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

/// Validate that `value` is a vault-relative path suitable as an **open**
/// target (non-mutating). Rejects absolute paths, `..`, and `.git` components.
pub fn validate_open_path(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("path must not be empty".to_string());
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return Err(format!("path must be relative: {value:?}"));
    }
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        if s == ".." {
            return Err(format!("path must not contain parent traversal: {value:?}"));
        }
        if s == ".git" {
            return Err(format!("path must not reference .git: {value:?}"));
        }
    }
    Ok(())
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

/// Insert a wikilink entry under the given `## {section}` heading in `index.md`.
/// Idempotent: does nothing if the target page is already referenced.
pub fn add_index_entry(
    ctx: &Ctx,
    section: &str,
    rel_path: &str,
    title: &str,
) -> Result<(), String> {
    let index = ctx.index();
    let link_target = rel_path.strip_suffix(".md").unwrap_or(rel_path);
    let entry = format!("- [[{link_target}]] — {title}");
    let mut text = read_text(&index);
    // Use a structural check: look for the exact wikilink syntax, not substring.
    if text
        .lines()
        .any(|line| line.contains(&format!("[[{link_target}]]")))
    {
        return Ok(());
    }
    let heading = format!("## {section}");
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    if let Some(start) = lines.iter().position(|line| line.trim() == heading) {
        let mut insert_at = lines.len();
        for (offset, line) in lines.iter().enumerate().skip(start + 1) {
            if line.starts_with("## ") {
                insert_at = offset;
                break;
            }
        }
        while insert_at > start + 1 && lines[insert_at - 1].trim().is_empty() {
            insert_at -= 1;
        }
        lines.insert(insert_at, entry);
        text = lines.join("\n");
        if !text.ends_with('\n') {
            text.push('\n');
        }
    } else {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("\n{heading}\n\n{entry}\n"));
    }
    let parent = index.parent().unwrap();
    fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
    let tmp = index.with_extension("tmp");
    fs::write(&tmp, text).map_err(|err| fs_err(&tmp, "write index tmp", err))?;
    fs::rename(&tmp, &index).map_err(|err| fs_err(&index, "rename index", err))
}

/// Whole days from `start` to `end` for `YYYY-MM-DD` dates (negative if end precedes start).
pub fn days_between(start: &str, end: &str) -> Option<i64> {
    let (sy, sm, sd) = parse_ymd(start)?;
    let (ey, em, ed) = parse_ymd(end)?;
    Some(days_from_civil(ey, em, ed) - days_from_civil(sy, sm, sd))
}

fn parse_ymd(text: &str) -> Option<(i64, i64, i64)> {
    let mut parts = text.trim().splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.trim().parse().ok()?;
    Some((year, month, day))
}

/// Days since 1970-01-01 (Howard Hinnant's proleptic Gregorian algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub struct SummaryIndex {
    by_source_path: HashSet<String>,
    by_source_id: HashSet<String>,
    by_canonical_id: HashSet<String>,
    fallback_pages: Vec<PathBuf>,
}

impl SummaryIndex {
    pub fn build(ctx: &Ctx) -> Self {
        let mut index = Self {
            by_source_path: HashSet::new(),
            by_source_id: HashSet::new(),
            by_canonical_id: HashSet::new(),
            fallback_pages: Vec::new(),
        };

        let source_folder = ctx.taxonomy.folder_for("source");
        let summaries = ctx.wiki().join(source_folder);
        if !summaries.exists() {
            return index;
        }

        for page in markdown_files(&summaries) {
            let fm = frontmatter(&page);
            let mut has_metadata = false;
            if let Some(v) = fm.get("source_path").filter(|v| !v.is_empty()) {
                index.by_source_path.insert(v.clone());
                has_metadata = true;
            }
            if let Some(v) = fm.get("source_id").filter(|v| !v.is_empty()) {
                index.by_source_id.insert(v.clone());
                has_metadata = true;
            }
            if let Some(v) = fm.get("canonical_id").filter(|v| !v.is_empty()) {
                index.by_canonical_id.insert(v.clone());
                has_metadata = true;
            }
            if !has_metadata {
                index.fallback_pages.push(page);
            }
        }

        index
    }

    pub fn contains_source(&self, ctx: &Ctx, path: &Path) -> bool {
        let rel = ctx.rel(path);
        if self.by_source_path.contains(&rel) {
            return true;
        }

        let source_id = source_id_for(ctx, path);
        if self.by_source_id.contains(&source_id) {
            return true;
        }

        let canonical_id = canonical_id_for_existing(path);
        if self.by_canonical_id.contains(&canonical_id) {
            return true;
        }

        if !self.fallback_pages.is_empty() {
            return self.fallback_pages.iter().any(|page| {
                let text = read_text(page);
                text.contains(&rel) || text.contains(&source_id) || text.contains(&canonical_id)
            });
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_vault(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("agents-wiki-{name}-{nonce}"))
    }

    #[test]
    fn summary_index_resolves_metadata_and_fallbacks() {
        let vault = temp_vault("summary-index-test");
        let ctx = Ctx::new(vault.clone());
        let source_folder = ctx.taxonomy.folder_for("source");
        let summaries_dir = ctx.wiki().join(source_folder);
        fs::create_dir_all(&summaries_dir).unwrap();
        fs::create_dir_all(ctx.raw()).unwrap();

        let raw_file = ctx.raw().join("test-source.md");
        fs::write(
            &raw_file,
            "---\nsource_id: src-123\ncanonical_id: can-123\n---\n",
        )
        .unwrap();

        // 1. Initially empty
        let index = SummaryIndex::build(&ctx);
        assert!(!index.contains_source(&ctx, &raw_file));

        // 2. Add summary with frontmatter metadata
        let summary_file1 = summaries_dir.join("summary1.md");
        fs::write(
            &summary_file1,
            "---\nsource_path: raw/test-source.md\nsource_id: src-123\ncanonical_id: can-123\n---\n",
        )
        .unwrap();

        let index = SummaryIndex::build(&ctx);
        assert!(index.contains_source(&ctx, &raw_file));

        // 3. Add summary page without metadata but matching in body
        fs::remove_file(&summary_file1).unwrap();
        let summary_file2 = summaries_dir.join("summary2.md");
        fs::write(&summary_file2, "Reference to raw/test-source.md in body\n").unwrap();

        let index = SummaryIndex::build(&ctx);
        assert!(index.contains_source(&ctx, &raw_file));

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn add_index_entry_files_under_section_and_is_idempotent() {
        let vault = temp_vault("index-entry");
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(ctx.wiki()).unwrap();
        fs::write(ctx.index(), "# Wiki Index\n\n## Sources\n\n## Concepts\n").unwrap();

        add_index_entry(&ctx, "Sources", "wiki/sources/foo.md", "Foo").unwrap();
        add_index_entry(&ctx, "Sources", "wiki/sources/foo.md", "Foo").unwrap();

        let text = read_text(&ctx.index());
        assert_eq!(
            text.matches("[[wiki/sources/foo]]").count(),
            1,
            "entry must be idempotent: {text}"
        );
        let sources_pos = text.find("## Sources").unwrap();
        let concepts_pos = text.find("## Concepts").unwrap();
        let entry_pos = text.find("[[wiki/sources/foo]]").unwrap();
        assert!(
            entry_pos > sources_pos && entry_pos < concepts_pos,
            "entry must sit under Sources: {text}"
        );

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn days_between_handles_ordering_and_invalid_input() {
        assert_eq!(days_between("2026-01-01", "2026-01-31"), Some(30));
        assert_eq!(days_between("2026-03-01", "2026-02-01"), Some(-28));
        assert_eq!(days_between("not-a-date", "2026-01-01"), None);
    }

    #[test]
    fn today_returns_valid_date_format() {
        let date = today();
        assert_eq!(date.len(), 10, "expected YYYY-MM-DD format: {date}");
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn civil_date_epoch_is_1970_01_01() {
        assert_eq!(civil_date(0), "1970-01-01");
    }

    #[test]
    fn safe_join_rejects_parent_traversal() {
        let base = PathBuf::from("/vault");
        assert!(safe_join(&base, "..").is_err());
        assert!(safe_join(&base, "../outside").is_err());
        assert!(safe_join(&base, ".git").is_err());
        assert!(safe_join(&base, "/tmp").is_err());
        assert!(safe_join(&base, "").is_err());
    }

    #[test]
    fn safe_join_allows_normal_components() {
        let base = PathBuf::from("/vault");
        assert!(safe_join(&base, "wiki").is_ok());
        assert!(safe_join(&base, "wiki/concepts").is_ok());
    }

    #[test]
    fn validate_open_path_rejects_unsafe_inputs() {
        assert!(validate_open_path("").is_err());
        assert!(validate_open_path("/absolute").is_err());
        assert!(validate_open_path("../outside").is_err());
        assert!(validate_open_path(".git/config").is_err());
    }

    #[test]
    fn validate_open_path_accepts_relative_paths() {
        assert!(validate_open_path("wiki/concepts/foo.md").is_ok());
        assert!(validate_open_path("raw/source.md").is_ok());
    }

    #[test]
    fn binary_canonical_id_is_unique_per_content() {
        let bytes_a = b"PDF binary content here \x00\x01\x02";
        let bytes_b = b"Different PDF content \x00\x01\x03";
        let id_a = canonical_id_from_bytes(bytes_a);
        let id_b = canonical_id_from_bytes(bytes_b);
        assert_ne!(id_a, id_b, "distinct binary files must get distinct IDs");
        assert!(id_a.starts_with("can-bytes-"));
    }
}
