use serde_json::{json, Value};
use std::{env, fs, io::Write, path::Path};

use crate::{
    args::{has_flag, opt_value, required_pos},
    context::Ctx,
    util::{
        append_log, move_path, read_text, resolve_vault_path, summary_exists, today,
        update_frontmatter_field,
    },
};

pub fn archive(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let pos = required_pos(args, 1, "archive <wiki/path> --reason REASON")?;
    let reason = opt_value(args, "--reason").ok_or("--reason is required")?;
    let src = resolve_vault_path(ctx, &pos[0])?;
    if !src.exists() {
        return Err(format!("path not found: {}", ctx.rel(&src)));
    }
    if !src.starts_with(ctx.wiki()) {
        return Err(
            "archive only supports wiki/ paths; use trash for raw/ or other vault files"
                .to_string(),
        );
    }
    if src == ctx.index() || src == ctx.log() {
        return Err(format!(
            "refusing to archive core wiki file: {}",
            ctx.rel(&src)
        ));
    }
    if src.starts_with(ctx.archive()) {
        eprintln!("already archived: {}", ctx.rel(&src));
        return Ok(0);
    }
    let rel_under = src.strip_prefix(ctx.wiki()).unwrap();
    let dest = move_path(&src, &ctx.archive().join(today()).join(rel_under))?;
    update_frontmatter_field(&dest, "status", "archived")?;
    update_frontmatter_field(&dest, "archived", &today())?;
    update_frontmatter_field(&dest, "archive_reason", &reason)?;
    append_log(
        ctx,
        "archive",
        &ctx.rel(&dest),
        &[
            format!("Archived `{}` to `{}`.", ctx.rel(&src), ctx.rel(&dest)),
            format!("Reason: {reason}."),
        ],
    )?;
    println!("{}", ctx.rel(&dest));
    Ok(0)
}

pub fn trash(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let pos = required_pos(args, 1, "trash <path> --reason REASON")?;
    let reason = opt_value(args, "--reason").ok_or("--reason is required")?;
    let src = resolve_vault_path(ctx, &pos[0])?;
    if !src.exists() {
        return Err(format!("path not found: {}", ctx.rel(&src)));
    }
    let exe = env::current_exe().unwrap_or_default();
    if [ctx.index(), ctx.log(), ctx.agents(), ctx.entrypoint()].contains(&src) || src == exe {
        return Err(format!("refusing to trash core file: {}", ctx.rel(&src)));
    }
    if src.starts_with(ctx.trash()) {
        eprintln!("already in trash: {}", ctx.rel(&src));
        return Ok(0);
    }

    let mut warnings = Vec::new();
    if src.is_file() && src.parent() == Some(&ctx.raw()) && summary_exists(ctx, &src) {
        warnings
            .push("raw source has a wiki summary; related wiki pages were not moved".to_string());
    }
    let rel_under = src.strip_prefix(&ctx.vault).unwrap();
    let dest = move_path(&src, &ctx.trash().join(today()).join(rel_under))?;
    let entry = trash_manifest_entry(ctx, "trash", &src, &dest, &reason)?;
    let mut lines = vec![
        format!(
            "Moved `{}` to `{}`.",
            entry["original_path"].as_str().unwrap(),
            entry["current_path"].as_str().unwrap()
        ),
        format!("Reason: {reason}."),
    ];
    lines.extend(warnings.clone());
    append_log(
        ctx,
        "trash",
        entry["original_path"].as_str().unwrap(),
        &lines,
    )?;
    println!("{}", ctx.rel(&dest));
    for warning in warnings {
        println!("WARN {warning}");
    }
    Ok(0)
}

pub fn trash_list(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let entries = trash_entries(ctx);
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
    } else if entries.is_empty() {
        println!("Trash is empty.");
    } else {
        for row in entries {
            println!(
                "{} | {} | original={} | reason={}",
                row["date"].as_str().unwrap_or(""),
                row["current_path"].as_str().unwrap_or(""),
                row["original_path"].as_str().unwrap_or(""),
                row["reason"].as_str().unwrap_or("")
            );
        }
    }
    Ok(0)
}

pub fn restore(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    let pos = required_pos(args, 1, "restore <trash/path> [--reason REASON]")?;
    let reason = opt_value(args, "--reason").unwrap_or_else(|| "restore requested".to_string());
    let src = resolve_vault_path(ctx, &pos[0])?;
    if !src.exists() {
        return Err(format!("path not found: {}", ctx.rel(&src)));
    }
    if !src.starts_with(ctx.trash()) {
        return Err("restore only supports files currently under trash/".to_string());
    }
    let current = ctx.rel(&src);
    let Some(entry) = trash_entries(ctx)
        .into_iter()
        .rev()
        .find(|entry| entry["current_path"].as_str() == Some(&current))
    else {
        return Err(format!("no trash manifest entry found for: {current}"));
    };
    let original = ctx.vault.join(entry["original_path"].as_str().unwrap());
    let dest = move_path(&src, &original)?;
    let restored = trash_manifest_entry(ctx, "restore", &src, &dest, &reason)?;
    append_log(
        ctx,
        "restore",
        entry["original_path"].as_str().unwrap(),
        &[
            format!(
                "Restored `{}` to `{}`.",
                restored["original_path"].as_str().unwrap(),
                restored["current_path"].as_str().unwrap()
            ),
            format!("Reason: {reason}."),
        ],
    )?;
    println!("{}", ctx.rel(&dest));
    Ok(0)
}

pub fn trash_entries(ctx: &Ctx) -> Vec<Value> {
    read_text(&ctx.trash_manifest())
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn trash_manifest_entry(
    ctx: &Ctx,
    action: &str,
    original: &Path,
    current: &Path,
    reason: &str,
) -> Result<Value, String> {
    fs::create_dir_all(ctx.trash()).map_err(|err| err.to_string())?;
    let entry = json!({
        "date": crate::util::today(),
        "action": action,
        "original_path": ctx.rel(original),
        "current_path": ctx.rel(current),
        "reason": reason,
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ctx.trash_manifest())
        .map_err(|err| err.to_string())?;
    writeln!(file, "{}", serde_json::to_string(&entry).unwrap()).map_err(|err| err.to_string())?;
    Ok(entry)
}
