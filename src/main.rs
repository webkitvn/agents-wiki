use agents_wiki::{args, context::Ctx, health, knowledge, setup, update};

const GUIDE: &str = include_str!("../skills/agents-wiki/GUIDE.md");
const OS_WINDOWS_START: &str = "<!-- agents-wiki:os:windows:start -->";
const OS_WINDOWS_END: &str = "<!-- agents-wiki:os:windows:end -->";
const OS_UNIX_START: &str = "<!-- agents-wiki:os:unix:start -->";
const OS_UNIX_END: &str = "<!-- agents-wiki:os:unix:end -->";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuideOs {
    Unix,
    Windows,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, String> {
    run_with_args(std::env::args().skip(1).collect())
}

fn run_with_args(mut argv: Vec<String>) -> Result<i32, String> {
    if first_command(&argv) == Some("guide") {
        return guide(&argv);
    }

    let resolved_vault = args::resolve_global_vault(&mut argv)?;
    let ctx = Ctx::new_with_resolution_source(resolved_vault.path, resolved_vault.source.as_str());

    let Some(command) = argv.first().cloned() else {
        print_help();
        return Ok(0);
    };
    let rest = &argv[1..];

    match command.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            Ok(0)
        }
        "init" => setup::init(rest),
        "status" => knowledge::status(&ctx),
        "paths" => knowledge::paths(&ctx, rest),
        "next" => knowledge::next(&ctx, rest),
        "new-source" => knowledge::new_source(&ctx, rest),
        "source-summary" => knowledge::source_summary(&ctx, rest),
        "page" => knowledge::page(&ctx, rest),
        "review" => knowledge::review(&ctx, rest),
        "reviews" => knowledge::reviews(&ctx, rest),
        "search" => knowledge::search(&ctx, rest),
        "lint" => health::lint(&ctx, rest),
        "doctor" => health::doctor(&ctx, rest),
        "update" => update::update(&ctx, rest),
        "reset" => health::reset(&ctx, rest),
        "log" => knowledge::log(&ctx, rest),
        "open" => knowledge::open(&ctx, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

fn guide(args: &[String]) -> Result<i32, String> {
    let os = guide_os(args)?;
    print!("{}", render_guide(GUIDE, os));
    Ok(0)
}

fn guide_os(args: &[String]) -> Result<GuideOs, String> {
    let mut index = args
        .iter()
        .position(|arg| arg == "guide")
        .map(|value| value + 1)
        .unwrap_or(0);
    let mut os = current_guide_os();
    while index < args.len() {
        let arg = &args[index];
        if arg == "--os" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| "--os requires windows, unix, or auto".to_string())?;
            os = parse_guide_os(value)?;
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--os=") {
            os = parse_guide_os(value)?;
            index += 1;
        } else {
            return Err(format!("unknown option: {arg}"));
        }
    }
    Ok(os)
}

fn current_guide_os() -> GuideOs {
    if cfg!(windows) {
        GuideOs::Windows
    } else {
        GuideOs::Unix
    }
}

fn parse_guide_os(value: &str) -> Result<GuideOs, String> {
    match value {
        "auto" => Ok(current_guide_os()),
        "unix" | "linux" | "macos" => Ok(GuideOs::Unix),
        "windows" => Ok(GuideOs::Windows),
        _ => Err(format!("unknown guide OS: {value}")),
    }
}

fn render_guide(input: &str, os: GuideOs) -> String {
    let mut out = String::new();
    let mut block: Option<GuideOs> = None;
    for line in input.lines() {
        match line {
            OS_WINDOWS_START => {
                block = Some(GuideOs::Windows);
                continue;
            }
            OS_WINDOWS_END => {
                block = None;
                continue;
            }
            OS_UNIX_START => {
                block = Some(GuideOs::Unix);
                continue;
            }
            OS_UNIX_END => {
                block = None;
                continue;
            }
            _ => {}
        }
        if block.is_none_or(|target| target == os) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn print_help() {
    println!("agents-wiki [--vault PATH] <command> [options]");
    println!();
    println!("Commands:");
    println!("  guide init status paths next new-source source-summary page review reviews");
    println!("  search lint doctor update log open");
    println!("  reset  WARNING: deletes all contents of the resolved vault after confirmation");
}

fn first_command(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--vault" {
            index += 2;
        } else if arg.starts_with("--vault=") {
            index += 1;
        } else {
            return Some(arg);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{first_command, guide_os, render_guide, run_with_args, GuideOs, GUIDE};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn finds_command_after_vault_flag() {
        assert_eq!(
            first_command(&args(&["--vault", "/tmp/vault", "guide"])),
            Some("guide")
        );
    }

    #[test]
    fn finds_command_after_vault_equals_flag() {
        assert_eq!(
            first_command(&args(&["--vault=/tmp/vault", "guide"])),
            Some("guide")
        );
    }

    #[test]
    fn guide_os_accepts_explicit_override() {
        assert_eq!(
            guide_os(&args(&["guide", "--os", "windows"])).unwrap(),
            GuideOs::Windows
        );
        assert_eq!(
            guide_os(&args(&["guide", "--os=unix"])).unwrap(),
            GuideOs::Unix
        );
    }

    #[test]
    fn guide_options_do_not_require_vault_resolution() {
        assert_eq!(
            guide_os(&args(&[
                "--vault",
                "/definitely/not/a/vault",
                "guide",
                "--os",
                "windows"
            ]))
            .unwrap(),
            GuideOs::Windows
        );
    }

    #[test]
    fn guide_command_bypasses_vault_resolution() {
        let code = run_with_args(args(&[
            "--vault",
            "/definitely/not/a/vault",
            "guide",
            "--os",
            "unix",
        ]))
        .unwrap();

        assert_eq!(code, 0);
    }

    #[test]
    fn render_guide_keeps_only_matching_os_block() {
        let guide = "common\n<!-- agents-wiki:os:windows:start -->\nwin\n<!-- agents-wiki:os:windows:end -->\n<!-- agents-wiki:os:unix:start -->\nunix\n<!-- agents-wiki:os:unix:end -->\nafter\n";

        let rendered = render_guide(guide, GuideOs::Windows);

        assert!(rendered.contains("common"));
        assert!(rendered.contains("win"));
        assert!(!rendered.contains("unix"));
        assert!(rendered.contains("after"));
    }

    #[test]
    fn embedded_windows_guide_hides_unix_only_sections() {
        let rendered = render_guide(GUIDE, GuideOs::Windows);

        assert!(!rendered.contains("agents-wiki:os:"));
        assert!(!rendered.contains("./scripts/install.sh"));
        assert!(!rendered.contains("~/.agents-wiki/config.yml"));
        assert!(!rendered.contains("On Unix-like systems"));
        assert!(rendered.contains("%APPDATA%\\agents-wiki\\config.yml"));
        assert!(rendered.contains(".\\scripts\\install.ps1"));
    }

    #[test]
    fn embedded_unix_guide_hides_windows_only_sections() {
        let rendered = render_guide(GUIDE, GuideOs::Unix);

        assert!(!rendered.contains("agents-wiki:os:"));
        assert!(!rendered.contains("PowerShell-safe"));
        assert!(!rendered.contains("%APPDATA%"));
        assert!(!rendered.contains("USERPROFILE"));
        assert!(!rendered.contains(".\\scripts\\install.ps1"));
        assert!(rendered.contains("~/.agents-wiki/config.yml"));
        assert!(rendered.contains("./scripts/install.sh"));
    }
}
