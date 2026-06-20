# Agents Wiki

Local-first Obsidian wiki tooling for AI agents.

`agents-wiki` provides deterministic CLI primitives for agents to manage a markdown knowledge base: source ingest, source summaries, reviews, archive/trash lifecycle, doctor checks, linting, and Obsidian handoff.

## Install

```bash
./scripts/install.sh
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

```bash
agents-wiki doctor
agents-wiki doctor --repair
agents-wiki status
agents-wiki next
agents-wiki new-source "Source title" --url "https://example.com"
agents-wiki source-summary "raw/source.md"
agents-wiki lint
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

