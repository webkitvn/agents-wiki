use agents_wiki::{
    context::Ctx,
    util::{add_index_entry, frontmatter, SummaryIndex},
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs;
use tempfile::TempDir;

fn make_vault(raw_count: usize, summary_count: usize, index_entries: usize) -> TempDir {
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

    dir
}

fn bench_summary_index_build(c: &mut Criterion) {
    let vault = make_vault(1000, 1000, 1000);
    let ctx = Ctx::new(vault.path().to_path_buf());

    c.bench_function("summary_index_build_1000", |b| {
        b.iter(|| {
            SummaryIndex::build(black_box(&ctx));
        });
    });
}

fn bench_summary_index_lookup(c: &mut Criterion) {
    let vault = make_vault(1000, 1000, 1000);
    let ctx = Ctx::new(vault.path().to_path_buf());
    let index = SummaryIndex::build(&ctx);
    let raw_file = vault.path().join("raw/source-00500.md");

    c.bench_function("summary_index_lookup_1000", |b| {
        b.iter(|| {
            index.contains_source(black_box(&ctx), black_box(&raw_file));
        });
    });
}

fn bench_frontmatter_parse(c: &mut Criterion) {
    let vault = make_vault(1, 1, 1);
    let summary_file = vault.path().join("wiki/sources/source-00000.md");

    c.bench_function("frontmatter_parse", |b| {
        b.iter(|| {
            frontmatter(black_box(&summary_file));
        });
    });
}

fn bench_add_index_entry(c: &mut Criterion) {
    let vault = make_vault(100, 100, 1000);
    let ctx = Ctx::new(vault.path().to_path_buf());
    let mut count = 0;

    c.bench_function("add_index_entry_large_index", |b| {
        b.iter(|| {
            count += 1;
            add_index_entry(
                black_box(&ctx),
                black_box("Sources"),
                black_box(&format!("wiki/sources/new-page-{count}.md")),
                black_box(&format!("New Page {count}")),
            )
            .unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_summary_index_build,
    bench_summary_index_lookup,
    bench_frontmatter_parse,
    bench_add_index_entry
);
criterion_main!(benches);
