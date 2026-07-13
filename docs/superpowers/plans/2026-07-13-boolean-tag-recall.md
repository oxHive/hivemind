# Boolean Tag-Based Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a boolean tag-expression syntax (`&`/`|`/`!`/parens) usable in `.hivemind.toml` session-start `recalls` entries and as a new optional `tags` parameter on the `memory_search` MCP tool.

**Architecture:** A new standalone module, `src/tag_query.rs`, owns the grammar: parsing a string into a small `TagExpr` AST and evaluating it against a memory's tag list. A new `SqliteStore::find_by_tag_expr` method (reusing the existing `list_memories` row-plus-tags fetch, consistent with how `list_memories`/`search` already fetch tags per row — no new bulk-query machinery, since this tool's realistic memory counts don't need it) is the single place that turns an expression into matching memories. Two call sites consume it: `session.rs`'s `execute_session_start` (for `.hivemind.toml` recalls) and `server.rs`'s `do_memory_search` (for the new `tags` param, which is the AND-only special case of the same `TagExpr`).

**Tech Stack:** Rust (existing `anyhow` for errors, no new dependencies — the grammar is simple enough for a small hand-rolled recursive-descent parser).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-13-boolean-tag-recall-design.md` — every requirement below traces to it.
- Grammar: `!` binds tightest, then `&`, then `|` (loosest). Atoms are `tag:<value>` or a parenthesized sub-expression.
- `.hivemind.toml` recall-string detection: a string is treated as a tag expression only if, trimmed, it starts with `tag:`, `!tag:`, or `(`. Anything else resolves exactly as today (unchanged: exact title, then FTS). If it starts with one of those prefixes but fails to parse, that failure must propagate as a real error through the existing warn-and-skip mechanism in `execute_session_start` — it must never silently fall through to an FTS search on the literal malformed string.
- Multiplicity: a tag-expression recall entry loads ALL matching memories (each still subject to the overall `max_tokens` budget, skip-and-continue per entry as today). A plain title/FTS recall entry continues to load only its single best match — this is unchanged existing behavior, do not touch it.
- Tag comparison is case-insensitive in practice: the parser lowercases every tag value it reads, and memory tags are already lowercased at write time by prior work (`src/store.rs`'s `validate_single_project_tag`/lowercasing) — so direct string equality inside `eval` is correct and sufficient, no `.to_lowercase()` needed at comparison time.
- `memory_search`'s new `tags: Option<Vec<String>>` param is AND-only (no OR/NOT exposed there — it's a plain JSON array, not a string grammar). It's combinable with the existing free-text `query`; either can be provided alone, or both together, but at least one of them must be present (empty/absent `query` AND absent/empty `tags` still returns zero results, same behavior as today's empty-query case).
- `memory_recall` is unchanged — do not touch it.
- No new SQL-level boolean query translation — evaluate in Rust over an already-fetched entry list, matching this codebase's existing scale assumptions (`src/store.rs`'s `export()` already does a `list_memories(100_000, 0)` full-table style fetch).
- Every existing test must keep passing — `execute_session_start`'s restructuring must preserve the exact current behavior for non-tag-expression recall entries (single-best-match, skip-on-budget-exceeded, continue-to-next-entry).

---

### Task 1: `src/tag_query.rs` — parser and evaluator

**Files:**
- Create: `src/tag_query.rs`
- Modify: `src/lib.rs` (register the new module)

**Interfaces:**
- Produces: `pub enum TagExpr { Tag(String), And(Box<TagExpr>, Box<TagExpr>), Or(Box<TagExpr>, Box<TagExpr>), Not(Box<TagExpr>) }`; `pub fn looks_like_tag_expr(s: &str) -> bool`; `pub fn parse(s: &str) -> anyhow::Result<TagExpr>`; `TagExpr::eval(&self, tags: &[String]) -> bool`; `TagExpr::and_all(tags: &[String]) -> Option<TagExpr>`. Tasks 2, 3, and 4 all consume these exact names/signatures.

- [ ] **Step 1: Write the failing tests**

Create `src/tag_query.rs` with just the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_tag_expr_detects_expected_prefixes() {
        assert!(looks_like_tag_expr("tag:project:hivemind"));
        assert!(looks_like_tag_expr("!tag:status:done"));
        assert!(looks_like_tag_expr("(tag:a & tag:b)"));
        assert!(looks_like_tag_expr("  tag:project:hivemind")); // leading whitespace trimmed
        assert!(!looks_like_tag_expr("my exact memory title"));
        assert!(!looks_like_tag_expr("plain fts keywords"));
    }

    #[test]
    fn parses_single_tag() {
        let expr = parse("tag:project:hivemind").unwrap();
        assert_eq!(expr, TagExpr::Tag("project:hivemind".to_string()));
    }

    #[test]
    fn parses_and() {
        let expr = parse("tag:a & tag:b").unwrap();
        assert!(expr.eval(&["a".to_string(), "b".to_string()]));
        assert!(!expr.eval(&["a".to_string()]));
    }

    #[test]
    fn parses_or() {
        let expr = parse("tag:a | tag:b").unwrap();
        assert!(expr.eval(&["a".to_string()]));
        assert!(expr.eval(&["b".to_string()]));
        assert!(!expr.eval(&["c".to_string()]));
    }

    #[test]
    fn parses_not() {
        let expr = parse("!tag:done").unwrap();
        assert!(expr.eval(&["other".to_string()]));
        assert!(!expr.eval(&["done".to_string()]));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // a & b | c  ==  (a & b) | c
        let expr = parse("tag:a & tag:b | tag:c").unwrap();
        // Only c present: (a&b) is false, so result depends on c being true
        assert!(expr.eval(&["c".to_string()]));
        // Only a present: (a&b) false, c absent -> false
        assert!(!expr.eval(&["a".to_string()]));
        // a and b present, c absent: (a&b) true -> true
        assert!(expr.eval(&["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn parens_override_precedence() {
        // a & (b | c)
        let expr = parse("tag:a & (tag:b | tag:c)").unwrap();
        assert!(expr.eval(&["a".to_string(), "c".to_string()]));
        assert!(!expr.eval(&["c".to_string()])); // a missing
    }

    #[test]
    fn tag_values_are_lowercased_on_parse() {
        let expr = parse("tag:Project:HiveMind").unwrap();
        assert_eq!(expr, TagExpr::Tag("project:hivemind".to_string()));
    }

    #[test]
    fn unbalanced_paren_is_an_error() {
        assert!(parse("(tag:a & tag:b").is_err());
        assert!(parse("tag:a)").is_err());
    }

    #[test]
    fn bare_word_without_tag_prefix_is_an_error() {
        assert!(parse("tag:a & oops").is_err());
    }

    #[test]
    fn empty_tag_value_is_an_error() {
        assert!(parse("tag:").is_err());
    }

    #[test]
    fn and_all_builds_and_chain() {
        let expr = TagExpr::and_all(&["a".to_string(), "b".to_string(), "c".to_string()]).unwrap();
        assert!(expr.eval(&["a".to_string(), "b".to_string(), "c".to_string()]));
        assert!(!expr.eval(&["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn and_all_empty_returns_none() {
        assert!(TagExpr::and_all(&[]).is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tag_query`
Expected: FAIL to compile (`TagExpr`, `parse`, `looks_like_tag_expr` don't exist yet).

- [ ] **Step 3: Implement the module**

Add above the test module in `src/tag_query.rs`:

```rust
use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagExpr {
    Tag(String),
    And(Box<TagExpr>, Box<TagExpr>),
    Or(Box<TagExpr>, Box<TagExpr>),
    Not(Box<TagExpr>),
}

impl TagExpr {
    /// Tags are already lowercased both here (at parse time) and at storage
    /// time (src/store.rs), so direct equality is correct.
    pub fn eval(&self, tags: &[String]) -> bool {
        match self {
            TagExpr::Tag(t) => tags.iter().any(|x| x == t),
            TagExpr::And(a, b) => a.eval(tags) && b.eval(tags),
            TagExpr::Or(a, b) => a.eval(tags) || b.eval(tags),
            TagExpr::Not(a) => !a.eval(tags),
        }
    }

    /// Builds an AND-chain from a flat list of required tags — the AND-only
    /// special case used by memory_search's `tags` param (a plain JSON array
    /// has no way to express OR/NOT, so this is the only combinator it needs).
    pub fn and_all(tags: &[String]) -> Option<TagExpr> {
        let mut iter = tags.iter();
        let first = iter.next()?;
        let mut expr = TagExpr::Tag(first.to_lowercase());
        for t in iter {
            expr = TagExpr::And(Box::new(expr), Box::new(TagExpr::Tag(t.to_lowercase())));
        }
        Some(expr)
    }
}

/// True if `s` looks like an attempted tag expression. Callers use this to
/// decide whether to call `parse` or fall back to normal title/FTS
/// resolution — see the detection rule in the design spec.
pub fn looks_like_tag_expr(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("tag:") || t.starts_with("!tag:") || t.starts_with('(')
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    And,
    Or,
    Not,
    LParen,
    RParen,
    Tag(String),
}

pub fn parse(s: &str) -> Result<TagExpr> {
    let tokens = tokenize(s)?;
    let mut pos = 0;
    let expr = parse_or(&tokens, &mut pos)?;
    if pos != tokens.len() {
        bail!("unexpected trailing input in tag expression: {s:?}");
    }
    Ok(expr)
}

fn tokenize(s: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '&' => {
                chars.next();
                tokens.push(Token::And);
            }
            '|' => {
                chars.next();
                tokens.push(Token::Or);
            }
            '!' => {
                chars.next();
                tokens.push(Token::Not);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || "&|!()".contains(c) {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                match word.strip_prefix("tag:") {
                    Some(value) if !value.is_empty() => {
                        tokens.push(Token::Tag(value.to_lowercase()));
                    }
                    Some(_) => bail!("empty tag value in tag expression: {s:?}"),
                    None => bail!("expected 'tag:' atom, found {word:?} in tag expression: {s:?}"),
                }
            }
        }
    }
    Ok(tokens)
}

fn parse_or(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    let mut expr = parse_and(tokens, pos)?;
    while matches!(tokens.get(*pos), Some(Token::Or)) {
        *pos += 1;
        let rhs = parse_and(tokens, pos)?;
        expr = TagExpr::Or(Box::new(expr), Box::new(rhs));
    }
    Ok(expr)
}

fn parse_and(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    let mut expr = parse_not(tokens, pos)?;
    while matches!(tokens.get(*pos), Some(Token::And)) {
        *pos += 1;
        let rhs = parse_not(tokens, pos)?;
        expr = TagExpr::And(Box::new(expr), Box::new(rhs));
    }
    Ok(expr)
}

fn parse_not(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    if matches!(tokens.get(*pos), Some(Token::Not)) {
        *pos += 1;
        let inner = parse_not(tokens, pos)?;
        return Ok(TagExpr::Not(Box::new(inner)));
    }
    parse_atom(tokens, pos)
}

fn parse_atom(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    match tokens.get(*pos) {
        Some(Token::Tag(t)) => {
            *pos += 1;
            Ok(TagExpr::Tag(t.clone()))
        }
        Some(Token::LParen) => {
            *pos += 1;
            let expr = parse_or(tokens, pos)?;
            match tokens.get(*pos) {
                Some(Token::RParen) => {
                    *pos += 1;
                    Ok(expr)
                }
                _ => bail!("missing closing paren in tag expression"),
            }
        }
        other => bail!("expected tag atom or '(', found {other:?}"),
    }
}
```

- [ ] **Step 4: Register the module**

In `src/lib.rs`, find:

```rust
pub mod store;
pub mod sync;
```

Change it to:

```rust
pub mod store;
pub mod sync;
pub mod tag_query;
```

(Keep this alphabetically-ish placed — check the surrounding lines in the actual file and slot it in sensibly; the exact surrounding context may differ slightly from this snippet since other modules may have been added since this plan was written.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tag_query`
Expected: all tests PASS (13 new tests).

- [ ] **Step 6: Run the full test suite to check for regressions**

Run: `cargo test --lib`
Expected: all pass (183 previous + 13 new = 196).

- [ ] **Step 7: Commit**

```bash
git add src/tag_query.rs src/lib.rs
git commit -m "feat: add boolean tag expression parser (AND/OR/NOT, parens)"
```

---

### Task 2: `SqliteStore::find_by_tag_expr`

**Files:**
- Modify: `src/store.rs`

**Interfaces:**
- Consumes: `crate::tag_query::TagExpr` (Task 1).
- Produces: `SqliteStore::find_by_tag_expr(&self, expr: &crate::tag_query::TagExpr) -> anyhow::Result<Vec<MemoryEntry>>`. Tasks 3 and 4 both call this exact method.

- [ ] **Step 1: Write the failing test**

In `src/store.rs`, find the test module's `list_memories_returns_all_stored` test and add this test immediately after it:

```rust
    #[tokio::test]
    async fn find_by_tag_expr_returns_matching_memories() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_rust",
            "Rust notes",
            "content",
            &["lang:rust".into(), "project:hivemind".into()],
        ))
        .await
        .unwrap();
        s.store(&test_row(
            "mem_vue",
            "Vue notes",
            "content",
            &["lang:vue".into(), "project:hivemind".into()],
        ))
        .await
        .unwrap();
        s.store(&test_row(
            "mem_other",
            "Unrelated",
            "content",
            &["project:oxhive".into()],
        ))
        .await
        .unwrap();

        let expr = crate::tag_query::parse("tag:project:hivemind & tag:lang:rust").unwrap();
        let results = s.find_by_tag_expr(&expr).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust notes");

        let expr_or = crate::tag_query::parse("tag:lang:rust | tag:lang:vue").unwrap();
        let mut results_or = s.find_by_tag_expr(&expr_or).await.unwrap();
        results_or.sort_by(|a, b| a.title.cmp(&b.title));
        assert_eq!(results_or.len(), 2);
        assert_eq!(results_or[0].title, "Rust notes");
        assert_eq!(results_or[1].title, "Vue notes");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib find_by_tag_expr_returns_matching_memories`
Expected: FAIL to compile (`find_by_tag_expr` doesn't exist yet).

- [ ] **Step 3: Implement the method**

In `src/store.rs`, find `list_memories` (the method ending near line 328):

```rust
    pub async fn list_memories(&self, limit: i64, offset: i64) -> Result<Vec<MemoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, content, created_at, updated_at, token_count, layer, memory_type
                 FROM memories ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2",
                params![limit, offset],
            )
            .await?;
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            let entry = self.row_to_entry(&row)?;
            let tags = self.fetch_tags(&entry.id).await?;
            results.push(MemoryEntry { tags, ..entry });
        }
        Ok(results)
    }
```

Add a new method immediately after it:

```rust
    /// Evaluates a tag boolean expression against every stored memory. Reuses
    /// `list_memories`'s per-row tag fetch (same N+1 pattern already used by
    /// `list_memories`/`search`) rather than a bulk-query optimization — fine
    /// at this tool's realistic memory counts (see `export()`'s identical
    /// `list_memories(100_000, 0)` full-table convention).
    pub async fn find_by_tag_expr(&self, expr: &crate::tag_query::TagExpr) -> Result<Vec<MemoryEntry>> {
        let all = self.list_memories(100_000, 0).await?;
        Ok(all.into_iter().filter(|e| expr.eval(&e.tags)).collect())
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib find_by_tag_expr_returns_matching_memories`
Expected: PASS.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test --lib`
Expected: all pass (196 previous + 1 new = 197).

- [ ] **Step 6: Commit**

```bash
git add src/store.rs
git commit -m "feat: add SqliteStore::find_by_tag_expr for boolean tag queries"
```

---

### Task 3: `.hivemind.toml` recalls — tag-expression support in `execute_session_start`

**Files:**
- Modify: `src/session.rs`

**Interfaces:**
- Consumes: `crate::tag_query::{looks_like_tag_expr, parse}` (Task 1), `SqliteStore::find_by_tag_expr` (Task 2).
- Produces: no new public API — `execute_session_start`'s existing signature and `SessionStartResult` shape are unchanged; only its internal per-recall resolution logic changes.

- [ ] **Step 1: Write the failing tests**

In `src/session.rs`, find the `skips_over_budget_but_continues_to_smaller_entries` test (the last test in the file) and add these two tests immediately after it:

```rust
    #[tokio::test]
    async fn tag_expr_recall_loads_all_matching_memories() {
        let (s, _dir) = store_with(&[
            (
                "id_rust",
                "rust notes",
                "short",
                vec!["lang:rust".to_string(), "project:hivemind".to_string()],
            ),
            (
                "id_vue",
                "vue notes",
                "short",
                vec!["lang:vue".to_string(), "project:hivemind".to_string()],
            ),
            (
                "id_other",
                "other project",
                "short",
                vec!["project:oxhive".to_string()],
            ),
        ])
        .await;
        let r = execute_session_start(&config(2000, vec!["tag:project:hivemind"]), &s)
            .await
            .unwrap();
        assert_eq!(r.loaded.len(), 2, "both hivemind-tagged memories should load");
        assert!(r.skipped.is_empty());
        let mut titles: Vec<_> = r.loaded.iter().map(|l| l.entry.title.clone()).collect();
        titles.sort();
        assert_eq!(titles, vec!["rust notes", "vue notes"]);
    }

    #[tokio::test]
    async fn malformed_tag_expr_recall_is_skipped_not_found_not_fts_searched() {
        let (s, _dir) = store_with(&[(
            "id_a",
            "tag:project:hivemind",
            "a memory whose title happens to look like a tag expression",
            vec![],
        )])
        .await;
        // "tag:" with no value is a parse error, not a valid expression —
        // must NOT fall back to treating it as a literal FTS/title query,
        // even though a memory with that literal title exists.
        let r = execute_session_start(&config(2000, vec!["tag:"]), &s)
            .await
            .unwrap();
        assert_eq!(r.loaded.len(), 0);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::NotFound);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tag_expr_recall_loads_all_matching_memories malformed_tag_expr_recall_is_skipped_not_found_not_fts_searched`
Expected: both FAIL — the first because only one entry loads today (current code takes `entries.into_iter().next().unwrap()` regardless of match count), the second either fails to compile-check the exact skip reason or currently would behave differently (verify what actually happens today by reading the existing code path — the point is these tests must fail before Step 3's change, confirming they exercise real new behavior).

- [ ] **Step 3: Implement the restructured recall loop**

In `src/session.rs`, find the full `execute_session_start` function body:

```rust
pub async fn execute_session_start(
    config: &HiveMindConfig,
    store: &SqliteStore,
) -> Result<SessionStartResult> {
    let max_tokens = config.max_tokens;
    let mut used_tokens = 0usize;
    let mut loaded = Vec::new();
    let mut skipped = Vec::new();

    for recall in &config.recalls {
        let entries = match store.resolve_recall(&recall.query).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(
                    "recall \"{}\" failed: {e:#}; treating as not found",
                    recall.query
                );
                skipped.push(SkippedEntry {
                    query: recall.query.clone(),
                    reason: SkipReason::NotFound,
                });
                continue;
            }
        };
        if entries.is_empty() {
            skipped.push(SkippedEntry {
                query: recall.query.clone(),
                reason: SkipReason::NotFound,
            });
        } else {
            // Use the first matching entry
            let entry = entries.into_iter().next().unwrap();
            let tokens = count_entry_tokens(&entry.title, &entry.content);
            if used_tokens + tokens > max_tokens {
                skipped.push(SkippedEntry {
                    query: recall.query.clone(),
                    reason: SkipReason::BudgetExceeded,
                });
                continue;
            }
            used_tokens += tokens;
            loaded.push(LoadedEntry {
                entry,
                tokens,
                source: recall.source,
            });
        }
    }

    let memories_recalled = loaded.len();
    Ok(SessionStartResult {
        project: config.project_name.clone(),
        loaded,
        skipped,
        used_tokens,
        max_tokens,
        memories_recalled,
    })
}
```

Replace it with:

```rust
pub async fn execute_session_start(
    config: &HiveMindConfig,
    store: &SqliteStore,
) -> Result<SessionStartResult> {
    let max_tokens = config.max_tokens;
    let mut used_tokens = 0usize;
    let mut loaded = Vec::new();
    let mut skipped = Vec::new();

    for recall in &config.recalls {
        let is_tag_expr = crate::tag_query::looks_like_tag_expr(&recall.query);

        let entries_result = if is_tag_expr {
            match crate::tag_query::parse(&recall.query) {
                Ok(expr) => store.find_by_tag_expr(&expr).await,
                Err(e) => Err(e),
            }
        } else {
            store.resolve_recall(&recall.query).await
        };

        let entries = match entries_result {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(
                    "recall \"{}\" failed: {e:#}; treating as not found",
                    recall.query
                );
                skipped.push(SkippedEntry {
                    query: recall.query.clone(),
                    reason: SkipReason::NotFound,
                });
                continue;
            }
        };

        if entries.is_empty() {
            skipped.push(SkippedEntry {
                query: recall.query.clone(),
                reason: SkipReason::NotFound,
            });
            continue;
        }

        // A tag expression can match many memories; a plain title/FTS recall
        // still resolves to at most its single best match.
        let candidates: Vec<_> = if is_tag_expr {
            entries
        } else {
            entries.into_iter().take(1).collect()
        };

        let mut any_loaded = false;
        for entry in candidates {
            let tokens = count_entry_tokens(&entry.title, &entry.content);
            if used_tokens + tokens > max_tokens {
                continue;
            }
            used_tokens += tokens;
            loaded.push(LoadedEntry {
                entry,
                tokens,
                source: recall.source,
            });
            any_loaded = true;
        }
        if !any_loaded {
            skipped.push(SkippedEntry {
                query: recall.query.clone(),
                reason: SkipReason::BudgetExceeded,
            });
        }
    }

    let memories_recalled = loaded.len();
    Ok(SessionStartResult {
        project: config.project_name.clone(),
        loaded,
        skipped,
        used_tokens,
        max_tokens,
        memories_recalled,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tag_expr_recall_loads_all_matching_memories malformed_tag_expr_recall_is_skipped_not_found_not_fts_searched`
Expected: both PASS.

- [ ] **Step 5: Run the full test suite to check for regressions**

Run: `cargo test --lib`
Expected: all pass, including the 4 pre-existing tests in this file (`loads_all_recalls_within_budget`, `records_not_found_recalls`, `to_json_matches_mcp_shape`, `skips_over_budget_but_continues_to_smaller_entries`) — these must still pass unchanged, confirming the restructuring preserved single-entry-recall behavior exactly. Total: 197 previous + 2 new = 199.

- [ ] **Step 6: Commit**

```bash
git add src/session.rs
git commit -m "feat: support boolean tag expressions in .hivemind.toml session-start recalls"
```

---

### Task 4: `memory_search` MCP tool — `tags` parameter

**Files:**
- Modify: `src/server.rs`

**Interfaces:**
- Consumes: `crate::tag_query::TagExpr` (Task 1), `SqliteStore::find_by_tag_expr` (Task 2).
- Produces: `MemorySearchInput` gains a `tags: Option<Vec<String>>` field; `query` changes from `String` to `Option<String>` (backward compatible — existing callers that always pass `query` are unaffected, since a present value still deserializes into `Some(...)`).

- [ ] **Step 1: Write the failing tests**

In `src/server.rs`, find the `memory_search_empty_query_returns_zero` test and add these two tests immediately after it:

```rust
    #[tokio::test]
    async fn memory_search_by_tags_only() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "rust preferences".to_string(),
            content: "use anyhow for errors".to_string(),
            tags: vec!["lang:rust".to_string(), "project:hivemind".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "vue preferences".to_string(),
            content: "use pinia for state".to_string(),
            tags: vec!["lang:vue".to_string(), "project:hivemind".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        let result = hm
            .do_memory_search(MemorySearchInput {
                query: None,
                tags: Some(vec!["lang:rust".to_string()]),
                limit: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "rust preferences");
    }

    #[tokio::test]
    async fn memory_search_query_and_tags_combined() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "rust error handling".to_string(),
            content: "use anyhow for errors".to_string(),
            tags: vec!["lang:rust".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "vue error handling".to_string(),
            content: "use error boundaries".to_string(),
            tags: vec!["lang:vue".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        // "error handling" FTS-matches both, but only the rust one carries the tag.
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("error handling".to_string()),
                tags: Some(vec!["lang:rust".to_string()]),
                limit: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "rust error handling");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib memory_search_by_tags_only memory_search_query_and_tags_combined`
Expected: FAIL to compile (`MemorySearchInput` has no `tags` field yet, and `query` isn't `Option<String>` yet).

- [ ] **Step 3: Update `MemorySearchInput` and the two existing test call sites**

Find:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemorySearchInput {
    /// Keywords to search memory titles and content
    pub query: String,
    /// Max results (default 5, capped at 10)
    #[serde(default)]
    pub limit: Option<i64>,
}
```

Change it to:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemorySearchInput {
    /// Keywords to search memory titles and content. Optional if `tags` is
    /// provided — a pure tag-boolean search with no keyword component.
    #[serde(default)]
    pub query: Option<String>,
    /// Require all of these tags (namespace:value form, e.g. "lang:rust").
    /// ANDed together, and ANDed with `query` if both are provided.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Max results (default 5, capped at 10)
    #[serde(default)]
    pub limit: Option<i64>,
}
```

Find the existing `memory_search_finds...`-style test (the one asserting `"pgx"` in the snippet):

```rust
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: "pgx".to_string(),
                limit: None,
            })
            .await
            .unwrap();
```

Change it to:

```rust
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("pgx".to_string()),
                tags: None,
                limit: None,
            })
            .await
            .unwrap();
```

Find `memory_search_empty_query_returns_zero`:

```rust
    #[tokio::test]
    async fn memory_search_empty_query_returns_zero() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: "  ".to_string(),
                limit: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["count"], 0);
```

Change the `MemorySearchInput` construction:

```rust
    #[tokio::test]
    async fn memory_search_empty_query_returns_zero() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("  ".to_string()),
                tags: None,
                limit: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["count"], 0);
```

- [ ] **Step 4: Restructure `do_memory_search`**

Find:

```rust
    pub async fn do_memory_search(
        &self,
        p: MemorySearchInput,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = p.limit.unwrap_or(5).clamp(1, 10);
        let trimmed = p.query.trim();
        if trimmed.is_empty() {
            return Ok(CallToolResult::structured(json!({
                "count": 0,
                "results": [],
            })));
        }
        let hits = self
            .store
            .search(trimmed, limit)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let results: Vec<_> = hits
            .iter()
            .map(|h| {
                let snippet: String = h.content.chars().take(200).collect();
                json!({
                    "id": h.id,
                    "title": h.title,
                    "snippet": snippet,
                    "tags": h.tags,
                    "layer": h.layer,
                })
            })
            .collect();
        Ok(CallToolResult::structured(json!({
            "count": results.len(),
            "results": results,
        })))
    }
```

Replace it with:

```rust
    pub async fn do_memory_search(
        &self,
        p: MemorySearchInput,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = p.limit.unwrap_or(5).clamp(1, 10);
        let query = p.query.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let tags = p.tags.filter(|t| !t.is_empty());

        if query.is_none() && tags.is_none() {
            return Ok(CallToolResult::structured(json!({
                "count": 0,
                "results": [],
            })));
        }

        let hits = match (query, tags) {
            (Some(q), Some(tags)) => {
                let expr = crate::tag_query::TagExpr::and_all(&tags)
                    .expect("tags checked non-empty above");
                let candidates = self
                    .store
                    .search(q, 50)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let mut filtered: Vec<_> =
                    candidates.into_iter().filter(|e| expr.eval(&e.tags)).collect();
                filtered.truncate(limit as usize);
                filtered
            }
            (Some(q), None) => self
                .store
                .search(q, limit)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
            (None, Some(tags)) => {
                let expr = crate::tag_query::TagExpr::and_all(&tags)
                    .expect("tags checked non-empty above");
                let mut results = self
                    .store
                    .find_by_tag_expr(&expr)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                results.truncate(limit as usize);
                results
            }
            (None, None) => unreachable!("handled by the early return above"),
        };

        let results: Vec<_> = hits
            .iter()
            .map(|h| {
                let snippet: String = h.content.chars().take(200).collect();
                json!({
                    "id": h.id,
                    "title": h.title,
                    "snippet": snippet,
                    "tags": h.tags,
                    "layer": h.layer,
                })
            })
            .collect();
        Ok(CallToolResult::structured(json!({
            "count": results.len(),
            "results": results,
        })))
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib memory_search_by_tags_only memory_search_query_and_tags_combined memory_search_empty_query_returns_zero`
Expected: all PASS. Also re-run the pre-existing "pgx" search test by name (check its exact name in the file) to confirm it still passes after the `Some(...)` wrapping change.

- [ ] **Step 6: Run the full test suite AND the integration suite**

Run: `cargo test --lib && cargo test --test api_integration`
Expected: all pass. Lib count: 199 previous + 2 new = 201. Integration: 14 unaffected (this task doesn't touch `src/api.rs`).

- [ ] **Step 7: Commit**

```bash
git add src/server.rs
git commit -m "feat: add tags param to memory_search MCP tool (AND-only boolean tag filter)"
```

---

### Task 5: Documentation — `README.md`

**Files:**
- Modify: `README.md`

**Interfaces:**
- None (documentation only).

- [ ] **Step 1: Extend the `.hivemind.toml` recalls section**

In `README.md`, find:

```
`recalls` is a list of memory titles to auto-inject at session start. Each entry is looked up by exact title, then falls back to FTS. The combined size is capped at `max_tokens`.
```

Change it to:

```
`recalls` is a list of memory titles to auto-inject at session start. Each entry is looked up by exact title, then falls back to FTS. The combined size is capped at `max_tokens`.

A recall entry can also be a boolean tag expression instead of a title — use `&` (AND), `|` (OR), `!` (NOT), and parens for grouping, with each tag written as `tag:<namespace:value>`:

```toml
recalls = [
  "tag:project:hivemind & tag:lang:rust",
  "tag:project:hivemind & !tag:status:done",
  "my exact memory title",
]
```

Unlike a plain title recall (which loads at most one memory), a tag expression loads **every** matching memory, still subject to the overall `max_tokens` budget. An entry is only parsed as a tag expression if it starts with `tag:`, `!tag:`, or `(` — anything else is treated as a plain title/FTS query exactly as before.
```

- [ ] **Step 2: Note the `memory_search` `tags` param**

Find:

```
- **By keyword**: ask Claude: *"search my memories for postgres"* → Claude calls `memory_search` (FTS, returns snippets)
```

Change it to:

```
- **By keyword**: ask Claude: *"search my memories for postgres"* → Claude calls `memory_search` (FTS, returns snippets)
- **By tag**: ask Claude: *"find memories tagged lang:rust and project:hivemind"* → Claude calls `memory_search` with a `tags` array (AND-only; combine with a keyword `query` too if you like)
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document boolean tag expressions in recalls and memory_search's tags param"
```
