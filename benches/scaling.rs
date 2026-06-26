use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_agents-wiki")
}

fn make_vault(
    raw_count: usize,
    summary_count: usize,
    index_entries: usize,
    binary_assets: usize,
) -> TempDir {
    let dir = tempfile::tempdir().expect("temp vault");
    let vault = dir.path();

    fs::create_dir_all(vault.join("raw/assets")).unwrap();
    fs::create_dir_all(vault.join("wiki/sources")).unwrap();
    fs::create_dir_all(vault.join("wiki/concepts")).unwrap();

    fs::write(
        vault.join("AGENTS.md"),
        "---\ntaxonomy:\n  - kind: source\n    folder: sources\n    section: Sources\n  - kind: concept\n    folder: concepts\n    section: Concepts\n---\n\n# Agents\n",
    )
    .unwrap();

    for i in 0..raw_count {
        fs::write(
            vault.join(format!("raw/source-{i:05}.md")),
            format!(
                "---\ntitle: Source {i}\ntype: raw-source\nsource_id: src-{i:05}\ncanonical_id: can-text-{i:05}\n---\n\n# Source {i}\n\nneedle content {i}\n"
            ),
        )
        .unwrap();
    }

    for i in 0..summary_count {
        fs::write(
            vault.join(format!("wiki/sources/source-{i:05}.md")),
            format!(
                "---\ntitle: Source {i}\ntype: source-summary\nsource_path: raw/source-{i:05}.md\nsource_id: src-{i:05}\ncanonical_id: can-text-{i:05}\nstatus: draft\n---\n\n# Source {i}\n\nSummary text.\n"
            ),
        )
        .unwrap();
    }

    let mut index = String::from("# Wiki Index\n\n## Sources\n\n");
    for i in 0..index_entries {
        index.push_str(&format!("- [[wiki/sources/source-{i:05}]] — Source {i}\n"));
    }
    index.push_str("\n## Concepts\n\n");
    fs::write(vault.join("wiki/index.md"), index).unwrap();

    for i in 0..binary_assets {
        let bytes = vec![i as u8; 512 * 1024];
        fs::write(vault.join(format!("raw/assets/blob-{i:04}.bin")), bytes).unwrap();
    }

    dir
}

fn run_agents_wiki(vault: &Path, args: &[&str]) {
    let status = Command::new(bin())
        .arg("--vault")
        .arg(vault)
        .args(args)
        .status()
        .expect("run agents-wiki");

    assert!(status.success(), "command failed: {args:?}");
}

fn bench_next(c: &mut Criterion) {
    let mut group = c.benchmark_group("next_scaling");

    for size in [100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let vault = make_vault(size, size, size, 0);

            b.iter(|| {
                run_agents_wiki(black_box(vault.path()), black_box(&["next", "--json"]));
            });
        });
    }

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_scaling");

    for size in [100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let vault = make_vault(size, size, size, 50);

            b.iter(|| {
                run_agents_wiki(
                    black_box(vault.path()),
                    black_box(&["search", "needle", "--limit", "20"]),
                );
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_next, bench_search);
criterion_main!(benches);
