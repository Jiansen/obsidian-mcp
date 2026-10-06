# CJK single-char query gap analysis (#114 follow-up)

## Finding

`query="床"` returns 0 hits while `query="混床"` returns 1 hit.
Confirmed with a standalone probe — this is **inherent to pure bigram
indexing**, not a bug in our tokenizer:

- Doc text `混床工艺` indexes bigrams: `混床`, `床工`, `工艺`
  — there is NO unigram term `床` in the posting lists.
- Query `床` tokenizes to a single unigram term `床` → no posting → 0 hits.
- Single-char docs (run length 1) DO emit a unigram, so `床` only matches
  docs where 床 appears alone.

This matches Lucene's `CJKBigramFilter` default (bigram-only output) — the
same "single char CJK query doesn't match bigram runs" tradeoff exists there,
which is why Lucene ships `CJKWidthFilter`/unigram modes and SmartChineseAnalyzer.

## Options considered

| Option | Effect | Cost |
|---|---|---|
| A. Index unigram+bigram (2 tokens per position, position_length=2) | single-char queries work; recall ↑ | index size ↑ further; BM25 term stats get noisier (unigrams very frequent → low IDF, ranking mildly diluted) |
| B. Accept gap (bigram-only) | matches Lucene CJK default; single-char queries are rare in engineering search | users typing a single char get 0 hits |
| C. Query-side fallback: if tokenized query is 1 token of len 1, rewrite to prefix/wildcard over bigram index | fixes UX without index bloat | custom code in search path; wildcard cost bounded by term dictionary scan |

## Decision for SafBon vault

Engineering queries in this vault are ≥2 chars in practice (checked
audit queries: 凝结水, 混床, 阴离子交换器, SiO2...). Take option B now
(documented), keep A as upgrade path if single-char queries show up in
usage data.

## Note on QueryParser multi-term behavior (verified)

Multi-token queries (e.g. `凝结水处理系统` → 6 bigrams) become a PHRASE
query automatically (tantivy `generate_literals_for_str` builds
`LogicalLiteral::Phrase` when the field tokenizer emits >1 token, requiring
positions — we index WithFreqsAndPositions). This is what keeps precision:
`处理系统` will not match scattered 处…理…系…统.
