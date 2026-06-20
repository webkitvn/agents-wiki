mod args;
mod context;
mod health;
mod knowledge;
mod lifecycle;
mod util;

use context::Ctx;

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
    let vault = args::parse_global_vault(&mut argv)?;
    let ctx = Ctx::new(vault);

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
        "status" => knowledge::status(&ctx),
        "paths" => knowledge::paths(&ctx),
        "next" => knowledge::next(&ctx, rest),
        "new-source" => knowledge::new_source(&ctx, rest),
        "source-summary" => knowledge::source_summary(&ctx, rest),
        "page" => knowledge::page(&ctx, rest),
        "review" => knowledge::review(&ctx, rest),
        "reviews" => knowledge::reviews(&ctx, rest),
        "archive" => lifecycle::archive(&ctx, rest),
        "trash" => lifecycle::trash(&ctx, rest),
        "trash-list" => lifecycle::trash_list(&ctx, rest),
        "restore" => lifecycle::restore(&ctx, rest),
        "search" => knowledge::search(&ctx, rest),
        "lint" => health::lint(&ctx, rest),
        "doctor" => health::doctor(&ctx, rest),
        "log" => knowledge::log(&ctx, rest),
        "open" => knowledge::open(&ctx, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

fn print_help() {
    println!("agents-wiki [--vault PATH] <command> [options]");
    println!();
    println!("Commands:");
    println!("  status paths next new-source source-summary page review reviews");
    println!("  archive trash trash-list restore search lint doctor log open");
}
