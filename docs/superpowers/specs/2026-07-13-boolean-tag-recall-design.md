---
title: Boolean Tag-Based Recall
date: 2026-07-13
status: approved
---

## Overview

Adds a boolean expression syntax over tags (`&` AND, `|` OR, `!` NOT, parens for grouping) usable in two places: `.hivemind.toml` session-start `recalls` entries, and a new optional `tags` parameter on the `memory_search` MCP tool. This is the implementation of a previously-recorded idea (`mem_13cd73bf5bfe42ffb720732fea17b030`, "boolean tag-based recall"), generalized from AND-only to full boolean logic per this session's discussion.

## Grammar

Standard precedence, `!` binds tightest, then `&`, then `|` (loosest) — matches conventional boolean-expression precedence so `a & b | c` reads as `(a & b) | c`:

```
expr     := or_expr
or_expr  := and_expr ( '|' and_expr )*
and_expr := not_expr ( '&' not_expr )*
not_expr := '!' not_expr | atom
atom     := 'tag:' <value> | '(' expr ')'
```

`<value>` is any run of characters that isn't whitespace or one of `& | ! ( )` — this covers ordinary namespaced tag values like `project:hivemind` or `lang:rust` without needing quotes.

Example: `tag:project:hivemind & (tag:lang:rust | tag:lang:vue) & !tag:status:done`

Matching is case-insensitive on the tag comparison (tags are already lowercased at write time by the existing tag-namespace-system work, so this is mostly a formality, not new normalization logic).

## Detection (`.hivemind.toml` recalls)

A recall string is parsed as a tag expression only if, after trimming, it starts with `tag:`, `!tag:`, or `(`. This keeps the change fully backward compatible: every existing plain-title/FTS recall string in the wild is untouched, since none of them start with those sequences by convention.

- Doesn't start with one of those prefixes → resolved exactly as today (`resolve_recall`: exact title match, then FTS fallback), zero behavior change.
- Starts with one of those prefixes and parses successfully → evaluated as a tag expression (see Multiplicity below).
- Starts with one of those prefixes but fails to parse → a real, surfaced error (e.g. unbalanced parens) — not a silent fallback to searching for the literal malformed string via FTS, since that would be confusing (the user clearly intended a tag query and made a mistake).

Known, accepted edge case: a plain memory title that happens to literally start with `(` would be misidentified as an attempted tag expression and fail to parse as one (surfacing a parse error instead of being treated as a plain title lookup). Extremely unlikely for real memory titles; not worth more detection complexity to eliminate.

## Multiplicity

A tag-expression recall entry loads **all** matching memories (not just the first), still subject to the overall `max_tokens` budget with the existing skip-and-continue behavior (a later, smaller entry can still fit even if an earlier one didn't). A plain title/FTS recall entry continues to load only its single best match — this distinction is a natural consequence of what each mode means (an exact title lookup has at most one right answer; a tag query can match many memories), not an arbitrary special case.

## Architecture

- New module `src/tag_query.rs`: parses a `&str` into a small AST (`TagExpr::Tag(String) | And(Box<TagExpr>, Box<TagExpr>) | Or(...) | Not(Box<TagExpr>)`), and evaluates an AST against a memory's tag set (`&[String]` or `&HashSet<String>`).
- New `SqliteStore` method (e.g. `find_by_tag_expr`) that fetches all memories plus a single batched query for all `(memory_id, tag)` pairs (avoiding N+1 per-memory tag lookups), groups tags by memory id, and filters via the `tag_query` evaluator. Scale is fine for this tool's realistic memory counts (hundreds, not millions) — no need for a SQL-level boolean translation.
- `session.rs`'s `execute_session_start` calls the tag-expression detector/parser per recall entry; if it's a tag expression, calls the new store method and pushes every result (subject to budget) instead of the existing single-entry path.
- `memory_search`'s new optional `tags: Vec<String>` parameter is the AND-only special case of the same capability — internally builds a `TagExpr` that ANDs every provided tag together (no OR/NOT exposed there, since it's a plain JSON array param, not a string grammar) and reuses the same store method, ANDed with the existing free-text FTS `query` filter when both are provided. `tags` alone (with `query` omitted or empty) is valid — a pure tag-boolean search with no keyword component.
- `memory_recall` is unchanged — its id/title single-lookup semantics don't fit a multi-match tag query (explicit scope decision from this session's discussion).

## Documentation

`README.md`'s existing `.hivemind.toml` section (~line 325-351) gets the tag-expression syntax and an example alongside the existing `recalls` explanation. The `memory_search` mention (~line 37) gets a note about the new `tags` parameter.

## Explicitly Out of Scope

- No SQL-level boolean query translation (Rust-side evaluation over a batched fetch is sufficient at this tool's scale).
- No OR/NOT for `memory_search`'s `tags` array param (AND-only, by design — full boolean logic there would require it to become a string expression instead of a plain array, which wasn't requested).
- No change to `memory_recall`.
- No quoting requirement for tag values in the `.hivemind.toml` grammar (values can't contain the operator/whitespace characters that would need escaping, given how tags are actually formed in this system).

## Testing

Backend: `src/tag_query.rs` gets direct unit tests for the parser (valid expressions, precedence, unbalanced-paren errors) and the evaluator (AND/OR/NOT truth tables against sample tag sets). `src/store.rs`'s new method gets tests following existing patterns (e.g. `store_persists_row_and_tags`-style fixtures). `session.rs`'s `execute_session_start` gets a test for a tag-expression recall loading multiple memories under budget, and one for a malformed tag expression surfacing as an error rather than a silent FTS fallback. `src/api.rs`/`src/server.rs`'s `memory_search` gets a test for the new `tags` param (tags-only, and tags+query combined).

Frontend: no dashboard changes in this pass (this feature is `.hivemind.toml`/MCP-only) — nothing to verify manually in the browser for this one.
