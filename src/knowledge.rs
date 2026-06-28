use serde_json::json;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    args::{has_flag, opt_value, required_pos},
    context::Ctx,
    health::{parse_usize_opt, validate_flags},
    manifest::Manifest,
    util::{
        add_index_entry, append_log, canonical_id_for_existing, canonical_id_for_file,
        canonical_id_for_new, canonical_lossy, markdown_files, page_path, read_text,
        resolve_vault_path, slugify, source_files, source_id_for, source_records, today,
        validate_open_path, write_new,
    },
};

pub fn status(ctx: &Ctx) -> Result<i32, String> {
    println!("vault: {}", ctx.vault.display());
    println!("raw_sources: {}", source_files(ctx).len());
    println!("wiki_pages: {}", markdown_files(&ctx.wiki()).len());
    println!(
        "index: {}",
        if ctx.index().exists() {
            "ok"
        } else {
            "missing"
        }
    );
    println!("log: {}", if ctx.log().exists() { "ok" } else { "missing" });
    if ctx.log().exists() {
        let entries: Vec<String> = read_text(&ctx.log())
            .lines()
            .filter(|line| line.starts_with("## ["))
            .map(|line| line.to_string())
            .collect();
        println!("recent_log:");
        for entry in entries
            .iter()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            println!("  {entry}");
        }
    }
    Ok(0)
}

pub fn paths(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--json"])?;
    if has_flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "vault": ctx.vault.display().to_string(),
                "resolution_source": ctx.resolution_source,
                "config_path": crate::args::config_path().display().to_string(),
                "raw": ctx.raw().display().to_string(),
                "assets": ctx.assets().display().to_string(),
                "wiki": ctx.wiki().display().to_string(),
                "index": ctx.index().display().to_string(),
                "log": ctx.log().display().to_string(),
            }))
            .map_err(|err| err.to_string())?
        );
        return Ok(0);
    }
    println!("vault={}", ctx.vault.display());
    println!("raw={}", ctx.raw().display());
    println!("assets={}", ctx.assets().display());
    println!("wiki={}", ctx.wiki().display());
    println!("index={}", ctx.index().display());
    println!("log={}", ctx.log().display());
    Ok(0)
}

pub fn next(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--json"])?;
    let summaries_index = crate::util::SummaryIndex::build(ctx);
    let pending: Vec<PathBuf> = source_files(ctx)
        .into_iter()
        .filter(|path| !summaries_index.contains_source(ctx, path))
        .collect();
    if has_flag(args, "--json") {
        let rows: Vec<_> = pending
            .iter()
            .map(|path| {
                json!({
                    "path": ctx.rel(path),
                    "source_id": source_id_for(ctx, path),
                    "canonical_id": canonical_id_for_existing(path),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"pending_sources": rows, "count": rows.len()}))
                .unwrap()
        );
        return Ok(0);
    }
    if pending.is_empty() {
        println!("No pending raw sources.");
    } else {
        println!("Pending raw sources:");
        for path in &pending {
            println!(
                "- {} ({}, {})",
                ctx.rel(path),
                source_id_for(ctx, path),
                canonical_id_for_existing(path)
            );
        }
        println!("\nSuggested next step:");
        println!("  agents-wiki source-summary \"{}\"", ctx.rel(&pending[0]));
    }
    Ok(0)
}

pub fn new_source(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--url", "--note", "--file", "--force"])?;
    let pos = required_pos(
        args,
        1,
        "new-source <title> [--url URL] [--note NOTE] [--file FILE] [--force]",
    )?;
    fs::create_dir_all(ctx.raw()).map_err(|err| err.to_string())?;
    fs::create_dir_all(ctx.assets()).map_err(|err| err.to_string())?;
    let title = &pos[0];
    let url = opt_value(args, "--url");
    let note = opt_value(args, "--note");
    let file = opt_value(args, "--file");
    let force = has_flag(args, "--force");
    let date = today();
    let slug = format!("{}-{}", date, slugify(title));
    let candidate = if let Some(file) = &file {
        let src = crate::util::expand_home(file);
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        canonical_id_for_file(&src, url.as_ref())
    } else {
        canonical_id_for_new(title, url.as_ref(), note.as_ref())
    };

    for record in source_records(ctx) {
        if record["canonical_id"] == candidate && !force {
            eprintln!(
                "duplicate raw source: {}",
                record["path"].as_str().unwrap_or("")
            );
            eprintln!("canonical_id: {candidate}");
            eprintln!("use --force to add anyway");
            return Ok(2);
        }
    }
    let mut manifest = Manifest::load(ctx)?;

    let dest = if let Some(file) = &file {
        let src = crate::util::expand_home(file);
        let ext = src
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        let dest = ctx.raw().join(format!("{slug}{ext}"));
        if dest.exists() {
            return Err(format!("raw source already exists: {}", ctx.rel(&dest)));
        }
        fs::copy(&src, &dest).map_err(|err| err.to_string())?;
        dest
    } else {
        let dest = ctx.raw().join(format!("{slug}.md"));
        if dest.exists() {
            return Err(format!("raw source already exists: {}", ctx.rel(&dest)));
        }
        let source_id = source_id_for(ctx, &dest);
        let url_line = format!("url: {}", url.clone().unwrap_or_default());
        let content = format!(
            "---\ntitle: {title}\ncreated: {date}\ntype: raw-source\nsource_id: {source_id}\ncanonical_id: {candidate}\n{url_line}\n---\n\n# {title}\n\n{}\n",
            note.clone().unwrap_or_default()
        );
        fs::write(&dest, content).map_err(|err| err.to_string())?;
        dest
    };

    let source_id = source_id_for(ctx, &dest);
    let canonical_id = canonical_id_for_existing(&dest);
    manifest.record_source(ctx, &source_id, &dest, &canonical_id, None);
    manifest.save(ctx)?;
    append_log(
        ctx,
        "source",
        title,
        &[
            format!("Added `{}`.", ctx.rel(&dest)),
            format!("Source id `{source_id}`."),
            format!("Canonical id `{canonical_id}`."),
        ],
    )?;
    println!("{}", ctx.rel(&dest));
    println!("{source_id}");
    println!("{canonical_id}");
    Ok(0)
}

pub fn source_summary(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--title"])?;
    let pos = required_pos(args, 1, "source-summary <raw/path> [--title TITLE]")?;
    let raw = resolve_vault_path(ctx, &pos[0])?;
    if !raw.is_file() {
        return Err(format!("raw source not found: {}", raw.display()));
    }
    if !raw.starts_with(canonical_lossy(&ctx.raw())) {
        return Err(format!("source must be under raw/: {}", raw.display()));
    }
    let title = opt_value(args, "--title")
        .unwrap_or_else(|| raw.file_stem().unwrap().to_string_lossy().to_string());
    let source_id = source_id_for(ctx, &raw);
    let canonical_id = canonical_id_for_existing(&raw);
    let path = page_path(ctx, "source", &title)?;
    let mut manifest = Manifest::load(ctx)?;
    let content = format!(
        "---\ntitle: {title}\nsummary: \"\"\ncreated: {}\ntype: source-summary\nsource_path: {}\nsource_id: {source_id}\ncanonical_id: {canonical_id}\nstatus: draft\nprovenance_source_ids: [{source_id}]\nprovenance_has_inferred_content: false\nprovenance_has_ambiguous_content: false\ntags: [llm-wiki, source]\n---\n\n# {title}\n\n## Summary\n\n## Key Claims\n\n- [ ] Cite `{}`.\n\n## Links\n\n## Follow-Up\n",
        today(),
        ctx.rel(&raw),
        ctx.rel(&raw)
    );
    write_new(ctx, &path, &content)?;
    let section = ctx
        .taxonomy
        .get("source")
        .map(|kind| kind.section.as_str())
        .unwrap_or("Sources");
    add_index_entry(ctx, section, &ctx.rel(&path), &title)?;
    append_log(
        ctx,
        "ingest",
        &title,
        &[format!(
            "Created source summary `{}` for `{}`.",
            ctx.rel(&path),
            ctx.rel(&raw)
        )],
    )?;
    manifest.record_source(ctx, &source_id, &raw, &canonical_id, Some(&path));
    manifest.record_page(ctx, &path, "source-summary", &title, vec![source_id]);
    manifest.save(ctx)?;
    Ok(0)
}

pub fn page(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &[])?;
    let pos = required_pos(args, 2, "page <kind> <title>")?;
    let kind = &pos[0];
    let title = &pos[1];
    // `source` pages are created via `source-summary` (they carry source ids), so
    // they are not creatable here even though they exist in the taxonomy.
    let Some(page_kind) = ctx.taxonomy.get(kind).filter(|item| item.kind != "source") else {
        let allowed = ctx
            .taxonomy
            .kinds()
            .iter()
            .filter(|item| item.kind != "source")
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("kind must be one of: {allowed}"));
    };
    let section = page_kind.section.clone();
    let path = page_path(ctx, kind, title)?;
    let mut manifest = Manifest::load(ctx)?;
    let content = format!(
        "---\ntitle: {title}\nsummary: \"\"\ncreated: {}\ntype: {kind}\nstatus: draft\nsource_count: 0\nprovenance_source_ids: []\nprovenance_has_inferred_content: false\nprovenance_has_ambiguous_content: false\ntags: [llm-wiki]\n---\n\n# {title}\n\n## Summary\n\n## Evidence\n\n## Links\n",
        today()
    );
    write_new(ctx, &path, &content)?;
    add_index_entry(ctx, &section, &ctx.rel(&path), title)?;
    append_log(
        ctx,
        kind,
        title,
        &[format!("Created `{}`.", ctx.rel(&path))],
    )?;
    manifest.record_page(ctx, &path, kind, title, Vec::new());
    manifest.save(ctx)?;
    Ok(0)
}

pub fn review(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--reason", "--source", "--context"])?;
    let pos = required_pos(
        args,
        1,
        "review <title> --reason REASON [--source PATH] [--context TEXT]",
    )?;
    let title = &pos[0];
    let reason = opt_value(args, "--reason").ok_or("--reason is required")?;
    let source = opt_value(args, "--source").unwrap_or_default();
    let context = opt_value(args, "--context").unwrap_or_default();
    let path = page_path(ctx, "review", title)?;
    let mut manifest = Manifest::load(ctx)?;
    let source_text = if source.is_empty() { "TBD" } else { &source };
    let content = format!(
        "---\ntitle: {title}\nsummary: \"\"\ncreated: {}\ntype: review\nstatus: open\nreason: {reason}\nsource_path: {source}\nprovenance_source_ids: []\nprovenance_has_inferred_content: false\nprovenance_has_ambiguous_content: false\ntags: [llm-wiki, review]\n---\n\n# {title}\n\n## Reason\n\n{reason}\n\n## Source\n\n- {source_text}\n\n## Context\n\n{context}\n\n## Decision\n",
        today()
    );
    write_new(ctx, &path, &content)?;
    // Use the taxonomy-resolved section for the review kind.
    let review_section = ctx.taxonomy.section_for("review");
    add_index_entry(ctx, &review_section, &ctx.rel(&path), title)?;
    append_log(
        ctx,
        "review",
        title,
        &[
            format!("Opened review `{}`.", ctx.rel(&path)),
            format!("Reason: {reason}."),
        ],
    )?;
    manifest.record_page(ctx, &path, "review", title, Vec::new());
    manifest.save(ctx)?;
    Ok(0)
}

pub fn reviews(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--status", "--json"])?;
    let status_filter = opt_value(args, "--status");
    let mut rows = Vec::new();
    let reviews_folder = ctx.taxonomy.folder_for("review");
    for path in markdown_files(&ctx.wiki().join(reviews_folder)) {
        let fields = crate::util::frontmatter(&path);
        let status = fields.get("status").cloned().unwrap_or_default();
        if status_filter
            .as_ref()
            .is_some_and(|filter| filter != &status)
        {
            continue;
        }
        rows.push(json!({
            "path": ctx.rel(&path),
            "title": fields.get("title").cloned().unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().to_string()),
            "status": status,
            "reason": fields.get("reason").cloned().unwrap_or_default(),
        }));
    }
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else if rows.is_empty() {
        println!("No review items.");
    } else {
        for row in rows {
            println!(
                "{} | {} | {} | {}",
                row["status"].as_str().unwrap_or("unknown"),
                row["reason"].as_str().unwrap_or("no-reason"),
                row["path"].as_str().unwrap_or(""),
                row["title"].as_str().unwrap_or("")
            );
        }
    }
    Ok(0)
}

pub fn search(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--limit"])?;
    let pos = required_pos(args, 1, "search <query> [--limit N]")?;
    let limit: usize = parse_usize_opt(args, "--limit")?.unwrap_or(20);
    for line in search_matches(ctx, &pos[0], limit) {
        println!("{line}");
    }
    Ok(0)
}

fn recursive_search_files(root: &Path, assets_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(path: &Path, assets_dir: &Path, out: &mut Vec<PathBuf>) {
        if path == assets_dir {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let item = entry.path();
            let Ok(meta) = fs::symlink_metadata(&item) else {
                continue;
            };
            if meta.is_dir() {
                walk(&item, assets_dir, out);
            } else if meta.is_file() {
                if let Some(ext) = item.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if matches!(
                        ext_lower.as_str(),
                        "md" | "txt" | "json" | "yml" | "yaml" | "toml" | "rs"
                    ) {
                        out.push(item);
                    }
                }
            }
        }
    }
    walk(root, assets_dir, &mut out);
    out.sort();
    out
}

fn search_matches(ctx: &Ctx, query: &str, limit: usize) -> Vec<String> {
    let query = query.to_lowercase();
    let mut out = Vec::new();
    let assets_dir = ctx.assets();
    for root in [ctx.raw(), ctx.wiki()] {
        for path in recursive_search_files(&root, &assets_dir) {
            let Ok(file) = File::open(&path) else {
                continue;
            };
            let reader = BufReader::new(file);
            for (index, line) in reader.lines().enumerate() {
                let Ok(line) = line else {
                    continue;
                };
                if line.to_lowercase().contains(&query) {
                    out.push(format!("{}:{}: {}", ctx.rel(&path), index + 1, line.trim()));
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
    }
    out
}

pub fn log(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--line"])?;
    let pos = required_pos(args, 2, "log <kind> <title> [--line LINE]")?;
    let lines: Vec<String> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            if arg == "--line" {
                args.get(index + 1).cloned()
            } else {
                None
            }
        })
        .collect();
    append_log(ctx, &pos[0], &pos[1], &lines)?;
    println!("{}", ctx.rel(&ctx.log()));
    Ok(0)
}

pub fn open(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &[])?;
    let pos = required_pos(args, 1, "open <path>")?;
    // Validate that the path is vault-relative and safe before passing to Obsidian.
    validate_open_path(&pos[0])?;
    let status = Command::new("obsidian")
        .arg(format!(
            "vault={}",
            ctx.vault.file_name().unwrap().to_string_lossy()
        ))
        .arg("open")
        .arg(format!("path={}", pos[0]))
        .status()
        .map_err(|err| err.to_string())?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn search_returns_all_matches_and_respects_limit() {
        let vault = temp_vault("search-matches");
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(ctx.wiki()).unwrap();
        fs::write(
            ctx.wiki().join("note.md"),
            "alpha line\nbeta\nalpha again\nalpha third\n",
        )
        .unwrap();

        let all = search_matches(&ctx, "alpha", 20);
        assert_eq!(all.len(), 3, "expected every matching line: {all:#?}");

        let limited = search_matches(&ctx, "alpha", 2);
        assert_eq!(limited.len(), 2, "limit should cap matches: {limited:#?}");

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn page_scaffold_includes_summary_provenance_and_manifest_record() {
        let vault = temp_vault("page-manifest");
        let ctx = Ctx::new(vault.clone());

        page(&ctx, &args(&["concept", "Test Concept"])).unwrap();

        let path = ctx.wiki().join("concepts/test-concept.md");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("summary: \"\""));
        assert!(text.contains("provenance_source_ids: []"));
        let manifest = Manifest::load(&ctx).unwrap();
        assert_eq!(
            manifest.pages["wiki/concepts/test-concept.md"].page_type,
            "concept"
        );
        assert!(manifest.pages["wiki/concepts/test-concept.md"]
            .source_ids
            .is_empty());

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn new_source_records_manifest_source() {
        let vault = temp_vault("source-manifest");
        let ctx = Ctx::new(vault.clone());

        new_source(&ctx, &args(&["Source Title", "--note", "body"])).unwrap();

        let manifest = Manifest::load(&ctx).unwrap();
        assert_eq!(manifest.sources.len(), 1);
        let source = manifest.sources.values().next().unwrap();
        assert!(source.raw_path.starts_with("raw/"));
        assert!(source.summary_path.is_none());

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn source_summary_records_manifest_source_and_page() {
        let vault = temp_vault("summary-manifest");
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(ctx.raw()).unwrap();
        let raw = ctx.raw().join("sample.md");
        fs::write(
            &raw,
            "---\nsource_id: src-sample\ncanonical_id: can-sample\n---\n\n# Sample\n",
        )
        .unwrap();

        source_summary(&ctx, &args(&["raw/sample.md", "--title", "Sample"])).unwrap();

        let page = ctx.wiki().join("sources/sample.md");
        let text = fs::read_to_string(&page).unwrap();
        assert!(text.contains("summary: \"\""));
        assert!(text.contains("provenance_source_ids: [src-"));
        let manifest = Manifest::load(&ctx).unwrap();
        let source = manifest.sources.values().next().unwrap();
        assert_eq!(
            source.summary_path.as_deref(),
            Some("wiki/sources/sample.md")
        );
        assert_eq!(
            manifest.pages["wiki/sources/sample.md"].page_type,
            "source-summary"
        );

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn mutating_commands_refuse_corrupt_manifest() {
        let vault = temp_vault("corrupt-manifest-command");
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(&vault).unwrap();
        fs::write(ctx.manifest(), "{").unwrap();

        let err = page(&ctx, &args(&["concept", "Blocked"])).unwrap_err();

        assert!(err.contains("failed to parse .manifest.json"));
        assert!(!ctx.wiki().join("concepts/blocked.md").exists());
        fs::remove_dir_all(vault).unwrap();
    }
}
