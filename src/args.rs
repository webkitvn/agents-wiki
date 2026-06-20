use serde::Deserialize;
use std::{env, fs, path::PathBuf};

use crate::{
    context::{CONFIG_PATH, DEFAULT_VAULT},
    util::expand_home,
};

#[derive(Debug, Deserialize)]
struct Config {
    vault_path: Option<String>,
}

pub fn parse_global_vault(args: &mut Vec<String>) -> Result<PathBuf, String> {
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--vault" {
            if index + 1 >= args.len() {
                return Err("--vault requires a path".to_string());
            }
            let vault = args.remove(index + 1);
            args.remove(index);
            return Ok(expand_home(&vault));
        } else if let Some(value) = args[index].strip_prefix("--vault=") {
            let vault = value.to_string();
            args.remove(index);
            return Ok(expand_home(&vault));
        } else {
            index += 1;
        }
    }

    if let Ok(vault) = env::var("AGENTS_WIKI_VAULT") {
        if !vault.trim().is_empty() {
            return Ok(expand_home(&vault));
        }
    }

    if let Some(vault) = config_vault_path() {
        return Ok(expand_home(&vault));
    }

    Ok(expand_home(DEFAULT_VAULT))
}

fn config_vault_path() -> Option<String> {
    let path = expand_home(CONFIG_PATH);
    let text = fs::read_to_string(path).ok()?;
    parse_config_vault_path(&text)
}

fn parse_config_vault_path(text: &str) -> Option<String> {
    let config: Config = serde_yaml::from_str(text).ok()?;
    config.vault_path.filter(|value| !value.trim().is_empty())
}

pub fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

pub fn opt_value(args: &[String], name: &str) -> Option<String> {
    for index in 0..args.len() {
        if args[index] == name {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = args[index].strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

pub fn positional(args: &[String]) -> Vec<String> {
    let value_flags = [
        "--url",
        "--note",
        "--file",
        "--title",
        "--reason",
        "--source",
        "--context",
        "--limit",
        "--vault",
    ];
    let mut out = Vec::new();
    let mut skip_next = false;
    for (index, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            if value_flags.contains(&arg.as_str()) && index + 1 < args.len() {
                skip_next = true;
            }
            continue;
        }
        out.push(arg.clone());
    }
    out
}

pub fn required_pos(args: &[String], count: usize, usage: &str) -> Result<Vec<String>, String> {
    let values = positional(args);
    if values.len() < count {
        Err(format!("usage: {usage}"))
    } else {
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_config_vault_path;

    #[test]
    fn parses_config_vault_path() {
        let vault = parse_config_vault_path("vault_path: \"/tmp/Agents Wiki\"\n");
        assert_eq!(vault.as_deref(), Some("/tmp/Agents Wiki"));
    }

    #[test]
    fn ignores_invalid_config() {
        assert_eq!(parse_config_vault_path("vault_path: ["), None);
    }

    #[test]
    fn ignores_empty_config_path() {
        assert_eq!(parse_config_vault_path("vault_path: \"\"\n"), None);
    }
}
