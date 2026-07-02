---
title: agents-wiki AI Operating Guide
description: Version-matched operating guide for LLM agents using the agents-wiki CLI.
audience: LLM agents
last_reviewed: 2026-07-02
source_of_truth: "Vault AGENTS.md plus the installed agents-wiki CLI behavior"
---

# agents-wiki AI Operating Guide

This guide is for LLM agents operating an `agents-wiki` vault. Use the CLI for
deterministic bookkeeping and use the vault's `AGENTS.md` as the wiki contract.

## Authority Order

When instructions conflict, follow this order:

1. The user's latest explicit request.
2. The vault's `AGENTS.md` contract.
3. This embedded AI operating guide.
4. The current CLI behavior shown by `agents-wiki guide`, `agents-wiki --help`, and command output.

## Core Rules

- MUST read and follow the vault's `AGENTS.md` before maintaining `wiki/`.
- MUST keep `raw/` immutable. Read raw sources; never translate, rewrite, move, or
  delete them unless the user explicitly asks.
- MUST write the entire generated `wiki/` layer in English: source summaries, pages,
  frontmatter summaries, titles, index entries, log entries, questions, reviews,
  entity pages, concept pages, and notes.
- MUST cite raw evidence on active wiki pages with the `raw/` path, `source_id`, or
  `provenance_source_ids`.
- MUST run `agents-wiki lint` before finishing wiki maintenance unless the user asks
  for read-only inspection.
- MUST use git for deletion, rename history, and restore. The CLI does not provide
  archive or trash lifecycle commands.
- MUST treat `GEMINI.md` and `CLAUDE.md` as compatibility aliases. `AGENTS.md`
  remains the authoritative vault contract.

Maintaining `wiki/` means creating or editing `wiki/sources/`, `wiki/entities/`,
`wiki/concepts/`, `wiki/questions/`, `wiki/reviews/`, `wiki/index.md`, and
`wiki/log.md`.

## Language Policy

| Case | Required behavior |
| --- | --- |
| Source content in `raw/` is English | Synthesize the wiki page in English and cite the raw source. |
| Source content in `raw/` is not English | Translate meaning into clear English synthesis for `wiki/`; keep `raw/` unchanged. |
| Proper noun, product name, technical term, or short quote needs the original language | Preserve the original wording only where needed for accuracy. |
| Translation nuance affects a claim | Cite the `raw/` source and include the original term in parentheses. |
| User asks for literal translation | Produce the translation requested by the user, but do not rewrite the raw source file. |

Do not produce sentence-by-sentence translation during normal ingest. The default
output is synthesized English wiki knowledge, not a translated copy of the source.

## Default Workflow

Use this workflow for source ingest and wiki maintenance:

1. Resolve the vault:

   ```bash
   agents-wiki paths
   ```

2. Read the vault contract:

   ```bash
   sed -n '1,220p' AGENTS.md
   ```

3. Inspect pending work:

   ```bash
   agents-wiki status
   agents-wiki next
   agents-wiki reviews --status open
   ```

4. Add the source to `raw/` only if it is not already present. Use one input form
   from the ingest decision table below.

5. Scaffold the source summary using the raw path printed by `new-source` or listed
   by `next`:

   ```bash
   agents-wiki source-summary "raw/<source-file>" --title "<English title>"
   ```

6. Read the raw source. Fill the source summary in English. Cite the raw path and
   `source_id`.

7. Update related entity, concept, question, source-summary, `wiki/index.md`, and
   `wiki/log.md` pages in English.

8. Open a review when a source contradicts existing wiki content or when translation
   ambiguity needs a decision:

   ```bash
   agents-wiki review "<English review title>" --reason "<why review is needed>" --source "raw/<source-file>"
   ```

9. Verify before finishing:

   ```bash
   agents-wiki lint
   ```

10. Report what changed, cite key wiki paths, and mention any unresolved review items.

## Ingest Decision Table

| Situation | Command | Result |
| --- | --- | --- |
| The user gives a URL | `agents-wiki new-source "<Title>" --url "<https-url>"` | Creates a markdown raw source with URL provenance. |
| The user gives a local PDF, image, markdown file, or text file | `agents-wiki new-source "<Title>" --file "/absolute/path/to/file"` | Copies the file into `raw/` and deduplicates by canonical ID. |
| The user gives short text directly in chat | `agents-wiki new-source "<Title>" --note "<text>"` | Creates a markdown raw source containing the note text. |
| The CLI reports duplicate exit code 2 | Reuse the existing raw source unless the user explicitly wants a duplicate. | Avoids duplicate source summaries. |
| The user explicitly wants to add a duplicate | Add `--force` to the chosen `new-source` command. | Creates the source even when canonical ID matches existing raw content. |

After `new-source`, capture the printed `raw path`, `source_id`, and `canonical_id`.
Use the printed raw path for `source-summary`; do not reconstruct temp paths by hand.

## Existing Vault Contract Updates

`agents-wiki doctor --repair` creates `AGENTS.md` only when it is missing. It does
not overwrite an existing vault contract.

`agents-wiki doctor --repair` also creates `GEMINI.md` and `CLAUDE.md` aliases when
they are missing. On Unix-like systems these aliases are relative symlinks to
`AGENTS.md`. If symlinks are unavailable, the CLI creates a small pointer file that
directs agents to `AGENTS.md`. Existing custom `GEMINI.md` or `CLAUDE.md` files are
not overwritten.

If an existing vault lacks the English wiki language policy:

1. Read the existing `AGENTS.md`.
2. Add a `Language policy` section that keeps `raw/` immutable and requires generated
   `wiki/` content to be English.
3. Preserve existing taxonomy frontmatter and vault-specific rules.
4. Run:

   ```bash
   agents-wiki doctor --repair
   agents-wiki lint
   ```

## Global Options And Vault Resolution

`--vault PATH` overrides the vault for any command. Resolution precedence:

1. `--vault PATH`
2. `AGENTS_WIKI_VAULT` env var
3. `~/.agents-wiki/config.yml` (`vault_path: "..."`)
4. `~/Documents/agents-wiki`

Many read commands accept `--json` for machine-readable output.

## Commands

### Guide

```bash
agents-wiki guide
```

Prints this embedded AI operating guide.

### Initialize

```bash
agents-wiki init "/path/to/agents-wiki" [--force]
```

Creates or keeps `~/.agents-wiki/config.yml`, runs `doctor --repair`, and then
runs:

```bash
npx skills add https://github.com/webkitvn/agents-wiki --skill
```

Use `--force` only when intentionally replacing the existing configured vault path.
Skill sync may prompt for target agents. If skill sync fails, `init` still succeeds.

### Update

```bash
agents-wiki update
```

Checks the latest semver tag, asks for confirmation, installs the new binary into
the current binary directory, runs `doctor --repair` on the resolved vault, and
syncs the skill with:

```bash
npx skills add https://github.com/webkitvn/agents-wiki --skill
```

If the binary is already current, `update` still runs the skill sync step.

### Inspect

```bash
agents-wiki status
agents-wiki paths
agents-wiki paths --json
agents-wiki next [--json]
```

- `status` prints the vault path, source/page counts, and last five log entries.
- `paths` prints absolute paths for `raw/`, `wiki/`, index, log, and related files.
- `paths --json` also prints `resolution_source`.
- `next` lists raw sources that still have no summary page.

### Ingest A Source

```bash
agents-wiki new-source "Title" --url "https://example.com"
agents-wiki new-source "Title" --file "/path/to/file.pdf"
agents-wiki new-source "Title" --note "free text"
agents-wiki new-source "Title" --file "/path/to/file.pdf" --force
```

`new-source` prints the raw path, `source_id`, and `canonical_id`. It exits 2 on
duplicate unless `--force` is provided.

```bash
agents-wiki source-summary "raw/2026-06-20-title.md" --title "English Title"
```

`source-summary` scaffolds a draft source page, files it into `wiki/index.md`, and
appends to `wiki/log.md`. After scaffolding, fill the page in English and update
related wiki pages.

### Create Wiki Pages

```bash
agents-wiki page entity "Name"
agents-wiki page concept "Topic"
agents-wiki page question "Question?"
agents-wiki review "Title" --reason "why" [--source "raw/x.md"] [--context "text"]
agents-wiki reviews [--status open] [--json]
```

New scaffolded pages include `summary: ""`, `provenance_source_ids`,
`provenance_has_inferred_content`, and `provenance_has_ambiguous_content`. Keep
`summary` to one or two concise English sentences when activating a page.

The valid `page` kinds, target folders, and `index.md` sections come from
`taxonomy:` frontmatter in the vault's `AGENTS.md`. Defaults are `entity`,
`concept`, `question`, and `review`. Edit the taxonomy to fit the vault domain,
then run:

```bash
agents-wiki doctor --repair
```

`source` is part of the taxonomy but is created through `source-summary`, not
`page`.

### Search And Log

```bash
agents-wiki search "query" [--limit N]
agents-wiki log <kind> "Title" --line "detail" [--line "more"]
agents-wiki open "wiki/concepts/foo.md"
```

- `search` matches case-insensitively across `raw/` and `wiki/`; default limit is 20.
- `log` appends to `wiki/log.md`.
- `open` opens a vault-relative path in Obsidian and rejects absolute paths or `..`.

### Lint And Doctor

```bash
agents-wiki lint [--json] [--stale-days N]
agents-wiki doctor [--json] [--repair]
```

`lint` reports missing index entries, missing citations, duplicate ids, orphan pages,
stale active pages, off-taxonomy pages, missing taxonomy sections, malformed
provenance, ambiguous provenance without a review link, and unreadable
`.manifest.json`. It exits 1 if any ERROR exists.

`doctor` checks vault structure, git, `.gitignore`, pending sources, and reviews.
`doctor --repair` scaffolds missing directories and core files (`index.md`,
`log.md`, `AGENTS.md`, `LLM Wiki.md`), creates `GEMINI.md` / `CLAUDE.md`
compatibility aliases, adds missing taxonomy section headings, creates
`.manifest.json`, initializes git when available, and fixes `.gitignore`.

## Path And Safety Rules

- All path arguments are relative to the vault root or absolute paths inside it.
- `open` only accepts vault-relative paths. It rejects `..` and absolute paths.
- Flags accept both `--flag value` and `--flag=value`.
- Unknown flags and non-numeric values for numeric options are rejected.
- `new-source --file` hashes raw bytes for binary files, so PDFs and images dedupe
  by content rather than by lossy text decoding.
- `.manifest.json` is deterministic bookkeeping for source/page paths and IDs. It
  is not a search index, RAG store, archive lifecycle, or graph database.
- `GEMINI.md` and `CLAUDE.md` are aliases only. Do not store divergent vault rules
  in those files; update `AGENTS.md` instead.
- Deleting and history are handled by git, e.g. `git rm`, `git mv`, `git restore`,
  and normal git history.

## Build And Install

```bash
cargo build --release
./scripts/install.sh
agents-wiki init "/path/to/agents-wiki"
```

The release binary is written to `target/release/agents-wiki`. The installer copies
it to the configured binary directory, usually `~/.local/bin`.
