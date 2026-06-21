use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
};

use crate::{
    args::{has_flag, opt_value},
    context::{Ctx, Taxonomy, GITIGNORE_RULES},
    util::{
        append_log, days_between, frontmatter, markdown_files, read_text, source_files,
        summary_exists, today,
    },
};

const DEFAULT_STALE_DAYS: i64 = 90;

pub fn lint(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let stale_days = opt_value(args, "--stale-days")
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_STALE_DAYS);
    let report = lint_report(ctx, stale_days);
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        let errors = report["errors"].as_array().unwrap();
        let warnings = report["warnings"].as_array().unwrap();
        for item in errors {
            println!("ERROR {}", item.as_str().unwrap());
        }
        for item in warnings {
            println!("WARN {}", item.as_str().unwrap());
        }
        if errors.is_empty() && warnings.is_empty() {
            println!("ok");
        }
    }
    Ok(if report["errors"].as_array().unwrap().is_empty() {
        0
    } else {
        1
    })
}

pub fn doctor(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let mut repaired = Vec::new();
    if has_flag(args, "--repair") {
        repaired = repair_doctor(ctx)?;
    }
    let mut report = doctor_report(ctx);
    if !repaired.is_empty() {
        report["repaired"] = json!(repaired);
    }
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("vault: {}", report["vault"].as_str().unwrap());
        println!(
            "healthy: {}",
            if report["healthy"].as_bool().unwrap() {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "git_initialized: {}",
            if report["state"]["git_initialized"].as_bool().unwrap() {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "git_dirty: {}",
            report["state"]["git_dirty"]
                .as_array()
                .map(|items| items.len())
                .unwrap_or(0)
        );
        println!(
            "pending_sources: {}",
            report["state"]["pending_sources"].as_array().unwrap().len()
        );
        println!(
            "open_reviews: {}",
            report["state"]["open_reviews"].as_array().unwrap().len()
        );
        if let Some(items) = report.get("repaired").and_then(|value| value.as_array()) {
            println!("repaired:");
            for item in items {
                println!("  - {}", item.as_str().unwrap());
            }
        }
        if report["issues"].as_array().unwrap().is_empty() {
            println!("issues: none");
        } else {
            println!("issues:");
            for issue in report["issues"].as_array().unwrap() {
                let label = issue
                    .get("path")
                    .or_else(|| issue.get("rule"))
                    .or_else(|| issue.get("message"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                println!(
                    "  - {} {}: {}",
                    issue["severity"].as_str().unwrap(),
                    issue["code"].as_str().unwrap(),
                    label
                );
            }
        }
    }
    Ok(
        if report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["severity"] == "error")
        {
            1
        } else {
            0
        },
    )
}

pub fn lint_report(ctx: &Ctx, stale_days: i64) -> Value {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let today_str = today();
    for required in [ctx.raw(), ctx.assets(), ctx.wiki(), ctx.index(), ctx.log()] {
        if !required.exists() {
            errors.push(format!("missing {}", ctx.rel(&required)));
        }
    }

    let index_text = read_text(&ctx.index());
    for page in markdown_files(&ctx.wiki()) {
        let text = read_text(&page);
        if !text
            .lines()
            .any(|line| line.starts_with("# ") && line.len() > 2)
        {
            warnings.push(format!("missing H1: {}", ctx.rel(&page)));
        }
        if page
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with('_'))
            || page == ctx.index()
            || page == ctx.log()
        {
            continue;
        }
        if !index_text.contains(&ctx.rel(&page))
            && !index_text.contains(page.file_stem().unwrap().to_str().unwrap())
        {
            warnings.push(format!("wiki page missing from index: {}", ctx.rel(&page)));
        }
        let fields = frontmatter(&page);
        if fields.get("status").is_some_and(|value| value == "active")
            && !text.contains("raw/")
            && !text.contains("source_id:")
        {
            warnings.push(format!(
                "active wiki page may lack raw citation: {}",
                ctx.rel(&page)
            ));
        }
        if fields.get("status").is_some_and(|value| value == "active") {
            if let Some(date) = fields.get("updated").or_else(|| fields.get("created")) {
                if let Some(age) = days_between(date, &today_str) {
                    if age > stale_days {
                        warnings.push(format!(
                            "stale active page ({age}d since {date}): {}",
                            ctx.rel(&page)
                        ));
                    }
                }
            }
        }
        if page.parent().and_then(|parent| parent.file_name()) == Some(OsStr::new("sources"))
            && fields.get("status").is_some_and(|value| value == "draft")
        {
            warnings.push(format!("draft source summary: {}", ctx.rel(&page)));
        }
    }

    for raw in source_files(ctx) {
        if raw.extension() == Some(OsStr::new("md")) {
            let fields = frontmatter(&raw);
            if !fields.contains_key("source_id") {
                warnings.push(format!(
                    "markdown raw source missing source_id frontmatter: {}",
                    ctx.rel(&raw)
                ));
            }
            if !fields.contains_key("canonical_id") {
                warnings.push(format!(
                    "markdown raw source missing canonical_id frontmatter: {}",
                    ctx.rel(&raw)
                ));
            }
        }
        if !summary_exists(ctx, &raw) {
            warnings.push(format!(
                "raw source may lack wiki summary: {}",
                ctx.rel(&raw)
            ));
        }
    }

    let mut source_ids: BTreeMap<String, std::path::PathBuf> = BTreeMap::new();
    let mut canonical_ids: BTreeMap<String, std::path::PathBuf> = BTreeMap::new();
    for page in markdown_files(&ctx.wiki()) {
        let fields = frontmatter(&page);
        if let Some(id) = fields.get("source_id") {
            if let Some(prev) = source_ids.insert(id.clone(), page.clone()) {
                warnings.push(format!(
                    "duplicate source_id `{}`: {} and {}",
                    id,
                    ctx.rel(&prev),
                    ctx.rel(&page)
                ));
            }
        }
        if let Some(id) = fields.get("canonical_id") {
            if let Some(prev) = canonical_ids.insert(id.clone(), page.clone()) {
                warnings.push(format!(
                    "duplicate canonical_id `{}`: {} and {}",
                    id,
                    ctx.rel(&prev),
                    ctx.rel(&page)
                ));
            }
        }
    }

    let mut raw_canonical_ids = BTreeMap::new();
    for raw in source_files(ctx) {
        let id = crate::util::canonical_id_for_existing(&raw);
        if let Some(prev) = raw_canonical_ids.insert(id.clone(), raw.clone()) {
            warnings.push(format!(
                "duplicate raw canonical_id `{}`: {} and {}",
                id,
                ctx.rel(&prev),
                ctx.rel(&raw)
            ));
        }
    }

    let wiki_pages = markdown_files(&ctx.wiki());
    let page_texts: Vec<(std::path::PathBuf, String)> = wiki_pages
        .iter()
        .map(|page| (page.clone(), read_text(page)))
        .collect();
    for page in &wiki_pages {
        if page == &ctx.index() || page == &ctx.log() {
            continue;
        }
        if page
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with('_'))
        {
            continue;
        }
        let stem = page.file_stem().and_then(|value| value.to_str()).unwrap();
        let rel = ctx.rel(page);
        let link_no_ext = rel.strip_suffix(".md").unwrap_or(&rel);
        let inbound = page_texts.iter().any(|(other, text)| {
            other != page && (text.contains(link_no_ext) || text.contains(&format!("[[{stem}")))
        });
        if !inbound {
            warnings.push(format!("orphan wiki page (no inbound links): {}", rel));
        }
    }

    let taxonomy_folders: BTreeSet<&str> = ctx
        .taxonomy
        .kinds()
        .iter()
        .map(|kind| kind.folder.as_str())
        .collect();
    for page in &wiki_pages {
        if page == &ctx.index() || page == &ctx.log() {
            continue;
        }
        if page
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with('_'))
        {
            continue;
        }
        let rel = ctx.rel(page);
        let under = rel.strip_prefix("wiki/").unwrap_or(&rel);
        let top = under.split('/').next().unwrap_or("");
        let in_taxonomy = under.contains('/') && taxonomy_folders.contains(top);
        if !in_taxonomy {
            warnings.push(format!(
                "off-taxonomy wiki page (decide: move it into a taxonomy folder, or add the kind to AGENTS.md `taxonomy`): {}",
                rel
            ));
        }
    }

    json!({"errors": errors, "warnings": warnings})
}

fn doctor_report(ctx: &Ctx) -> Value {
    let mut issues = Vec::new();
    for dir in ctx.required_dirs() {
        if !dir.exists() {
            issues.push(json!({"severity": "error", "code": "missing_dir", "path": ctx.rel(&dir), "repairable": true}));
        }
    }
    for file in [
        ctx.agents(),
        ctx.entrypoint(),
        ctx.index(),
        ctx.log(),
        ctx.gitignore(),
    ] {
        if !file.exists() {
            issues.push(json!({"severity": "error", "code": "missing_file", "path": ctx.rel(&file), "repairable": true}));
        }
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("agents-wiki"));
    let cli_executable = exe
        .metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);
    if !cli_executable {
        issues.push(json!({"severity": "error", "code": "cli_not_executable", "path": exe.display().to_string(), "repairable": true}));
    }
    let git_initialized = git_repo_exists(ctx);
    if !git_initialized {
        issues.push(json!({"severity": "warning", "code": "git_not_initialized", "path": ".", "repairable": true}));
    }
    for rule in missing_gitignore_rules(ctx) {
        issues.push(json!({"severity": "warning", "code": "missing_gitignore_rule", "rule": rule, "repairable": true}));
    }
    let lint = lint_report(ctx, DEFAULT_STALE_DAYS);
    for item in lint["errors"].as_array().unwrap() {
        issues.push(json!({"severity": "error", "code": "lint_error", "message": item.as_str().unwrap(), "repairable": false}));
    }
    for item in lint["warnings"].as_array().unwrap() {
        issues.push(json!({"severity": "warning", "code": "lint_warning", "message": item.as_str().unwrap(), "repairable": false}));
    }
    let healthy = !issues.iter().any(|issue| issue["severity"] == "error");
    json!({
        "vault": ctx.vault.display().to_string(),
        "healthy": healthy,
        "issues": issues,
        "state": {
            "pending_sources": pending_source_items(ctx),
            "open_reviews": open_review_items(ctx),
            "git_initialized": git_initialized,
            "git_dirty": git_dirty_status(ctx),
            "cli_executable": cli_executable,
        }
    })
}

fn repair_doctor(ctx: &Ctx) -> Result<Vec<String>, String> {
    let mut repaired = Vec::new();
    for dir in ctx.required_dirs() {
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
            repaired.push(format!("Created `{}`.", ctx.rel(&dir)));
        }
    }
    for (path, content) in [
        (ctx.agents(), agents_skeleton()),
        (ctx.index(), index_skeleton(ctx)),
        (ctx.log(), log_skeleton()),
        (ctx.entrypoint(), entrypoint_skeleton()),
    ] {
        if write_if_missing(&path, content)? {
            repaired.push(format!("Created `{}`.", ctx.rel(&path)));
        }
    }

    let missing = missing_gitignore_rules(ctx);
    if !missing.is_empty() {
        let mut existing = read_text(&ctx.gitignore());
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&missing.join("\n"));
        existing.push('\n');
        fs::write(ctx.gitignore(), existing).map_err(|err| err.to_string())?;
        repaired.push(format!("Updated `{}`.", ctx.rel(&ctx.gitignore())));
    }
    if !git_repo_exists(ctx)
        && Command::new("git")
            .arg("init")
            .current_dir(&ctx.vault)
            .status()
            .map_err(|err| err.to_string())?
            .success()
    {
        repaired.push("Initialized git repository.".to_string());
    }
    if !repaired.is_empty() {
        append_log(ctx, "doctor", "repair", &repaired)?;
    }
    Ok(repaired)
}

fn write_if_missing(path: &Path, content: String) -> Result<bool, String> {
    if path.exists() {
        return Ok(false);
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())?;
    Ok(true)
}

fn index_skeleton(ctx: &Ctx) -> String {
    let mut text = format!(
        "---\ntitle: Wiki Index\ncreated: {}\ntype: wiki-index\ntags: [llm-wiki, index]\n---\n\n# Wiki Index\n",
        crate::util::today()
    );
    for kind in ctx.taxonomy.kinds() {
        text.push_str(&format!("\n## {}\n", kind.section));
    }
    text
}

fn log_skeleton() -> String {
    format!("---\ntitle: Wiki Log\ncreated: {}\ntype: wiki-log\ntags: [llm-wiki, log]\n---\n\n# Wiki Log\n", crate::util::today())
}

fn agents_skeleton() -> String {
    let mut taxonomy = String::new();
    for kind in Taxonomy::default_taxonomy().kinds() {
        taxonomy.push_str(&format!(
            "  - kind: {}\n    folder: {}\n    section: {}\n",
            kind.kind, kind.folder, kind.section
        ));
    }
    format!(
        r#"---
title: Agents Wiki Contract
type: wiki-schema
tags: [llm-wiki, schema]
# taxonomy maps each page `kind` to its `wiki/<folder>/` directory and its
# `## <section>` heading in index.md. Edit this list to fit your domain, then run
# `agents-wiki doctor --repair` to scaffold any new folders. No recompile needed.
taxonomy:
{taxonomy}---

# Agents Wiki — LLM Wiki Contract

This file is the schema. It tells you (the LLM) how this wiki is structured and how
to maintain it. You own the wiki; co-evolve this file as the conventions change.

## Layers

- `raw/` — immutable source files. Read them; never edit them. This is the source of truth.
- `wiki/` — agent-maintained markdown, organised by the `taxonomy` above.
- `wiki/index.md` — the catalog (one section per taxonomy kind).
- `wiki/log.md` — the append-only timeline of ingests, pages, and lint passes.

## Page conventions

- Every page starts with YAML frontmatter (`title`, `created`, `type`, `status`, `tags`)
  and a single `# H1`.
- `status: draft` until reviewed; `status: active` once it cites at least one `raw/` source.
- Active pages must cite their evidence: link the `raw/` path or include its `source_id:`.
- Cross-link liberally with `[[wikilinks]]`; an orphan page (no inbound links) is flagged by lint.

## Choosing a kind

- `entity` — a concrete thing: person, org, product, place.
- `concept` — an idea, theme, or topic that spans sources.
- `question` — an open question or a durable answer worth keeping.
- `review` — a flag for a contradiction or claim that needs a human/agent decision.

## Operations

- Ingest: `agents-wiki new-source` then `agents-wiki source-summary`. The CLI files the
  summary into `index.md` and `log.md`. Then YOU do the synthesis — a single source often
  touches 10-15 pages: update related entity/concept pages, add cross-links, and open a
  `review` when a new source contradicts an existing claim.
- Query: `agents-wiki search` to find pages, read them, answer with citations to `raw/`
  sources. File durable answers back with `agents-wiki page question "..."` so they compound.
- Lint: `agents-wiki lint` surfaces missing index entries, missing citations, duplicate ids,
  orphan pages, and stale pages. Resolving contradictions is your job, not the CLI's.

## Workflow

- Run `agents-wiki doctor` before autonomous work and `agents-wiki lint` before finishing.
- Versioning, deletion, and history are handled by git — this CLI does not manage a trash.
"#
    )
}

fn entrypoint_skeleton() -> String {
    format!("---\ntitle: LLM Wiki\ncreated: {}\ntype: vault-entrypoint\ntags: [llm-wiki, index]\n---\n\n# LLM Wiki\n\n- [[wiki/index]]\n- [[wiki/log]]\n", crate::util::today())
}

fn git_repo_exists(ctx: &Ctx) -> bool {
    git_output(ctx, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
    })
}

fn git_dirty_status(ctx: &Ctx) -> Option<Vec<String>> {
    if !git_repo_exists(ctx) {
        return None;
    }
    git_output(ctx, &["status", "--short"]).map(|output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.to_string())
            .collect()
    })
}

fn git_output(ctx: &Ctx, args: &[&str]) -> Option<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(&ctx.vault)
        .output()
        .ok()
}

fn missing_gitignore_rules(ctx: &Ctx) -> Vec<String> {
    let existing: BTreeSet<String> = read_text(&ctx.gitignore())
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    GITIGNORE_RULES
        .iter()
        .filter(|rule| !existing.contains(**rule))
        .map(|rule| rule.to_string())
        .collect()
}

fn open_review_items(ctx: &Ctx) -> Vec<String> {
    markdown_files(&ctx.wiki().join("reviews"))
        .into_iter()
        .filter(|path| {
            frontmatter(path)
                .get("status")
                .map(|status| status == "open")
                .unwrap_or(true)
        })
        .map(|path| ctx.rel(&path))
        .collect()
}

fn pending_source_items(ctx: &Ctx) -> Vec<String> {
    source_files(ctx)
        .into_iter()
        .filter(|path| !summary_exists(ctx, path))
        .map(|path| ctx.rel(&path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
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
    fn repair_reports_created_core_notes() {
        let vault = temp_vault("repair-core-notes");
        let ctx = Ctx::new(vault.clone());

        let repaired = repair_doctor(&ctx).unwrap();

        for expected in [
            "Created `wiki/index.md`.",
            "Created `wiki/log.md`.",
            "Created `AGENTS.md`.",
            "Created `LLM Wiki.md`.",
        ] {
            assert!(
                repaired.iter().any(|item| item == expected),
                "missing repair entry: {expected}\nactual: {repaired:#?}"
            );
        }
        assert!(ctx.index().exists());
        assert!(ctx.log().exists());
        assert!(ctx.agents().exists());
        assert!(ctx.entrypoint().exists());

        let repaired_again = repair_doctor(&ctx).unwrap();
        assert!(!repaired_again
            .iter()
            .any(|item| item == "Created `wiki/index.md`."));

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn scaffolded_agents_taxonomy_roundtrips_and_creates_folders() {
        let vault = temp_vault("taxonomy-roundtrip");
        let ctx = Ctx::new(vault.clone());

        repair_doctor(&ctx).unwrap();

        // The embedded AGENTS.md frontmatter must parse back to the default taxonomy.
        assert_eq!(Taxonomy::load(&vault), Taxonomy::default_taxonomy());

        // Every taxonomy folder must be scaffolded, and the index must hold each section.
        let index = read_text(&ctx.index());
        for kind in Taxonomy::default_taxonomy().kinds() {
            assert!(
                ctx.wiki().join(&kind.folder).is_dir(),
                "missing folder for {}",
                kind.kind
            );
            assert!(
                index.contains(&format!("## {}", kind.section)),
                "index missing section {}",
                kind.section
            );
        }

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn custom_taxonomy_drives_repair_folders() {
        let vault = temp_vault("taxonomy-custom");
        fs::create_dir_all(&vault).unwrap();
        fs::write(
            vault.join("AGENTS.md"),
            "---\ntaxonomy:\n  - kind: person\n    folder: people\n    section: People\n---\n\n# Schema\n",
        )
        .unwrap();
        let ctx = Ctx::new(vault.clone());

        repair_doctor(&ctx).unwrap();

        assert!(ctx.wiki().join("people").is_dir());
        assert!(!ctx.wiki().join("concepts").exists());
        assert!(read_text(&ctx.index()).contains("## People"));

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn lint_flags_off_taxonomy_pages() {
        let vault = temp_vault("off-taxonomy");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        // Page inside a known taxonomy folder: allowed.
        fs::write(
            ctx.wiki().join("concepts").join("ok.md"),
            "---\ntitle: Ok\ntype: concept\n---\n\n# Ok\n",
        )
        .unwrap();
        // Page in an unknown subfolder: flagged.
        fs::create_dir_all(ctx.wiki().join("notes")).unwrap();
        fs::write(
            ctx.wiki().join("notes").join("stray.md"),
            "---\ntitle: Stray\ntype: note\n---\n\n# Stray\n",
        )
        .unwrap();
        // Page loose directly under wiki/: flagged.
        fs::write(
            ctx.wiki().join("loose.md"),
            "---\ntitle: Loose\ntype: note\n---\n\n# Loose\n",
        )
        .unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);
        let warnings: Vec<&str> = report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        let off: Vec<&&str> = warnings
            .iter()
            .filter(|item| item.starts_with("off-taxonomy wiki page"))
            .collect();
        assert!(off.iter().any(|item| item.contains("notes/stray.md")));
        assert!(off.iter().any(|item| item.contains("wiki/loose.md")));
        assert!(
            !off.iter().any(|item| item.contains("concepts/ok.md")),
            "page in a taxonomy folder must not be flagged: {warnings:#?}"
        );

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn lint_flags_orphan_but_not_linked_pages() {
        let vault = temp_vault("orphan-lint");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("hub.md"),
            "---\ntitle: Hub\ntype: concept\n---\n\n# Hub\n\nSee [[linked]].\n",
        )
        .unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("linked.md"),
            "---\ntitle: Linked\ntype: concept\n---\n\n# Linked\n",
        )
        .unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("orphan.md"),
            "---\ntitle: Orphan\ntype: concept\n---\n\n# Orphan\n",
        )
        .unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);
        let warnings: Vec<&str> = report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        let orphans: Vec<&&str> = warnings
            .iter()
            .filter(|item| item.starts_with("orphan wiki page"))
            .collect();
        assert!(
            orphans
                .iter()
                .any(|item| item.contains("concepts/orphan.md")),
            "orphan page must be flagged: {warnings:#?}"
        );
        assert!(
            !orphans
                .iter()
                .any(|item| item.contains("concepts/linked.md")),
            "linked page must not be flagged: {warnings:#?}"
        );

        fs::remove_dir_all(vault).unwrap();
    }
}
