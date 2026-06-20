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
    args::has_flag,
    context::{Ctx, GITIGNORE_RULES},
    lifecycle::trash_entries,
    util::{append_log, frontmatter, markdown_files, read_text, source_files, summary_exists},
};

pub fn lint(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let report = lint_report(ctx);
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

pub fn lint_report(ctx: &Ctx) -> Value {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
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
            || page.starts_with(ctx.archive())
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
    let lint = lint_report(ctx);
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
            "trash_entries": trash_entries(ctx).len(),
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
    write_if_missing(ctx, &ctx.index(), index_skeleton())?;
    write_if_missing(ctx, &ctx.log(), log_skeleton())?;
    write_if_missing(ctx, &ctx.agents(), agents_skeleton())?;
    write_if_missing(ctx, &ctx.entrypoint(), entrypoint_skeleton())?;

    for dir in [ctx.archive(), ctx.trash()] {
        let keep = dir.join(".gitkeep");
        if dir.exists() && !keep.exists() {
            fs::write(&keep, "").map_err(|err| err.to_string())?;
            repaired.push(format!("Created `{}`.", ctx.rel(&keep)));
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

fn write_if_missing(_ctx: &Ctx, path: &Path, content: String) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())?;
    Ok(())
}

fn index_skeleton() -> String {
    format!("---\ntitle: Wiki Index\ncreated: {}\ntype: wiki-index\ntags: [llm-wiki, index]\n---\n\n# Wiki Index\n\n## Sources\n\n## Entities\n\n## Concepts\n\n## Questions\n\n## Reviews\n", crate::util::today())
}

fn log_skeleton() -> String {
    format!("---\ntitle: Wiki Log\ncreated: {}\ntype: wiki-log\ntags: [llm-wiki, log]\n---\n\n# Wiki Log\n", crate::util::today())
}

fn agents_skeleton() -> String {
    "# Agents Wiki LLM Wiki Contract\n\n- `raw/` contains immutable source files.\n- `wiki/` contains agent-maintained markdown.\n- Run `bin/agents-wiki doctor` before autonomous work.\n- Run `bin/agents-wiki lint` before finishing.\n".to_string()
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
