# Agents Wiki

Local-first Obsidian wiki tooling for AI agents.

`agents-wiki` provides deterministic CLI primitives for agents to manage a markdown knowledge base: source ingest, source summaries, reviews, doctor checks, linting, and Obsidian handoff.

## Core Idea

`agents-wiki` is the operational layer for an LLM-maintained Obsidian knowledge base. It is not a generic Obsidian automation wrapper, a RAG engine, or a human-facing notes app.

The CLI handles predictable bookkeeping: ingesting immutable `raw/` sources, scaffolding pages in `wiki/`, maintaining `wiki/index.md` and `wiki/log.md`, and checking structure. The LLM handles synthesis: reading sources, updating entity / concept / question pages, adding cross-links, resolving contradictions, and deciding what knowledge belongs in the wiki. Versioning, deletion, and history are left to git.

By default, `raw/` sources stay in their original language and the LLM writes the
generated `wiki/` layer in English. The vault's `AGENTS.md` is the authoritative
contract for this policy.

The page taxonomy (which `kind`s exist, their `wiki/` folders, and `index.md` sections) is configured in the `taxonomy:` frontmatter of the vault's `AGENTS.md`, so structure can be adapted per domain without recompiling. Edit it, then run `agents-wiki doctor --repair`.

Obsidian remains the knowledge environment. Obsidian CLI can support agent workflows with vault-native context such as opening files, backlinks, unresolved links, tags, tasks, properties, search context, and plugin commands. Those capabilities should accelerate the agent, not replace the filesystem-first safety model that keeps `agents-wiki` deterministic.

## Install

Cross-platform install from GitHub:

```bash
cargo install --git https://github.com/webkitvn/agents-wiki.git --locked --force
```

Checkout-local install on macOS/Linux:

```bash
./scripts/install.sh
```

Common install option:

```bash
./scripts/install.sh --bin-dir "$HOME/.local/bin"
```

Checkout-local install on Windows PowerShell:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

Common Windows install option:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1 -BinDir "$env:USERPROFILE\.local\bin"
```

The checkout-local installers:

- builds the Rust release binary
- copy it to `~/.local/bin/agents-wiki` on macOS/Linux by default
- copy it to `%USERPROFILE%\.local\bin\agents-wiki.exe` on Windows by default

Then configure and scaffold your vault location:

macOS/Linux:

```bash
agents-wiki init "$HOME/Documents/agents-wiki"
```

Windows PowerShell:

```powershell
agents-wiki init "$env:USERPROFILE\Documents\agents-wiki"
```

Initialization scaffolds `AGENTS.md` as the vault contract and creates
`GEMINI.md` / `CLAUDE.md` compatibility aliases that point agents back to
`AGENTS.md`.

After initialization, the CLI also runs:

```bash
npx skills add https://github.com/webkitvn/agents-wiki --skill
```

The skill installer may prompt for target agents. If it fails, initialization
still succeeds and the CLI prints the command to rerun manually.

Update an installed binary from the latest semver tag:

```bash
agents-wiki update
```

On macOS/Linux, `update` asks for confirmation before changing anything. After a
successful binary update, it runs `doctor --repair` on the resolved vault using
the updated binary, then runs `npx skills add https://github.com/webkitvn/agents-wiki --skill`
to update the bundled skill.

On Windows, `update` checks the latest semver tag and prints a `cargo install`
command. Run that command from PowerShell, then run `agents-wiki doctor --repair`
on the vault. The CLI does not overwrite the running `agents-wiki.exe` in place.

Use `--force` only when you intentionally want to overwrite an existing
configured vault path:

```bash
agents-wiki init "$HOME/Documents/agents-wiki" --force
```

## Configuration

Default config path:

macOS/Linux:

```text
~/.agents-wiki/config.yml
```

Windows:

```text
%APPDATA%\agents-wiki\config.yml
```

On Windows, the CLI also reads legacy `%USERPROFILE%\.agents-wiki\config.yml` if
the primary `%APPDATA%` config does not exist. `init` always writes the primary
config path.

Schema:

```yaml
vault_path: "/path/to/agents-wiki"
```

Vault resolution precedence:

1. `--vault /path/to/vault`
2. `AGENTS_WIKI_VAULT=/path/to/vault`
3. OS config file
4. OS default documents vault

## Obsidian Web Clipper

Use Obsidian Web Clipper as a source capture tool. Clipped markdown should be
saved into `raw/`, not `wiki/`: `raw/` is the immutable source layer, while
`wiki/` is the LLM-maintained synthesis layer.

To save new clips directly into `raw/`:

1. Open the Obsidian Web Clipper extension settings.
2. Open the template you use for captures, usually `Default`.
3. Set the template's note location / folder to:

```text
raw
```

Use the vault-relative folder name `raw`, not `/raw` and not an absolute path.
For example, if your vault is `~/Documents/agents-wiki`, new clips should land
in `~/Documents/agents-wiki/raw`.

If the clipper popup still shows `Clippings`, change the popup folder field to
`raw` before saving the clip.

## Usage

For the full version-matched AI operating guide:

```bash
agents-wiki guide
```

```bash
agents-wiki init "$HOME/Documents/agents-wiki"
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

Deleting and history are handled by git (the vault is a git repo), e.g.
`git rm`, `git mv`, `git restore`. If git is not installed, filesystem
operations still work, but the vault has no versioning, deletion, or restore
safety until git is installed and initialized.

## Development

```bash
cargo fmt --check
cargo test
cargo build --release
```

## License

MIT
