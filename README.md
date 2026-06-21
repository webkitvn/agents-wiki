# Agents Wiki

Local-first Obsidian wiki tooling for AI agents.

`agents-wiki` provides deterministic CLI primitives for agents to manage a markdown knowledge base: source ingest, source summaries, reviews, archive/trash lifecycle, doctor checks, linting, and Obsidian handoff.

## Core Idea

`agents-wiki` is the operational layer for an LLM-maintained Obsidian knowledge base. It is not a generic Obsidian automation wrapper, a RAG engine, or a human-facing notes app.

The CLI handles predictable bookkeeping: ingesting immutable `raw/` sources, scaffolding pages in `wiki/`, maintaining `wiki/index.md` and `wiki/log.md`, checking structure, and moving files through archive / trash lifecycle safely. The LLM handles synthesis: reading sources, updating entity / concept / question pages, adding cross-links, resolving contradictions, and deciding what knowledge belongs in the wiki.

Obsidian remains the knowledge environment. Obsidian CLI can support agent workflows with vault-native context such as opening files, backlinks, unresolved links, tags, tasks, properties, search context, and plugin commands. Those capabilities should accelerate the agent, not replace the filesystem-first safety model that keeps `agents-wiki` deterministic.

## Install

```bash
./scripts/install.sh
```

Or install directly from GitHub:

```bash
curl -fsSL https://raw.githubusercontent.com/webkitvn/agents-wiki/main/scripts/install.sh | bash
```

Common options:

```bash
./scripts/install.sh --vault "$HOME/Documents/Agents Wiki"
./scripts/install.sh --bin-dir "$HOME/.local/bin"
./scripts/install.sh --force-config
```

The installer:

- builds the Rust release binary
- copies it to `~/.local/bin/agents-wiki` by default
- creates `~/.agents-wiki/config.yml` if missing
- creates/scaffolds the configured vault with `agents-wiki doctor --repair`

## Configuration

Default config path:

```text
~/.agents-wiki/config.yml
```

Schema:

```yaml
vault_path: "/path/to/Agents Wiki"
```

Vault resolution precedence:

1. `--vault /path/to/vault`
2. `AGENTS_WIKI_VAULT=/path/to/vault`
3. `~/.agents-wiki/config.yml`
4. `~/Documents/Agents Wiki`

## Usage

For the full version-matched command reference:

```bash
agents-wiki guide
```

```bash
agents-wiki doctor
agents-wiki doctor --repair
agents-wiki status
agents-wiki next
agents-wiki new-source "Source title" --url "https://example.com"
agents-wiki source-summary "raw/source.md"
agents-wiki lint
```

Search and logging:

```bash
agents-wiki search "query" --limit 20
agents-wiki log <kind> "Title" --line "Detail line"
```

`search` matches case-insensitively across `raw/` and `wiki/`, printing
`path:line: text` for every matching line (defaults to 20 results, override
with `--limit`).

`source-summary`, `page`, and `review` automatically file the new page into
`wiki/index.md` under the matching section and append an entry to `wiki/log.md`.

`lint` reports missing index entries, missing citations, duplicate ids, orphan
pages (no inbound links), and stale active pages. Override the staleness window
with `--stale-days N` (default 90).

Open a vault path in Obsidian:

```bash
agents-wiki open "wiki/concepts/foo.md"
```

Lifecycle commands:

```bash
agents-wiki archive "wiki/concepts/foo.md" --reason "No longer active"
agents-wiki trash "raw/source.md" --reason "Accidental import"
agents-wiki trash-list
agents-wiki restore "trash/2026-06-20/raw/source.md"
```

## Development

```bash
cargo fmt --check
cargo test
cargo build --release
```

## License

MIT
