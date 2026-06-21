# Agents Wiki Project Instructions

This is a Rust CLI for local-first Obsidian wiki tooling. Keep changes focused on deterministic file operations, clear CLI behavior, and safe vault path handling.

## Core Principle

`agents-wiki` is the operational layer for an LLM-maintained Obsidian knowledge base. Its job is to give AI agents small, deterministic primitives for wiki bookkeeping: ingesting raw sources, scaffolding wiki pages, maintaining `index.md` and `log.md`, and checking structure. Versioning and deletion are delegated to git rather than a bespoke lifecycle.

Do not turn this project into a generic Obsidian automation wrapper, a RAG engine, or a human-facing notes app. The LLM is responsible for synthesis, cross-linking, contradiction handling, and deciding what knowledge belongs in the wiki. This CLI is responsible for predictable file operations, vault safety, and conventions that keep agent work consistent across sessions.

Obsidian and Obsidian CLI may support the workflow by providing vault-native context such as opening files, backlinks, unresolved links, tags, tasks, properties, search context, or plugin commands. Treat those capabilities as agent accelerators, not as replacements for the core filesystem-first safety model unless a requested behavior specifically requires Obsidian runtime semantics.

## Design Source

The design follows the LLM-wiki pattern documented in [docs/llm-wiki.md](docs/llm-wiki.md): immutable `raw/` sources, an LLM-maintained `wiki/`, a schema file (this `AGENTS.md`), plus the ingest / query / lint operations and the `index.md` + `log.md` conventions. Use it as the reference when deciding behavior; the CLI provides deterministic primitives while synthesis stays the LLM's job.

## Keep the Skill in Sync

[skills/agents-wiki/GUIDE.md](skills/agents-wiki/GUIDE.md) is the user-facing CLI usage guide embedded into the binary and printed by `agents-wiki guide`. Whenever a change affects the CLI's command surface or behavior (commands, flags, arguments, output format, or vault resolution), update `GUIDE.md` in the same change so the rebuilt binary stays accurate. Update [skills/agents-wiki/SKILL.md](skills/agents-wiki/SKILL.md) only when the skill bootstrap text or `description` trigger phrases change.

## Development Commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

Use the full check set for Rust code changes. For docs-only changes, no Rust verification is required unless the docs describe behavior that should be checked.

## Rust Conventions

- Prefer borrowing over cloning unless ownership is required.
- Return `Result` for fallible behavior; do not add production `unwrap()` or `expect()`.
- Keep CLI/file-system errors explicit and user-readable.
- Add tests for behavior changes, especially path resolution, vault safety, taxonomy resolution, and repair/lint behavior.
- Do not introduce new dependencies unless the task clearly needs them.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" -> "Write tests for invalid inputs, then make them pass"
- "Fix the bug" -> "Write a test that reproduces it, then make it pass"
- "Refactor X" -> "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```text
1. [Step] -> verify: [check]
2. [Step] -> verify: [check]
3. [Step] -> verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.
