# E2E verification notes — CJK bigram tokenizer (feat/cjk-bigram-tokenizer)

## Setup
- Real Docker image `ghcr.io/jiansen/obsidian-mcp:feat-cjk-bigram-tokenizer`
  (built by the fork's Docker workflow from the PR branch, ghcr tag auto-derived).
- Run on Dev host with a 3-note mixed-language vault, queried over MCP HTTP
  `tools/call search_text` with full initialize/session handshake.

## Results (2026-10-06, Dev 37998)

| Query | Expectation | Result |
|---|---|---|
| `凝结水处理系统` (long CJK phrase) | condensate.md | ✅ PASS (was 0 hits before fix) |
| `阴离子交换器` (CJK subword) | anion.md | ✅ PASS |
| `混床` (2-char CJK) | condensate.md | ✅ PASS |
| `deployment pipeline` (EN stems) | rust.md | ✅ PASS |
| `docker` / `images` / `github actions` | rust.md | ✅ PASS |
| `processing` | rust.md | ❌ miss — **correct**: body contains "uses"(use) but never "processing"(process); the word is absent, not a regression. Equivalent en_stem behavior verified by tantivy probe (processing→process) |
| `processng` fuzzy:true | rust.md | ❌ 0 hits — pre-existing en_stem behavior, isolated with standalone tantivy probe (fuzzy edit-distance-1 does not bridge stem forms: processng vs process distance 3 after stemming). Not a regression; out of scope |

## Positive proof vs old code
Old en_stem index: `凝结水处理系统` → 0 hits (documented in ticket #114 probe on Prod).
New index: same query → hit with correct snippet and byte offsets.

## Log evidence
`docker logs cjk-e2e`:
```
INFO obsidian_mcp::vault: tantivy BM25 index built notes=3
INFO obsidian_mcp: HTTP MCP server listening addr=0.0.0.0:37842
```
