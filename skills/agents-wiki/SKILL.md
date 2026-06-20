---
name: agents-wiki
description: "Manage a local Obsidian markdown wiki / personal knowledge base via the agents-wiki CLI. Use when the user wants to add/ingest/save a source, article, link, note, or PDF into the wiki; remember something for later; search the wiki or notes; ask what they know about a topic; create or update a wiki page (entity/concept/question); file an answer back into the knowledge base; or lint/health-check/repair the vault. Triggers on: add to my wiki, ingest this, save this source, remember this, search my notes, what do I know about X, update the knowledge base, build a wiki page."
license: MIT
---

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
#   orphan pages, stale active pages. Exit 1 if any ERROR.

agents-wiki doctor [--json] [--repair]
#   checks vault structure / git / .gitignore / pending sources / reviews.
#   --repair scaffolds missing dirs and core files (index.md, log.md,
#   AGENTS.md, "LLM Wiki.md"), inits git, and fixes .gitignore.
```

Run `agents-wiki doctor --repair` once to initialize a new vault.

### Lifecycle (archive / trash / restore)

```bash
agents-wiki archive "wiki/concepts/foo.md" --reason "obsolete"  # -> wiki/archive/<date>/
agents-wiki trash "raw/source.md" --reason "accidental"          # -> trash/<date>/ (manifest)
agents-wiki trash-list [--json]
agents-wiki restore "trash/2026-06-20/raw/source.md" [--reason "..."]
```

`archive` only accepts `wiki/` paths; `trash` accepts any vault path. Core files
(`index.md`, `log.md`, `AGENTS.md`, the entrypoint, the CLI binary) are protected.

## Notes

- All path arguments are relative to the vault root (or absolute inside it).
- `--reason` is required for `archive` and `trash`.
- Flags accept both `--flag value` and `--flag=value`.
- Build from source: `cargo build --release` (binary at `target/release/agents-wiki`);
  install with `./scripts/install.sh`.
