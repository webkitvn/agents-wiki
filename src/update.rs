use crate::{
    context::Ctx,
    health::validate_flags,
    skills::{warn_if_agents_wiki_skill_sync_fails, SKILL_SYNC_COMMAND},
};
use std::{
    cmp::Ordering,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const REPO_URL: &str = "https://github.com/webkitvn/agents-wiki.git";
const GIT_COMMAND: &str = "git";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ReleaseTag {
    name: String,
    version: Version,
}

pub fn update(ctx: &Ctx, args: &[String]) -> Result<i32, String> {
    validate_flags(args, &[])?;

    let current = parse_version(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| "current package version is not a valid semver version".to_string())?;
    let latest = latest_release_tag(REPO_URL)?;
    let install_dir = current_exe_dir()?;
    let install_path = install_dir.join(binary_name());

    if latest.version <= current {
        println!("agents-wiki is up to date ({})", env!("CARGO_PKG_VERSION"));
        warn_if_agents_wiki_skill_sync_fails("agents-wiki update check");
        return Ok(0);
    }

    if cfg!(windows) {
        print!("{}", windows_update_guidance(&latest.name));
        return Ok(0);
    }

    println!(
        "agents-wiki {} -> {}",
        env!("CARGO_PKG_VERSION"),
        latest.name
    );
    println!("Install path: {}", install_path.display());
    println!("Repair vault: {}", ctx.vault.display());
    print!("{}", update_prompt(&latest.name, &ctx.vault));
    io::stdout().flush().map_err(|err| err.to_string())?;

    if !confirm_default_yes()? {
        println!("Update cancelled.");
        return Ok(0);
    }

    install_release(REPO_URL, &latest.name, &install_dir)?;
    run_repair(&install_path, ctx)?;
    warn_if_agents_wiki_skill_sync_fails("agents-wiki update and doctor --repair");
    Ok(0)
}

fn latest_release_tag(repo_url: &str) -> Result<ReleaseTag, String> {
    let output = Command::new(GIT_COMMAND)
        .args(["ls-remote", "--tags", "--refs", repo_url, "v*"])
        .output()
        .map_err(|_| "git is required to update agents-wiki from GitHub releases".to_string())?;

    if !output.status.success() {
        return Err(format!(
            "failed to fetch release tags: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    latest_tag_from_ls_remote(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "no semver release tags found".to_string())
}

fn latest_tag_from_ls_remote(text: &str) -> Option<ReleaseTag> {
    text.lines()
        .filter_map(|line| line.split_once("refs/tags/").map(|(_, tag)| tag.trim()))
        .filter_map(parse_release_tag)
        .max_by(|left, right| left.version.cmp(&right.version))
}

fn parse_release_tag(tag: &str) -> Option<ReleaseTag> {
    let version = parse_version(tag.strip_prefix('v')?)?;
    Some(ReleaseTag {
        name: tag.to_string(),
        version,
    })
}

fn parse_version(text: &str) -> Option<Version> {
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

fn confirm_default_yes() -> Result<bool, String> {
    let mut answer = String::new();
    let bytes = io::stdin()
        .read_line(&mut answer)
        .map_err(|err| err.to_string())?;
    if bytes == 0 {
        return Ok(false);
    }
    let value = answer.trim();
    Ok(value.is_empty() || value.eq_ignore_ascii_case("y") || value.eq_ignore_ascii_case("yes"))
}

fn update_prompt(tag: &str, vault: &Path) -> String {
    format!(
        "Update agents-wiki to {tag}, run doctor --repair on {}, and sync the skill with `{SKILL_SYNC_COMMAND}`? [Y/n] ",
        vault.display()
    )
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "agents-wiki.exe"
    } else {
        "agents-wiki"
    }
}

fn cargo_install_command(tag: &str) -> String {
    format!("cargo install --git {REPO_URL} --tag {tag} --locked --force")
}

fn windows_update_guidance(tag: &str) -> String {
    format!(
        "agents-wiki {} -> {tag}\n\nWindows cannot safely replace the running agents-wiki.exe in place.\nRun this command from PowerShell to update:\n\n  {}\n\nThen run `agents-wiki doctor --repair` on your vault.\n",
        env!("CARGO_PKG_VERSION"),
        cargo_install_command(tag)
    )
}

fn install_release(repo_url: &str, tag: &str, install_dir: &Path) -> Result<(), String> {
    let temp = TempUpdateDir::new()?;
    let checkout = temp.path().join("agents-wiki");
    let clone_status = Command::new(GIT_COMMAND)
        .args(["clone", "--depth", "1", "--branch", tag, repo_url])
        .arg(&checkout)
        .status()
        .map_err(|_| "git is required to update agents-wiki from GitHub releases".to_string())?;

    if !clone_status.success() {
        return Err(format!("failed to clone agents-wiki release {tag}"));
    }

    let install_status = Command::new("bash")
        .arg(checkout.join("scripts/install.sh"))
        .arg("--bin-dir")
        .arg(install_dir)
        .status()
        .map_err(|err| err.to_string())?;

    if install_status.success() {
        Ok(())
    } else {
        Err(format!("failed to install agents-wiki release {tag}"))
    }
}

fn run_repair(binary: &Path, ctx: &Ctx) -> Result<(), String> {
    let status = Command::new(binary)
        .arg("--vault")
        .arg(&ctx.vault)
        .arg("doctor")
        .arg("--repair")
        .status()
        .map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "updated agents-wiki, but doctor --repair failed for {}",
            ctx.vault.display()
        ))
    }
}

fn current_exe_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    exe.parent().map(|path| path.to_path_buf()).ok_or_else(|| {
        format!(
            "could not determine install directory for {}",
            exe.display()
        )
    })
}

struct TempUpdateDir {
    path: PathBuf,
}

impl TempUpdateDir {
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("agents-wiki-update-{nonce}"));
        fs::create_dir_all(&path).map_err(|err| err.to_string())?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempUpdateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        binary_name, cargo_install_command, latest_tag_from_ls_remote, parse_release_tag,
        parse_version, update_prompt, windows_update_guidance, TempUpdateDir, Version,
    };
    use crate::skills::SKILL_SYNC_COMMAND;
    use std::path::Path;

    #[test]
    fn parses_strict_semver() {
        assert_eq!(
            parse_version("1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.2.x"), None);
    }

    #[test]
    fn parses_release_tags_with_v_prefix() {
        let tag = parse_release_tag("v0.4.1").expect("valid tag");
        assert_eq!(tag.name, "v0.4.1");
        assert_eq!(
            tag.version,
            Version {
                major: 0,
                minor: 4,
                patch: 1
            }
        );
        assert!(parse_release_tag("0.4.1").is_none());
        assert!(parse_release_tag("v0.4.1-beta").is_none());
    }

    #[test]
    fn picks_latest_semver_tag_from_ls_remote_output() {
        let output = "\
aaaaaaaa\trefs/tags/v0.3.0
bbbbbbbb\trefs/tags/v0.10.0
cccccccc\trefs/tags/v0.4.1
dddddddd\trefs/tags/not-a-version
";

        let latest = latest_tag_from_ls_remote(output).expect("latest tag");

        assert_eq!(latest.name, "v0.10.0");
        assert_eq!(
            latest.version,
            Version {
                major: 0,
                minor: 10,
                patch: 0
            }
        );
    }

    #[test]
    fn update_prompt_names_repair_and_skill_sync() {
        let prompt = update_prompt("v1.2.3", Path::new("/tmp/wiki"));

        assert!(prompt.contains("v1.2.3"));
        assert!(prompt.contains("doctor --repair"));
        assert!(prompt.contains("/tmp/wiki"));
        assert!(prompt.contains(SKILL_SYNC_COMMAND));
    }

    #[test]
    fn binary_name_matches_platform() {
        if cfg!(windows) {
            assert_eq!(binary_name(), "agents-wiki.exe");
        } else {
            assert_eq!(binary_name(), "agents-wiki");
        }
    }

    #[test]
    fn windows_guidance_uses_cargo_install_with_tag() {
        let command = cargo_install_command("v1.2.3");
        assert_eq!(
            command,
            "cargo install --git https://github.com/webkitvn/agents-wiki.git --tag v1.2.3 --locked --force"
        );
        let guidance = windows_update_guidance("v1.2.3");
        assert!(guidance.contains("agents-wiki.exe"));
        assert!(guidance.contains(&command));
    }

    #[test]
    fn temp_update_dir_removes_directory_on_drop() {
        let path = {
            let temp = TempUpdateDir::new().expect("temp dir");
            let path = temp.path().to_path_buf();
            assert!(path.is_dir());
            path
        };

        assert!(!path.exists());
    }
}
