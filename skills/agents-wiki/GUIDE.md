# agents-wiki CLI

Command reference for the `agents-wiki` binary. Invoke as `agents-wiki <command> [options]`.

## Global options & vault resolution

`--vault PATH` overrides the vault for any command. Resolution precedence:

1. `--vault PATH`
2. `AGENTS_WIKI_VAULT` env var
3. `~/.agents-wiki/config.yml` (`vault_path: "..."`)
4. `~/Documents/Agents Wiki`

Many read commands accept `--json` for machine-readable output.

## Commands

### Guide

```bash
agents-wiki guide          # print this embedded guide
```

### Inspect

```bash
agents-wiki status          # vault path, source/page counts, last 5 log entries
agents-wiki paths           # absolute paths for raw/, wiki/, index, log, etc.
agents-wiki next [--json]   # raw sources that still have no summary page
```

### Ingest a source

```bash
# Create a raw source (deduped by canonical id). Pick ONE input form:
agents-wiki new-source "Title" --url "https://example.com"
agents-wiki new-source "Title" --file "/path/to/file.pdf"
agents-wiki new-source "Title" --note "free text"
#   --force   add even if a duplicate canonical id exists
# Prints: raw path, source_id, canonical_id. Exit 2 on duplicate.

# Scaffold the summary page for a raw source (auto-files into index + log):
agents-wiki source-summary "raw/2026-06-20-title.md" --title "Title"
```

### Create wiki pages

```bash
agents-wiki page entity   "Name"      # -> wiki/entities/...,  filed under ## Entities
agents-wiki page concept  "Topic"     # -> wiki/concepts/...,  ## Concepts
agents-wiki page question "Question?"  # -> wiki/questions/..., ## Questions

agents-wiki review "Title" --reason "why" [--source "raw/x.md"] [--context "text"]
agents-wiki reviews [--status open] [--json]   # list review items
```

The valid `page` kinds, their target folders, and their `index.md` sections come
from the `taxonomy:` frontmatter in the vault's `AGENTS.md` (defaults: entity,
concept, question, review). Edit that list to fit your domain, then run
`agents-wiki doctor --repair` to scaffold any new folders — no recompile needed.
(`source` is part of the taxonomy but created via `source-summary`, not `page`.)

### Search & log

```bash
agents-wiki search "query" [--limit N]   # case-insensitive over raw/ + wiki/,
                                         # prints path:line: text (default 20)
agents-wiki log <kind> "Title" --line "detail" [--line "more"]  # append to log.md
agents-wiki open "wiki/concepts/foo.md"  # open a vault path in Obsidian
```

### Lint & doctor

```bash
agents-wiki lint [--json] [--stale-days N]   # default stale window 90 days
#   reports: missing index entries, missing citations, duplicate ids,
#   orphan pages, stale active pages, and off-taxonomy pages (pages not under a
#   taxonomy folder — move them or extend AGENTS.md `taxonomy`). Exit 1 if any ERROR.

agents-wiki doctor [--json] [--repair]
#   checks vault structure / git / .gitignore / pending sources / reviews.
#   --repair scaffolds missing dirs and core files (index.md, log.md,
#   AGENTS.md, "LLM Wiki.md"), inits git, and fixes .gitignore.
```

Run `agents-wiki doctor --repair` once to initialize a new vault.

### Deleting & history

There is no archive/trash command. The vault is a git repo, so use git for
versioning, deletion, and restore (`git rm`, `git mv`, `git restore`, history).

## Notes

- All path arguments are relative to the vault root (or absolute inside it).
- Flags accept both `--flag value` and `--flag=value`.
- Build from source: `cargo build --release` (binary at `target/release/agents-wiki`);
  install with `./scripts/install.sh`.
