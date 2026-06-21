use std::path::Path;

use crate::{
    args::{self, ConfigWriteResult},
    context::Ctx,
    health,
    util::expand_home,
};

pub fn init(args: &[String]) -> Result<i32, String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("agents-wiki init <vault-path> [--force]");
        return Ok(0);
    }
    reject_unknown_flags(args)?;

    let positional = args::positional(args);
    if positional.len() != 1 {
        return Err("usage: init <vault-path> [--force]".to_string());
    }

    let vault = expand_home(&positional[0]);
    ensure_absolute(&vault)?;
    let force = args::has_flag(args, "--force");
    args::validate_config_update(&vault, force)?;

    let ctx = Ctx::new(vault.clone());
    let repaired = health::repair_doctor(&ctx)?;
    let config = args::write_config_vault_path(&vault, force)?;

    match config {
        ConfigWriteResult::Created => println!("Created config: {}", args::config_path().display()),
        ConfigWriteResult::Updated => println!("Updated config: {}", args::config_path().display()),
        ConfigWriteResult::Kept => println!("Keeping config: {}", args::config_path().display()),
    }
    println!("Vault: {}", vault.display());
    if !repaired.is_empty() {
        println!("repaired:");
        for item in repaired {
            println!("  - {item}");
        }
    }
    Ok(0)
}

fn ensure_absolute(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err("init path must be absolute or start with ~".to_string())
    }
}

fn reject_unknown_flags(args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.starts_with("--") && arg != "--force" {
            return Err(format!("unknown option for init: {arg}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_absolute, reject_unknown_flags};
    use std::path::PathBuf;

    #[test]
    fn init_requires_absolute_path() {
        assert!(ensure_absolute(&PathBuf::from("/tmp/wiki")).is_ok());
        assert!(ensure_absolute(&PathBuf::from("relative/wiki")).is_err());
    }

    #[test]
    fn init_rejects_unknown_flags() {
        let args = vec!["/tmp/wiki".to_string(), "--typo".to_string()];

        let err = reject_unknown_flags(&args).unwrap_err();

        assert!(err.contains("--typo"));
    }

    #[test]
    fn init_allows_force_flag() {
        let args = vec!["/tmp/wiki".to_string(), "--force".to_string()];

        reject_unknown_flags(&args).expect("force is supported");
    }
}
