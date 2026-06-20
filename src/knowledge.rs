use serde_json::json;
use std::{ffi::OsStr, fs, path::PathBuf, process::Command};

use crate::{
    args::{has_flag, opt_value, required_pos},
    context::Ctx,
    util::{
        add_index_entry, append_log, canonical_id_for_existing, canonical_id_for_file,
        canonical_id_for_new, markdown_files, page_path, read_text, recursive_files,
        resolve_vault_path, slugify, source_files, source_id_for, source_records, summary_exists,
        today, write_new,
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

pub fn paths(ctx: &Ctx) -> Result<i32, String> {
    println!("vault={}", ctx.vault.display());
    println!("raw={}", ctx.raw().display());
    println!("assets={}", ctx.assets().display());
    println!("wiki={}", ctx.wiki().display());
    println!("archive={}", ctx.archive().display());
    println!("trash={}", ctx.trash().display());
    println!("index={}", ctx.index().display());
    println!("log={}", ctx.log().display());
    Ok(0)
}

pub fn next(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let pending: Vec<PathBuf> = source_files(ctx)
        .into_iter()
        .filter(|path| !summary_exists(ctx, path))
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
    let pos = required_pos(args, 1, "source-summary <raw/path> [--title TITLE]")?;
    let raw = resolve_vault_path(ctx, &pos[0])?;
    if !raw.is_file() {
        return Err(format!("raw source not found: {}", raw.display()));
    }
    if !raw.starts_with(ctx.raw()) {
        return Err(format!("source must be under raw/: {}", raw.display()));
    }
    let title = opt_value(args, "--title")
        .unwrap_or_else(|| raw.file_stem().unwrap().to_string_lossy().to_string());
    let source_id = source_id_for(ctx, &raw);
    let canonical_id = canonical_id_for_existing(&raw);
    let path = page_path(ctx, "source", &title)?;
    let content = format!(
        "---\ntitle: {title}\ncreated: {}\ntype: source-summary\nsource_path: {}\nsource_id: {source_id}\ncanonical_id: {canonical_id}\nstatus: draft\ntags: [llm-wiki, source]\n---\n\n# {title}\n\n## Summary\n\n## Key Claims\n\n- [ ] Cite `{}`.\n\n## Links\n\n## Follow-Up\n",
        today(),
        ctx.rel(&raw),
        ctx.rel(&raw)
    );
    write_new(ctx, &path, &content)?;
    add_index_entry(ctx, "Sources", &ctx.rel(&path), &title)?;
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
    Ok(0)
}

pub fn page(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let pos = required_pos(args, 2, "page entity|concept|question|review <title>")?;
    let kind = &pos[0];
    let title = &pos[1];
    if !["entity", "concept", "question", "review"].contains(&kind.as_str()) {
        return Err("kind must be entity, concept, question, or review".to_string());
    }
    let path = page_path(ctx, kind, title)?;
    let content = format!(
        "---\ntitle: {title}\ncreated: {}\ntype: {kind}\nstatus: draft\nsource_count: 0\ntags: [llm-wiki]\n---\n\n# {title}\n\n## Summary\n\n## Evidence\n\n## Links\n",
        today()
    );
    write_new(ctx, &path, &content)?;
    let section = match kind.as_str() {
        "entity" => "Entities",
        "concept" => "Concepts",
        "question" => "Questions",
        "review" => "Reviews",
        _ => unreachable!("kind validated above"),
    };
    add_index_entry(ctx, section, &ctx.rel(&path), title)?;
    append_log(
        ctx,
        kind,
        title,
        &[format!("Created `{}`.", ctx.rel(&path))],
    )?;
    Ok(0)
}

pub fn review(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
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
    let source_text = if source.is_empty() { "TBD" } else { &source };
    let content = format!(
        "---\ntitle: {title}\ncreated: {}\ntype: review\nstatus: open\nreason: {reason}\nsource_path: {source}\ntags: [llm-wiki, review]\n---\n\n# {title}\n\n## Reason\n\n{reason}\n\n## Source\n\n- {source_text}\n\n## Context\n\n{context}\n\n## Decision\n",
        today()
    );
    write_new(ctx, &path, &content)?;
    add_index_entry(ctx, "Reviews", &ctx.rel(&path), title)?;
    append_log(
        ctx,
        "review",
        title,
        &[
            format!("Opened review `{}`.", ctx.rel(&path)),
            format!("Reason: {reason}."),
        ],
    )?;
    Ok(0)
}

pub fn reviews(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let status_filter = opt_value(args, "--status");
    let mut rows = Vec::new();
    for path in markdown_files(&ctx.wiki().join("reviews")) {
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
    let pos = required_pos(args, 1, "search <query> [--limit N]")?;
    let limit: usize = opt_value(args, "--limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    for line in search_matches(ctx, &pos[0], limit) {
        println!("{line}");
    }
    Ok(0)
}

fn search_matches(ctx: &Ctx, query: &str, limit: usize) -> Vec<String> {
    let query = query.to_lowercase();
    let mut out = Vec::new();
    for root in [ctx.raw(), ctx.wiki()] {
        for path in recursive_files(&root) {
            if path.extension() == Some(OsStr::new("pyc")) {
                continue;
            }
            let text = read_text(&path);
            for (index, line) in text.lines().enumerate() {
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
    let pos = required_pos(args, 1, "open <path>")?;
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
}
