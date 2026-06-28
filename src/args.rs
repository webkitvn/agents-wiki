use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    context::{CONFIG_PATH, DEFAULT_VAULT},
    util::expand_home,
};

#[derive(Debug, Deserialize)]
struct Config {
    vault_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConfigFile {
    vault_path: String,
}

#[derive(Debug, PartialEq)]
pub enum ConfigWriteResult {
    Created,
    Updated,
    Kept,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VaultResolutionSource {
    Cli,
    Env,
    Config,
    Default,
}

impl VaultResolutionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::Config => "config",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ResolvedVault {
    pub path: PathBuf,
    pub source: VaultResolutionSource,
}

pub fn parse_global_vault(args: &mut Vec<String>) -> Result<PathBuf, String> {
    Ok(resolve_global_vault(args)?.path)
}

pub fn resolve_global_vault(args: &mut Vec<String>) -> Result<ResolvedVault, String> {
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--vault" {
            if index + 1 >= args.len() {
                return Err("--vault requires a path".to_string());
            }
            let vault = args.remove(index + 1);
            args.remove(index);
            return Ok(ResolvedVault {
                path: expand_home(&vault),
                source: VaultResolutionSource::Cli,
            });
        } else if let Some(value) = args[index].strip_prefix("--vault=") {
            let vault = value.to_string();
            args.remove(index);
            return Ok(ResolvedVault {
                path: expand_home(&vault),
                source: VaultResolutionSource::Cli,
            });
        } else {
            index += 1;
        }
    }

    if let Ok(vault) = env::var("AGENTS_WIKI_VAULT") {
        if !vault.trim().is_empty() {
            return Ok(ResolvedVault {
                path: expand_home(&vault),
                source: VaultResolutionSource::Env,
            });
        }
    }

    if let Some(vault) = config_vault_path() {
        return Ok(ResolvedVault {
            path: expand_home(&vault),
            source: VaultResolutionSource::Config,
        });
    }

    Ok(ResolvedVault {
        path: expand_home(DEFAULT_VAULT),
        source: VaultResolutionSource::Default,
    })
}

fn config_vault_path() -> Option<String> {
    let path = config_path();
    let text = fs::read_to_string(path).ok()?;
    parse_config_vault_path(&text)
}

fn parse_config_vault_path(text: &str) -> Option<String> {
    let config: Config = serde_yaml::from_str(text).ok()?;
    config.vault_path.filter(|value| !value.trim().is_empty())
}

pub fn config_path() -> PathBuf {
    expand_home(CONFIG_PATH)
}

pub fn validate_config_update(vault: &Path, force: bool) -> Result<(), String> {
    validate_config_update_at(&config_path(), vault, force)
}

pub fn write_config_vault_path(vault: &Path, force: bool) -> Result<ConfigWriteResult, String> {
    write_config_vault_path_at(&config_path(), vault, force)
}

fn validate_config_update_at(config_file: &Path, vault: &Path, force: bool) -> Result<(), String> {
    if force || !config_file.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(config_file).map_err(|err| err.to_string())?;
    let Some(existing) = parse_config_vault_path(&text) else {
        return Err(format!(
            "Existing {} has no usable vault_path. Run with --force to overwrite it.",
            config_file.display()
        ));
    };
    if expand_home(&existing) == vault {
        return Ok(());
    }
    Err(format!(
        "Existing {} uses vault_path: \"{}\". Run with --force to overwrite it.",
        config_file.display(),
        existing
    ))
}

fn write_config_vault_path_at(
    config_file: &Path,
    vault: &Path,
    force: bool,
) -> Result<ConfigWriteResult, String> {
    validate_config_update_at(config_file, vault, force)?;
    if config_file.exists() && !force {
        return Ok(ConfigWriteResult::Kept);
    }
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let existed = config_file.exists();
    let config = ConfigFile {
        vault_path: vault.to_string_lossy().to_string(),
    };
    let text = serde_yaml::to_string(&config).map_err(|err| err.to_string())?;
    fs::write(config_file, text).map_err(|err| err.to_string())?;
    Ok(if existed {
        ConfigWriteResult::Updated
    } else {
        ConfigWriteResult::Created
    })
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
        "--stale-days",
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
    use super::{
        parse_config_vault_path, resolve_global_vault, validate_config_update_at,
        write_config_vault_path_at, ConfigWriteResult, VaultResolutionSource,
    };
    use std::{
        env, fs,
        path::PathBuf,
        sync::{Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("agents-wiki-{name}-{nonce}"))
    }

    #[test]
    fn parses_config_vault_path() {
        let vault = parse_config_vault_path("vault_path: \"/tmp/agents-wiki\"\n");
        assert_eq!(vault.as_deref(), Some("/tmp/agents-wiki"));
    }

    #[test]
    fn ignores_invalid_config() {
        assert_eq!(parse_config_vault_path("vault_path: ["), None);
    }

    #[test]
    fn ignores_empty_config_path() {
        assert_eq!(parse_config_vault_path("vault_path: \"\"\n"), None);
    }

    #[test]
    fn resolves_cli_vault_flag() {
        let _guard = env_lock();
        env::remove_var("AGENTS_WIKI_VAULT");
        let mut args = vec![
            "--vault".to_string(),
            "/tmp/cli-vault".to_string(),
            "paths".to_string(),
        ];

        let resolved = resolve_global_vault(&mut args).unwrap();

        assert_eq!(resolved.path, PathBuf::from("/tmp/cli-vault"));
        assert_eq!(resolved.source, VaultResolutionSource::Cli);
        assert_eq!(args, vec!["paths".to_string()]);
    }

    #[test]
    fn resolves_cli_vault_equals_flag() {
        let _guard = env_lock();
        env::remove_var("AGENTS_WIKI_VAULT");
        let mut args = vec!["--vault=/tmp/cli-vault".to_string(), "paths".to_string()];

        let resolved = resolve_global_vault(&mut args).unwrap();

        assert_eq!(resolved.path, PathBuf::from("/tmp/cli-vault"));
        assert_eq!(resolved.source, VaultResolutionSource::Cli);
        assert_eq!(args, vec!["paths".to_string()]);
    }

    #[test]
    fn resolves_env_vault() {
        let _guard = env_lock();
        env::set_var("AGENTS_WIKI_VAULT", "/tmp/env-vault");
        let mut args = vec!["paths".to_string()];

        let resolved = resolve_global_vault(&mut args).unwrap();

        assert_eq!(resolved.path, PathBuf::from("/tmp/env-vault"));
        assert_eq!(resolved.source, VaultResolutionSource::Env);
        env::remove_var("AGENTS_WIKI_VAULT");
    }

    #[test]
    fn resolves_config_vault() {
        let _guard = env_lock();
        let old_home = env::var("HOME").ok();
        let dir = temp_path("config-resolve");
        fs::create_dir_all(dir.join(".agents-wiki")).unwrap();
        fs::write(
            dir.join(".agents-wiki").join("config.yml"),
            "vault_path: /tmp/config-vault\n",
        )
        .unwrap();
        env::remove_var("AGENTS_WIKI_VAULT");
        env::set_var("HOME", &dir);
        let mut args = vec!["paths".to_string()];

        let resolved = resolve_global_vault(&mut args).unwrap();

        assert_eq!(resolved.path, PathBuf::from("/tmp/config-vault"));
        assert_eq!(resolved.source, VaultResolutionSource::Config);
        if let Some(home) = old_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_default_vault() {
        let _guard = env_lock();
        let old_home = env::var("HOME").ok();
        let dir = temp_path("default-resolve");
        fs::create_dir_all(&dir).unwrap();
        env::remove_var("AGENTS_WIKI_VAULT");
        env::set_var("HOME", &dir);
        let mut args = vec!["paths".to_string()];

        let resolved = resolve_global_vault(&mut args).unwrap();

        assert_eq!(resolved.path, dir.join("Documents/agents-wiki"));
        assert_eq!(resolved.source, VaultResolutionSource::Default);
        if let Some(home) = old_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_write_refuses_different_existing_path_without_force() {
        let dir = temp_path("config-refuse");
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.yml");
        fs::write(&config, "vault_path: /tmp/old\n").unwrap();

        let result = validate_config_update_at(&config, &PathBuf::from("/tmp/new"), false);

        assert!(result.unwrap_err().contains("--force"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_write_keeps_same_existing_path_without_force() {
        let dir = temp_path("config-keep");
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.yml");
        fs::write(&config, "vault_path: /tmp/same\n").unwrap();

        let result = write_config_vault_path_at(&config, &PathBuf::from("/tmp/same"), false)
            .expect("same path may be kept");

        assert_eq!(result, ConfigWriteResult::Kept);
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "vault_path: /tmp/same\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_write_overwrites_with_force() {
        let dir = temp_path("config-force");
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.yml");
        fs::write(&config, "vault_path: /tmp/old\n").unwrap();

        let result = write_config_vault_path_at(&config, &PathBuf::from("/tmp/new"), true)
            .expect("force overwrites");

        assert_eq!(result, ConfigWriteResult::Updated);
        assert_eq!(
            parse_config_vault_path(&fs::read_to_string(&config).unwrap()).as_deref(),
            Some("/tmp/new")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
