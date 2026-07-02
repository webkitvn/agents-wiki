---
name: agents-wiki
description: "Manage a local Obsidian markdown wiki / personal knowledge base via the agents-wiki CLI. Use when the user wants to add/ingest/save a source, article, link, note, or PDF into the wiki; remember something for later; search the wiki or notes; ask what they know about a topic; create or update a wiki page (entity/concept/question); file an answer back into the knowledge base; or lint/health-check/repair the vault. Triggers on: add to my wiki, ingest this, save this source, remember this, search my notes, what do I know about X, update the knowledge base, build a wiki page."
license: MIT
---

# agents-wiki CLI

Use the embedded, version-matched AI operating guide:

```bash
agents-wiki guide
```

The guide is embedded in the binary from `skills/agents-wiki/GUIDE.md`, so the
installed command and its operating instructions stay in sync after each build.

Before maintaining a vault, read its `AGENTS.md` contract. `GEMINI.md` and
`CLAUDE.md` are compatibility aliases, not separate contracts. Keep `raw/`
immutable and write the generated `wiki/` layer in English.
