use std::process::Command;

const SKILL_REPO_URL: &str = "https://github.com/webkitvn/agents-wiki";

pub const SKILL_SYNC_COMMAND: &str =
    "npx skills add https://github.com/webkitvn/agents-wiki --skill";

pub fn sync_agents_wiki_skill() -> Result<(), String> {
    println!("Syncing agents-wiki skill...");
    let status = Command::new("npx")
        .args(skill_add_args())
        .status()
        .map_err(|err| format!("failed to run npx skills add: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("failed to sync agents-wiki skill with `npx skills add`".to_string())
    }
}

pub fn warn_if_agents_wiki_skill_sync_fails(completed_step: &str) {
    if let Err(err) = sync_agents_wiki_skill() {
        eprintln!("WARNING: {completed_step} completed, but agents-wiki skill sync failed: {err}");
        eprintln!("Run manually: {SKILL_SYNC_COMMAND}");
    }
}

fn skill_add_args() -> [&'static str; 4] {
    ["skills", "add", SKILL_REPO_URL, "--skill"]
}

#[cfg(test)]
mod tests {
    use super::skill_add_args;

    #[test]
    fn builds_skill_add_command_args() {
        assert_eq!(
            skill_add_args(),
            [
                "skills",
                "add",
                "https://github.com/webkitvn/agents-wiki",
                "--skill"
            ]
        );
    }
}
