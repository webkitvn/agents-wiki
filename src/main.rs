use agents_wiki::{args, context::Ctx, health, knowledge, setup, update};

const GUIDE: &str = include_str!("../skills/agents-wiki/GUIDE.md");

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
    let mut argv: Vec<String> = std::env::args().skip(1).collect();

    if first_command(&argv) == Some("guide") {
        print!("{GUIDE}");
        return Ok(0);
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
    use super::first_command;

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
}
