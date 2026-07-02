use serde::Serialize;

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

use crate::{
    args::has_flag,
    context::{Ctx, Taxonomy, GITIGNORE_RULES},
    manifest,
    util::{
        append_log, days_between, frontmatter, fs_err, markdown_files, read_text, source_files,
        today,
    },
};

const DEFAULT_STALE_DAYS: i64 = 90;
const GIT_COMMAND: &str = "git";
const CONTRACT_ALIAS_TARGET: &str = "AGENTS.md";
const CONTRACT_ALIASES: &[&str] = &["GEMINI.md", "CLAUDE.md"];

// ─── Typed report types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct LintReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorIssue {
    pub severity: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub repairable: bool,
}

impl DoctorIssue {
    fn error(code: &str, path: impl Into<String>) -> Self {
        Self {
            severity: "error".to_string(),
            code: code.to_string(),
            path: Some(path.into()),
            message: None,
            rule: None,
            repairable: true,
        }
    }
    fn warning(code: &str) -> Self {
        Self {
            severity: "warning".to_string(),
            code: code.to_string(),
            path: None,
            message: None,
            rule: None,
            repairable: false,
        }
    }
    fn warning_path(code: &str, path: impl Into<String>, repairable: bool) -> Self {
        Self {
            severity: "warning".to_string(),
            code: code.to_string(),
            path: Some(path.into()),
            message: None,
            rule: None,
            repairable,
        }
    }
    fn warning_rule(code: &str, rule: impl Into<String>) -> Self {
        Self {
            severity: "warning".to_string(),
            code: code.to_string(),
            path: None,
            message: None,
            rule: Some(rule.into()),
            repairable: true,
        }
    }
    fn from_lint_error(message: &str) -> Self {
        Self {
            severity: "error".to_string(),
            code: "lint_error".to_string(),
            path: None,
            message: Some(message.to_string()),
            rule: None,
            repairable: false,
        }
    }
    fn from_lint_warning(message: &str) -> Self {
        Self {
            severity: "warning".to_string(),
            code: "lint_warning".to_string(),
            path: None,
            message: Some(message.to_string()),
            rule: None,
            repairable: false,
        }
    }

    fn label(&self) -> &str {
        self.path
            .as_deref()
            .or(self.rule.as_deref())
            .or(self.message.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Serialize)]
struct DoctorState {
    pending_sources: Vec<String>,
    open_reviews: Vec<String>,
    git_available: bool,
    git_initialized: bool,
    git_dirty: Option<Vec<String>>,
    cli_executable: bool,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    vault: String,
    healthy: bool,
    issues: Vec<DoctorIssue>,
    state: DoctorState,
    #[serde(skip_serializing_if = "Option::is_none")]
    repaired: Option<Vec<String>>,
}

// ─── Public commands ──────────────────────────────────────────────────────────

pub fn lint(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--stale-days", "--json"])?;
    let stale_days = parse_i64_opt(args, "--stale-days")?.unwrap_or(DEFAULT_STALE_DAYS);
    let report = lint_report(ctx, stale_days);
    if has_flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
    } else {
        for item in &report.errors {
            println!("ERROR {item}");
        }
        for item in &report.warnings {
            println!("WARN {item}");
        }
        if report.errors.is_empty() && report.warnings.is_empty() {
            println!("ok");
        }
    }
    Ok(if report.errors.is_empty() { 0 } else { 1 })
}

pub fn doctor(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &["--repair", "--json"])?;
    let mut repaired: Option<Vec<String>> = None;
    if has_flag(args, "--repair") {
        let items = repair_doctor(ctx)?;
        if !items.is_empty() {
            repaired = Some(items);
        }
    }
    let mut report = build_doctor_report(ctx);
    report.repaired = repaired;
    if has_flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
    } else {
        println!("vault: {}", report.vault);
        println!("healthy: {}", if report.healthy { "yes" } else { "no" });
        println!(
            "git_initialized: {}",
            if report.state.git_initialized {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "git_dirty: {}",
            report
                .state
                .git_dirty
                .as_ref()
                .map(|items| items.len())
                .unwrap_or(0)
        );
        println!("pending_sources: {}", report.state.pending_sources.len());
        println!("open_reviews: {}", report.state.open_reviews.len());
        if let Some(items) = &report.repaired {
            println!("repaired:");
            for item in items {
                println!("  - {item}");
            }
        }
        if report.issues.is_empty() {
            println!("issues: none");
        } else {
            println!("issues:");
            for issue in &report.issues {
                println!("  - {} {}: {}", issue.severity, issue.code, issue.label());
            }
        }
    }
    Ok(
        if report.issues.iter().any(|issue| issue.severity == "error") {
            1
        } else {
            0
        },
    )
}

pub fn reset(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("agents-wiki reset");
        println!("WARNING: deletes all contents of the resolved vault after N/y confirmation.");
        return Ok(0);
    }
    if !args.is_empty() {
        return Err("usage: reset".to_string());
    }
    if !ctx.vault.exists() {
        println!("Vault does not exist: {}", ctx.vault.display());
        return Ok(0);
    }
    validate_reset_target(ctx)?;
    if is_dir_empty(&ctx.vault)? {
        println!("Vault is already empty: {}", ctx.vault.display());
        return Ok(0);
    }
    print!(
        "WARNING: delete all contents of {}? [N/y] ",
        ctx.vault.display()
    );
    io::stdout().flush().map_err(|err| err.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| err.to_string())?;
    if !answer_confirms_reset(&answer) {
        println!("Aborted.");
        return Ok(1);
    }
    let deleted = reset_vault_contents(ctx)?;
    println!(
        "Deleted {deleted} item(s) from {}. Run `agents-wiki init <vault-path>` to scaffold a fresh vault.",
        ctx.vault.display()
    );
    Ok(0)
}

// ─── Report builders ──────────────────────────────────────────────────────────

pub fn lint_report(ctx: &Ctx, stale_days: i64) -> LintReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let today_str = today();
    if let Some(err) = manifest::load_for_lint(ctx) {
        warnings.push(format!("manifest_unreadable: {err}"));
    }
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
        // Use structural wikilink check instead of substring to avoid false positives
        // on short stems (e.g. "ai", "ml").
        let rel = ctx.rel(&page);
        let link_no_ext = rel.strip_suffix(".md").unwrap_or(&rel);
        if !index_text.contains(&format!("[[{link_no_ext}]]"))
            && !index_text.contains(&format!("[[{link_no_ext}|"))
        {
            warnings.push(format!("wiki page missing from index: {rel}"));
        }
        let fields = frontmatter(&page);
        if !fields.is_empty() {
            match fields.get("summary") {
                Some(summary) if frontmatter_value(summary).chars().count() > 200 => {
                    warnings.push(format!("overlong_summary: {}", ctx.rel(&page)));
                }
                Some(_) => {}
                None => warnings.push(format!("missing_summary: {}", ctx.rel(&page))),
            }
            for key in [
                "provenance_has_inferred_content",
                "provenance_has_ambiguous_content",
            ] {
                if let Some(value) = fields.get(key) {
                    let value = frontmatter_value(value);
                    if value != "true" && value != "false" {
                        warnings.push(format!(
                            "malformed provenance value `{key}`: {}",
                            ctx.rel(&page)
                        ));
                    }
                }
            }
            if provenance_bool(&fields, "provenance_has_ambiguous_content")
                && fields.get("type").is_none_or(|value| value != "review")
                && !text.contains("[[wiki/reviews/")
            {
                warnings.push(format!(
                    "ambiguous provenance without review link: {}",
                    ctx.rel(&page)
                ));
            }
        }
        if fields.get("status").is_some_and(|value| value == "active")
            && !page_has_evidence(&fields, &text)
        {
            errors.push(format!(
                "active wiki page lacks evidence: {}",
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
        let source_folder = ctx.taxonomy.folder_for("source");
        if page.parent().and_then(|parent| parent.file_name()) == Some(OsStr::new(source_folder))
            && fields.get("status").is_some_and(|value| value == "draft")
        {
            warnings.push(format!("draft source summary: {}", ctx.rel(&page)));
        }
    }

    let summaries_index = crate::util::SummaryIndex::build(ctx);
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
        if !summaries_index.contains_source(ctx, &raw) {
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
        let rel = ctx.rel(page);
        let link_no_ext = rel.strip_suffix(".md").unwrap_or(&rel);
        // Use structural wikilink check for orphan detection.
        let inbound = page_texts.iter().any(|(other, text)| {
            other != page
                && (text.contains(&format!("[[{link_no_ext}]]"))
                    || text.contains(&format!("[[{link_no_ext}|")))
        });
        if !inbound {
            warnings.push(format!("orphan wiki page (no inbound links): {rel}"));
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

    // Check that each taxonomy section exists in index.md.
    for kind in ctx.taxonomy.kinds() {
        let heading = format!("## {}", kind.section);
        if !index_text.contains(&heading) {
            warnings.push(format!(
                "taxonomy section missing from index.md: {}",
                kind.section
            ));
        }
    }

    LintReport { errors, warnings }
}

fn frontmatter_value(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn provenance_source_ids_has_entries(fields: &BTreeMap<String, String>) -> bool {
    fields
        .get("provenance_source_ids")
        .map(|value| {
            let value = value.trim().trim_matches(['[', ']']);
            value
                .split(',')
                .map(|item| item.trim().trim_matches('"'))
                .any(|item| !item.is_empty())
        })
        .unwrap_or(false)
}

fn provenance_bool(fields: &BTreeMap<String, String>, key: &str) -> bool {
    fields
        .get(key)
        .map(|value| frontmatter_value(value) == "true")
        .unwrap_or(false)
}

fn page_has_evidence(fields: &BTreeMap<String, String>, text: &str) -> bool {
    text.contains("raw/")
        || fields
            .get("source_id")
            .is_some_and(|value| !value.trim().is_empty())
        || fields
            .get("source_path")
            .is_some_and(|value| !value.trim().is_empty())
        || provenance_source_ids_has_entries(fields)
}

fn build_doctor_report(ctx: &Ctx) -> DoctorReport {
    let mut issues = Vec::new();
    for dir in ctx.required_dirs() {
        if !dir.exists() {
            issues.push(DoctorIssue::error("missing_dir", ctx.rel(&dir)));
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
            issues.push(DoctorIssue::error("missing_file", ctx.rel(&file)));
        }
    }
    for alias in CONTRACT_ALIASES {
        if let Some(issue) = contract_alias_issue(ctx, alias) {
            issues.push(issue);
        }
    }
    if !ctx.manifest().exists() {
        issues.push(DoctorIssue::warning_path(
            "missing_manifest",
            ".manifest.json",
            true,
        ));
    }

    let cli_executable = is_cli_executable();
    if !cli_executable {
        let exe_path = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "agents-wiki".to_string());
        issues.push(DoctorIssue::error("cli_not_executable", exe_path));
    }

    let git_available = git_available();
    let git_initialized = git_available && git_repo_exists(ctx);
    if !git_available {
        issues.push(DoctorIssue::warning("git_unavailable"));
    } else if !git_initialized {
        issues.push(DoctorIssue::warning_path("git_not_initialized", ".", true));
    }
    for rule in missing_gitignore_rules(ctx) {
        issues.push(DoctorIssue::warning_rule("missing_gitignore_rule", rule));
    }

    let lint = lint_report(ctx, DEFAULT_STALE_DAYS);
    for msg in &lint.errors {
        issues.push(DoctorIssue::from_lint_error(msg));
    }
    for msg in &lint.warnings {
        issues.push(DoctorIssue::from_lint_warning(msg));
    }

    let healthy = !issues.iter().any(|issue| issue.severity == "error");
    DoctorReport {
        vault: ctx.vault.display().to_string(),
        healthy,
        issues,
        state: DoctorState {
            pending_sources: pending_source_items(ctx),
            open_reviews: open_review_items(ctx),
            git_available,
            git_initialized,
            git_dirty: git_dirty_status(ctx),
            cli_executable,
        },
        repaired: None,
    }
}

pub fn repair_doctor(ctx: &Ctx) -> Result<Vec<String>, String> {
    let mut repaired = Vec::new();
    for dir in ctx.required_dirs() {
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(|err| fs_err(&dir, "create directory", err))?;
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
    for alias in CONTRACT_ALIASES {
        if let Some(item) = ensure_contract_alias(ctx, alias)? {
            repaired.push(item);
        }
    }
    if manifest::ensure_exists(ctx)? {
        repaired.push(format!("Created `{}`.", ctx.rel(&ctx.manifest())));
    }

    // Repair missing taxonomy sections in an existing index.md.
    let index_sections_repaired = repair_index_sections(ctx)?;
    repaired.extend(index_sections_repaired);

    let missing = missing_gitignore_rules(ctx);
    if !missing.is_empty() {
        let mut existing = read_text(&ctx.gitignore());
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&missing.join("\n"));
        existing.push('\n');
        let gitignore = ctx.gitignore();
        fs::write(&gitignore, existing).map_err(|err| fs_err(&gitignore, "write", err))?;
        repaired.push(format!("Updated `{}`.", ctx.rel(&ctx.gitignore())));
    }
    let repaired_before_git = !repaired.is_empty();
    if let Some(item) = repair_git(ctx, GIT_COMMAND, repaired_before_git)? {
        repaired.push(item);
    }
    if !repaired.is_empty() {
        append_log(ctx, "doctor", "repair", &repaired)?;
    }
    Ok(repaired)
}

/// Idempotently add any missing `## {section}` headings to an existing
/// `wiki/index.md` that doesn't have them yet. Returns the list of repair
/// messages for each section that was added.
fn repair_index_sections(ctx: &Ctx) -> Result<Vec<String>, String> {
    let index = ctx.index();
    if !index.exists() {
        // Nothing to repair; the full skeleton will be written by `write_if_missing`.
        return Ok(Vec::new());
    }
    let mut text = read_text(&index);
    let mut added = Vec::new();
    for kind in ctx.taxonomy.kinds() {
        let heading = format!("## {}", kind.section);
        if !text.contains(&heading) {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("\n{heading}\n"));
            added.push(format!(
                "Added `{}` section to `{}`.",
                kind.section,
                ctx.rel(&index)
            ));
        }
    }
    if !added.is_empty() {
        fs::write(&index, &text).map_err(|err| fs_err(&index, "write", err))?;
    }
    Ok(added)
}

// ─── Reset helpers ────────────────────────────────────────────────────────────

fn validate_reset_target(ctx: &Ctx) -> Result<(), String> {
    if !ctx.vault.is_dir() {
        return Err(format!("vault is not a directory: {}", ctx.vault.display()));
    }
    let canonical = fs::canonicalize(&ctx.vault).map_err(|err| err.to_string())?;
    if reset_path_is_protected(&canonical) {
        return Err(format!(
            "refusing to reset protected path: {}",
            canonical.display()
        ));
    }
    if !looks_like_agents_wiki_vault(ctx) && !is_dir_empty(&ctx.vault)? {
        return Err(format!(
            "refusing to reset non-agents-wiki directory: {}",
            ctx.vault.display()
        ));
    }
    Ok(())
}

fn reset_path_is_protected(path: &Path) -> bool {
    if path.parent().is_none() || path.components().count() < 3 {
        return true;
    }
    std::env::var("HOME").is_ok_and(|home| path == Path::new(&home))
}

fn looks_like_agents_wiki_vault(ctx: &Ctx) -> bool {
    frontmatter(&ctx.agents())
        .get("type")
        .is_some_and(|value| value == "wiki-schema")
        || frontmatter(&ctx.entrypoint())
            .get("type")
            .is_some_and(|value| value == "vault-entrypoint")
        || (frontmatter(&ctx.index())
            .get("type")
            .is_some_and(|value| value == "wiki-index")
            && frontmatter(&ctx.log())
                .get("type")
                .is_some_and(|value| value == "wiki-log"))
}

fn is_dir_empty(path: &Path) -> Result<bool, String> {
    Ok(fs::read_dir(path)
        .map_err(|err| err.to_string())?
        .next()
        .is_none())
}

fn answer_confirms_reset(answer: &str) -> bool {
    matches!(answer.trim(), "y" | "Y")
}

fn reset_vault_contents(ctx: &Ctx) -> Result<usize, String> {
    let mut deleted = 0;
    for entry in fs::read_dir(&ctx.vault).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let file_type = fs::symlink_metadata(&path)
            .map_err(|err| format!("{}: {err}", path.display()))?
            .file_type();
        if file_type.is_dir() {
            fs::remove_dir_all(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        } else {
            fs::remove_file(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        }
        deleted += 1;
    }
    Ok(deleted)
}

// ─── Scaffold helpers ─────────────────────────────────────────────────────────

fn write_if_missing(path: &Path, content: String) -> Result<bool, String> {
    if path.exists() {
        return Ok(false);
    }
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
    fs::write(path, content).map_err(|err| fs_err(path, "write", err))?;
    Ok(true)
}

fn contract_alias_issue(ctx: &Ctx, alias: &str) -> Option<DoctorIssue> {
    let path = ctx.vault.join(alias);
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return Some(DoctorIssue::warning_path(
            "missing_contract_alias",
            alias,
            true,
        ));
    };
    if meta.file_type().is_symlink() {
        return match fs::read_link(&path) {
            Ok(target) if target == Path::new(CONTRACT_ALIAS_TARGET) => None,
            _ => Some(DoctorIssue::warning_path(
                "contract_alias_conflict",
                alias,
                false,
            )),
        };
    }
    if meta.is_file() && read_text(&path).trim() == contract_alias_pointer().trim() {
        return None;
    }
    Some(DoctorIssue::warning_path(
        "contract_alias_conflict",
        alias,
        false,
    ))
}

fn ensure_contract_alias(ctx: &Ctx, alias: &str) -> Result<Option<String>, String> {
    let path = ctx.vault.join(alias);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::read_link(&path).map_err(|err| fs_err(&path, "read link", err))?;
            if target == Path::new(CONTRACT_ALIAS_TARGET) {
                Ok(None)
            } else {
                Ok(Some(format!(
                    "Skipped `{alias}`: existing symlink does not point to `{CONTRACT_ALIAS_TARGET}`."
                )))
            }
        }
        Ok(meta)
            if meta.is_file() && read_text(&path).trim() == contract_alias_pointer().trim() =>
        {
            Ok(None)
        }
        Ok(_) => Ok(Some(format!(
            "Skipped `{alias}`: existing path is not an agents-wiki contract alias."
        ))),
        Err(err) if err.kind() == io::ErrorKind::NotFound => create_contract_alias(&path, alias),
        Err(err) => Err(fs_err(&path, "inspect", err)),
    }
}

fn create_contract_alias(path: &Path, alias: &str) -> Result<Option<String>, String> {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).map_err(|err| fs_err(parent, "create directory", err))?;
    match symlink_contract_alias(path) {
        Ok(()) => Ok(Some(format!("Created `{alias}` alias."))),
        Err(_) => {
            fs::write(path, contract_alias_pointer()).map_err(|err| fs_err(path, "write", err))?;
            Ok(Some(format!(
                "Created `{alias}` pointer file because symlink was unavailable."
            )))
        }
    }
}

#[cfg(unix)]
fn symlink_contract_alias(path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(CONTRACT_ALIAS_TARGET, path)
}

#[cfg(windows)]
fn symlink_contract_alias(path: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(CONTRACT_ALIAS_TARGET, path)
}

#[cfg(not(any(unix, windows)))]
fn symlink_contract_alias(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink unsupported on this platform",
    ))
}

fn contract_alias_pointer() -> String {
    format!(
        "# Agents Wiki Contract Alias\n\nUse `{CONTRACT_ALIAS_TARGET}` as the source of truth for this vault.\n"
    )
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
- `GEMINI.md` and `CLAUDE.md` — compatibility aliases for tools that read those
  filenames. `AGENTS.md` remains the source of truth.

## Language policy

- MUST write the entire `wiki/` layer in English: pages, summaries, titles, index
  entries, log entries, questions, reviews, entity pages, concept pages, and notes.
- Keep `raw/` sources in their original language. NEVER translate, rewrite, or
  normalize source files under `raw/`.
- When a source is not English, translate its meaning into clear English synthesis.
  Do not produce literal sentence-by-sentence translation unless the task asks for it.
- Preserve original-language proper nouns, product names, technical terms, and short
  quotations only when needed for accuracy.
- If translation nuance matters, cite the `raw/` source and include the original term
  in parentheses.

## Page conventions

- Every page starts with YAML frontmatter (`title`, `summary`, `created`, `type`,
  `status`, provenance fields, `tags`) and a single `# H1`.
- Keep `summary` to one or two short sentences so agents can preview pages cheaply.
- `status: draft` until reviewed; `status: active` once it cites at least one `raw/` source.
- Active pages must cite their evidence: link the `raw/` path, include its `source_id:`,
  or list IDs in `provenance_source_ids`.
- Mark ambiguous synthesized content with `provenance_has_ambiguous_content: true`
  and link the relevant review page.
- Cross-link liberally with `[[wikilinks]]`; an orphan page (no inbound links) is flagged by lint.

## Choosing a kind

- `entity` — a concrete thing: person, org, product, place.
- `concept` — an idea, theme, or topic that spans sources.
- `question` — an open question or a durable answer worth keeping.
- `review` — a flag for a contradiction or claim that needs a human/agent decision.

## Operations

- Ingest: `agents-wiki new-source` then `agents-wiki source-summary`. The CLI files the
  summary into `index.md` and `log.md`. Then YOU do the English synthesis — a single source
  often touches 10-15 pages: update related entity/concept pages, add cross-links, and open
  a `review` when a new source contradicts an existing claim.
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

// ─── Git helpers ──────────────────────────────────────────────────────────────

fn git_repo_exists(ctx: &Ctx) -> bool {
    git_repo_exists_with_command(ctx, GIT_COMMAND)
}

fn git_repo_exists_with_command(ctx: &Ctx, command: &str) -> bool {
    git_output(ctx, command, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
    })
}

fn git_available() -> bool {
    command_available(GIT_COMMAND)
}

fn command_available(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn git_dirty_status(ctx: &Ctx) -> Option<Vec<String>> {
    git_dirty_status_with_command(ctx, GIT_COMMAND)
}

fn git_dirty_status_with_command(ctx: &Ctx, command: &str) -> Option<Vec<String>> {
    if !git_repo_exists_with_command(ctx, command) {
        return None;
    }
    git_output(ctx, command, &["status", "--short"]).map(|output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.to_string())
            .collect()
    })
}

fn repair_git(ctx: &Ctx, command: &str, report_missing: bool) -> Result<Option<String>, String> {
    if git_repo_exists_with_command(ctx, command) {
        return Ok(None);
    }
    if !command_available(command) {
        if !report_missing {
            return Ok(None);
        }
        return Ok(Some(
            "Skipped git init because git is not installed; install git to enable versioning, deletion, and restore."
                .to_string(),
        ));
    }
    if Command::new(command)
        .arg("init")
        .current_dir(&ctx.vault)
        .status()
        .map_err(|err| err.to_string())?
        .success()
    {
        return Ok(Some("Initialized git repository.".to_string()));
    }
    Ok(None)
}

fn git_output(ctx: &Ctx, command: &str, args: &[&str]) -> Option<std::process::Output> {
    Command::new(command)
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

/// Returns open review items using the taxonomy-resolved reviews folder.
fn open_review_items(ctx: &Ctx) -> Vec<String> {
    let reviews_folder = ctx.taxonomy.folder_for("review");
    markdown_files(&ctx.wiki().join(reviews_folder))
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
    let summaries_index = crate::util::SummaryIndex::build(ctx);
    source_files(ctx)
        .into_iter()
        .filter(|path| !summaries_index.contains_source(ctx, path))
        .map(|path| ctx.rel(&path))
        .collect()
}

/// Check if the current CLI binary is executable. Portable across Unix/Windows.
fn is_cli_executable() -> bool {
    let exe = std::env::current_exe();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        exe.ok()
            .and_then(|path| path.metadata().ok())
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        // On Windows a file is executable if it exists and has a .exe extension.
        exe.ok()
            .map(|path| {
                path.exists()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("exe"))
                        .unwrap_or(true) // assume executable if no extension info
            })
            .unwrap_or(false)
    }
}

// ─── CLI argument validation helpers ─────────────────────────────────────────

/// Reject any `--foo` argument that is not in `allowed`. Value flags (e.g.
/// `--stale-days 90`) are handled by checking the flag name only.
pub fn validate_flags(args: &[String], allowed: &[&str]) -> Result<(), String> {
    for arg in args {
        if let Some(flag) = arg.strip_prefix("--") {
            // Strip `=value` suffix for `--flag=value` style args.
            let flag_name = format!("--{}", flag.split('=').next().unwrap_or(flag));
            if !allowed.contains(&flag_name.as_str()) {
                return Err(format!("unknown option: {flag_name}"));
            }
        }
    }
    Ok(())
}

/// Parse an optional `i64` flag value. Returns `Err` if the flag is present
/// but its value is not a valid integer (instead of silently falling back).
pub fn parse_i64_opt(args: &[String], flag: &str) -> Result<Option<i64>, String> {
    match crate::args::opt_value(args, flag) {
        None => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{flag} must be an integer, got: {value:?}")),
    }
}

/// Parse an optional `usize` flag value. Returns `Err` if the flag is present
/// but its value is not a valid non-negative integer.
pub fn parse_usize_opt(args: &[String], flag: &str) -> Result<Option<usize>, String> {
    match crate::args::opt_value(args, flag) {
        None => Ok(None),
        Some(value) => value
            .parse::<usize>()
            .map(Some)
            .map_err(|_| format!("{flag} must be a non-negative integer, got: {value:?}")),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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
        for alias in CONTRACT_ALIASES {
            assert_contract_alias(&ctx, alias);
            assert!(
                repaired
                    .iter()
                    .any(|item| item.starts_with(&format!("Created `{alias}` "))),
                "missing repair entry for {alias}\nactual: {repaired:#?}"
            );
        }

        let agents = read_text(&ctx.agents());
        assert!(agents.contains("## Language policy"));
        assert!(agents.contains("MUST write the entire `wiki/` layer in English"));
        assert!(agents.contains("Keep `raw/` sources in their original language"));

        let repaired_again = repair_doctor(&ctx).unwrap();
        assert!(!repaired_again
            .iter()
            .any(|item| item == "Created `wiki/index.md`."));
        for alias in CONTRACT_ALIASES {
            assert!(
                !repaired_again
                    .iter()
                    .any(|item| item.starts_with(&format!("Created `{alias}` "))),
                "alias must be idempotent: {alias}\nactual: {repaired_again:#?}"
            );
        }

        fs::remove_dir_all(vault).unwrap();
    }

    fn assert_contract_alias(ctx: &Ctx, alias: &str) {
        let path = ctx.vault.join(alias);
        let meta = fs::symlink_metadata(&path).expect("alias exists");
        if meta.file_type().is_symlink() {
            assert_eq!(fs::read_link(&path).unwrap(), PathBuf::from("AGENTS.md"));
        } else {
            assert_eq!(read_text(&path).trim(), contract_alias_pointer().trim());
        }
    }

    #[test]
    fn repair_doctor_does_not_overwrite_existing_contract_alias_file() {
        let vault = temp_vault("custom-contract-alias");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("GEMINI.md"), "# Custom Gemini Instructions\n").unwrap();
        let ctx = Ctx::new(vault.clone());

        let repaired = repair_doctor(&ctx).unwrap();

        assert_eq!(
            read_text(&vault.join("GEMINI.md")),
            "# Custom Gemini Instructions\n"
        );
        assert!(
            repaired.iter().any(|item| item
                == "Skipped `GEMINI.md`: existing path is not an agents-wiki contract alias."),
            "expected custom alias skip\nactual: {repaired:#?}"
        );
        assert_contract_alias(&ctx, "CLAUDE.md");

        let report = build_doctor_report(&ctx);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "contract_alias_conflict" && issue.path.as_deref() == Some("GEMINI.md")
        }));

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn missing_git_skips_init_without_failing_repair() {
        let vault = temp_vault("missing-git-repair");
        fs::create_dir_all(&vault).unwrap();
        let ctx = Ctx::new(vault.clone());

        let repair = repair_git(&ctx, "agents-wiki-missing-git-command", true).unwrap();

        assert_eq!(
            repair.as_deref(),
            Some(
                "Skipped git init because git is not installed; install git to enable versioning, deletion, and restore."
            )
        );
        assert_eq!(
            git_dirty_status_with_command(&ctx, "agents-wiki-missing-git-command"),
            None
        );
        assert_eq!(
            repair_git(&ctx, "agents-wiki-missing-git-command", false).unwrap(),
            None
        );

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
    fn repair_index_sections_adds_missing_headings() {
        let vault = temp_vault("repair-index-sections");
        fs::create_dir_all(&vault).unwrap();
        // Pre-existing index with only one section.
        let ctx = Ctx::new(vault.clone());
        fs::create_dir_all(ctx.wiki()).unwrap();
        fs::write(
            ctx.index(),
            "---\ntype: wiki-index\n---\n\n# Wiki Index\n\n## Sources\n",
        )
        .unwrap();

        // Run repair on a vault that has a custom taxonomy with two kinds.
        fs::write(
            ctx.agents(),
            "---\ntaxonomy:\n  - kind: source\n    folder: sources\n    section: Sources\n  - kind: concept\n    folder: concepts\n    section: Concepts\n---\n\n# Schema\n",
        )
        .unwrap();
        let ctx = Ctx::new(vault.clone()); // reload taxonomy
        let repaired = repair_index_sections(&ctx).unwrap();

        assert!(
            repaired.iter().any(|msg| msg.contains("Concepts")),
            "missing section should be repaired: {repaired:#?}"
        );
        let index = read_text(&ctx.index());
        assert!(
            index.contains("## Concepts"),
            "Concepts must be added: {index}"
        );
        assert!(
            index.contains("## Sources"),
            "existing Sources must be preserved: {index}"
        );

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
        let warnings = &report.warnings;
        let off_strs: Vec<&str> = warnings
            .iter()
            .filter(|item| item.starts_with("off-taxonomy wiki page"))
            .map(|s| s.as_str())
            .collect();
        assert!(off_strs.iter().any(|item| item.contains("notes/stray.md")));
        assert!(off_strs.iter().any(|item| item.contains("wiki/loose.md")));
        assert!(
            !off_strs.iter().any(|item| item.contains("concepts/ok.md")),
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
            "---\ntitle: Hub\ntype: concept\n---\n\n# Hub\n\nSee [[wiki/concepts/linked]].\n",
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
        let warnings = &report.warnings;
        let orphans: Vec<&str> = warnings
            .iter()
            .filter(|item| item.starts_with("orphan wiki page"))
            .map(|s| s.as_str())
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

    #[test]
    fn lint_warns_for_missing_and_overlong_summary() {
        let vault = temp_vault("summary-lint");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("missing.md"),
            "---\ntitle: Missing\ncreated: 2026-01-01\ntype: concept\nstatus: draft\ntags: [llm-wiki]\n---\n\n# Missing\n",
        )
        .unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("long.md"),
            format!(
                "---\ntitle: Long\nsummary: \"{}\"\ncreated: 2026-01-01\ntype: concept\nstatus: draft\ntags: [llm-wiki]\n---\n\n# Long\n",
                "x".repeat(201)
            ),
        )
        .unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);

        assert!(report
            .warnings
            .iter()
            .any(|item| item == "missing_summary: wiki/concepts/missing.md"));
        assert!(report
            .warnings
            .iter()
            .any(|item| item == "overlong_summary: wiki/concepts/long.md"));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn lint_errors_for_active_page_without_evidence() {
        let vault = temp_vault("active-evidence");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("active.md"),
            "---\ntitle: Active\nsummary: \"ok\"\ncreated: 2026-01-01\ntype: concept\nstatus: active\nprovenance_source_ids: []\ntags: [llm-wiki]\n---\n\n# Active\n",
        )
        .unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);

        assert!(report
            .errors
            .iter()
            .any(|item| item == "active wiki page lacks evidence: wiki/concepts/active.md"));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn lint_warns_for_malformed_and_unreviewed_ambiguous_provenance() {
        let vault = temp_vault("provenance-lint");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::write(
            ctx.wiki().join("concepts").join("ambiguous.md"),
            "---\ntitle: Ambiguous\nsummary: \"ok\"\ncreated: 2026-01-01\ntype: concept\nstatus: draft\nprovenance_source_ids: []\nprovenance_has_inferred_content: maybe\nprovenance_has_ambiguous_content: true\ntags: [llm-wiki]\n---\n\n# Ambiguous\n",
        )
        .unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);

        assert!(report.warnings.iter().any(|item| {
            item == "malformed provenance value `provenance_has_inferred_content`: wiki/concepts/ambiguous.md"
        }));
        assert!(report.warnings.iter().any(|item| {
            item == "ambiguous provenance without review link: wiki/concepts/ambiguous.md"
        }));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn lint_warns_for_corrupt_manifest() {
        let vault = temp_vault("manifest-lint");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::write(ctx.manifest(), "{").unwrap();

        let report = lint_report(&ctx, DEFAULT_STALE_DAYS);

        assert!(report
            .warnings
            .iter()
            .any(|item| item.starts_with("manifest_unreadable: failed to parse .manifest.json")));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn repair_doctor_creates_manifest() {
        let vault = temp_vault("repair-manifest");
        let ctx = Ctx::new(vault.clone());

        let repaired = repair_doctor(&ctx).unwrap();

        assert!(ctx.manifest().exists());
        assert!(repaired
            .iter()
            .any(|item| item == "Created `.manifest.json`."));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn reset_confirmation_only_accepts_y() {
        assert!(answer_confirms_reset("y\n"));
        assert!(answer_confirms_reset("Y"));
        assert!(!answer_confirms_reset(""));
        assert!(!answer_confirms_reset("yes"));
        assert!(!answer_confirms_reset("n"));
    }

    #[test]
    fn reset_refuses_non_agents_wiki_directory_with_contents() {
        let vault = temp_vault("reset-refuse");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("notes.md"), "# Not a vault\n").unwrap();
        let ctx = Ctx::new(vault.clone());

        let err = validate_reset_target(&ctx).unwrap_err();

        assert!(err.contains("non-agents-wiki"));
        assert!(vault.join("notes.md").exists());
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn reset_refuses_directory_with_unrelated_agents_file() {
        let vault = temp_vault("reset-agents-file");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("AGENTS.md"), "# Repo instructions\n").unwrap();
        let ctx = Ctx::new(vault.clone());

        let err = validate_reset_target(&ctx).unwrap_err();

        assert!(err.contains("non-agents-wiki"));
        assert!(vault.join("AGENTS.md").exists());
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn reset_deletes_all_vault_children_but_keeps_root() {
        let vault = temp_vault("reset-delete");
        let ctx = Ctx::new(vault.clone());
        repair_doctor(&ctx).unwrap();
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::write(vault.join(".obsidian").join("workspace.json"), "{}").unwrap();

        validate_reset_target(&ctx).unwrap();
        let deleted = reset_vault_contents(&ctx).unwrap();

        assert!(deleted > 0);
        assert!(vault.is_dir());
        assert!(is_dir_empty(&vault).unwrap());
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn validate_flags_rejects_unknown_options() {
        let args = vec!["--json".to_string(), "--typo".to_string()];
        assert!(validate_flags(&args, &["--json"]).is_err());
        assert!(validate_flags(&args, &["--json", "--typo"]).is_ok());
    }

    #[test]
    fn parse_i64_opt_returns_error_on_invalid_value() {
        let args = vec!["--stale-days".to_string(), "ninety".to_string()];
        assert!(parse_i64_opt(&args, "--stale-days").is_err());
        let args2 = vec!["--stale-days".to_string(), "90".to_string()];
        assert_eq!(parse_i64_opt(&args2, "--stale-days").unwrap(), Some(90));
    }

    #[test]
    fn parse_usize_opt_returns_error_on_negative() {
        let args = vec!["--limit".to_string(), "abc".to_string()];
        assert!(parse_usize_opt(&args, "--limit").is_err());
    }
}
