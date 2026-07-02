# Contributing to agents-wiki

`agents-wiki` is a Rust CLI for local-first Obsidian wiki tooling. Contributions
should keep the project focused on deterministic file operations, clear CLI
behavior, and safe vault path handling.

## Project Direction

This project is the operational layer for an LLM-maintained Obsidian knowledge
base. The CLI owns predictable bookkeeping:

- ingest immutable `raw/` sources
- scaffold and maintain generated `wiki/` pages
- update `wiki/index.md` and `wiki/log.md`
- check and repair vault structure
- keep deletion, rename history, and restore delegated to git

Do not expand the project into a generic Obsidian automation wrapper, a RAG
engine, or a human-facing notes app. The LLM owns synthesis and cross-linking;
the CLI owns deterministic primitives and safety.

## Local Setup

Install Rust stable, then clone the repo and build:

```bash
cargo build --locked
```

For checkout-local installation on macOS/Linux:

```bash
./scripts/install.sh
```

For checkout-local installation on Windows PowerShell:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

After installing the binary, initialize a test vault outside the repo:

```bash
agents-wiki init "$HOME/Documents/agents-wiki-test" --force
agents-wiki --vault "$HOME/Documents/agents-wiki-test" doctor
```

## Development Workflow

Before changing behavior, read:

- `AGENTS.md` for the repo contract
- `README.md` for user-facing setup and usage
- `skills/agents-wiki/GUIDE.md` for the embedded agent guide
- `docs/llm-wiki.md` for the design pattern behind the CLI

Keep changes small and traceable to the issue or request. Avoid speculative
abstractions, unrelated formatting churn, and broad refactors.

When changing CLI commands, flags, output, vault resolution, or command behavior,
update `skills/agents-wiki/GUIDE.md` in the same change. The binary embeds this
file for `agents-wiki guide`.

Update `skills/agents-wiki/SKILL.md` only when the skill bootstrap text or
trigger description changes.

## Verification

Run the full Rust check set for code changes:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

For docs-only changes, Rust verification is not required unless the docs describe
behavior that should be checked against the binary.

## Testing Expectations

Add or update tests for behavior changes, especially around:

- path resolution and vault safety
- `init`, `doctor`, and `doctor --repair`
- taxonomy resolution
- command output or exit codes
- git-available and git-missing behavior
- Windows path or config behavior

Prefer focused tests that reproduce the exact behavior being changed.

## Pull Requests

Before opening a PR:

1. Keep the diff scoped to the requested change.
2. Confirm generated docs and embedded guides are in sync when behavior changes.
3. Run the relevant verification commands.
4. Summarize the user-visible behavior change and the checks you ran.

Do not include unrelated cleanup in the same PR. If you notice dead code or a
separate issue, mention it separately.
