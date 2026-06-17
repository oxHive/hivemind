use crate::budget::count_entry_tokens;
use crate::config::{HiveMindConfig, RecallSource};
use crate::model::MemoryEntry;
use crate::store::SqliteStore;
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NotFound,
    BudgetExceeded,
}

impl SkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkipReason::NotFound => "not_found",
            SkipReason::BudgetExceeded => "budget_exceeded",
        }
    }
}

#[derive(Debug)]
pub struct LoadedEntry {
    pub entry: MemoryEntry,
    pub tokens: usize,
    pub source: RecallSource,
}

#[derive(Debug)]
pub struct SkippedEntry {
    pub query: String,
    pub reason: SkipReason,
}

#[derive(Debug)]
pub struct SessionStartResult {
    pub project: String,
    pub loaded: Vec<LoadedEntry>,
    pub skipped: Vec<SkippedEntry>,
    pub used_tokens: usize,
    pub max_tokens: usize,
}

impl SessionStartResult {
    pub fn truncated(&self) -> bool {
        !self.skipped.is_empty()
    }
    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens)
    }
}

/// Run the configured session-start recalls under the token budget.
/// On over-budget, the entry is skipped and the loop CONTINUES — a later,
/// smaller entry may still fit. Recalls are resolved id -> title -> FTS.
pub fn execute_session_start(
    config: &HiveMindConfig,
    store: &SqliteStore,
) -> Result<SessionStartResult> {
    let max_tokens = config.max_tokens;
    let mut used_tokens = 0usize;
    let mut loaded = Vec::new();
    let mut skipped = Vec::new();

    for recall in &config.recalls {
        match store.resolve_recall(&recall.query)? {
            None => skipped.push(SkippedEntry {
                query: recall.query.clone(),
                reason: SkipReason::NotFound,
            }),
            Some(entry) => {
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
    }

    Ok(SessionStartResult {
        project: config.project_name.clone(),
        loaded,
        skipped,
        used_tokens,
        max_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HiveMindConfig, Recall, RecallSource};
    use crate::db;
    use crate::model::{Layer, MemoryType, NewMemory};
    use crate::store::SqliteStore;

    fn store_with(entries: &[(&str, &str)]) -> SqliteStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        let s = SqliteStore::new(conn);
        for (title, content) in entries {
            s.store(NewMemory {
                title: title.to_string(),
                content: content.to_string(),
                layer: Layer::Personal,
                memory_type: MemoryType::Preference,
                tags: vec![],
                project: None,
                source: None,
            })
            .unwrap();
        }
        s
    }

    fn config(max: usize, recalls: Vec<&str>) -> HiveMindConfig {
        HiveMindConfig {
            project_name: "test-proj".to_string(),
            max_tokens: max,
            recalls: recalls
                .into_iter()
                .map(|q| Recall {
                    query: q.to_string(),
                    source: RecallSource::Project,
                })
                .collect(),
            condition_paths: vec![],
            file_open_rule_count: 0,
            mention_trigger_count: 0,
        }
    }

    #[test]
    fn skip_reason_as_str_values() {
        assert_eq!(SkipReason::NotFound.as_str(), "not_found");
        assert_eq!(SkipReason::BudgetExceeded.as_str(), "budget_exceeded");
    }

    #[test]
    fn session_result_remaining_and_truncated() {
        let result = SessionStartResult {
            project: "p".to_string(),
            loaded: vec![],
            skipped: vec![],
            used_tokens: 300,
            max_tokens: 2000,
        };
        assert_eq!(result.remaining(), 1700);
        assert!(!result.truncated());

        let result_with_skip = SessionStartResult {
            project: "p".to_string(),
            loaded: vec![],
            skipped: vec![SkippedEntry {
                query: "q".to_string(),
                reason: SkipReason::NotFound,
            }],
            used_tokens: 2001,
            max_tokens: 2000,
        };
        assert_eq!(
            result_with_skip.remaining(),
            0,
            "saturating_sub prevents underflow"
        );
        assert!(result_with_skip.truncated());
    }

    #[test]
    fn loads_all_recalls_within_budget() {
        let s = store_with(&[("pref a", "short content a"), ("pref b", "short content b")]);
        let r = execute_session_start(&config(2000, vec!["pref a", "pref b"]), &s).unwrap();
        assert_eq!(r.loaded.len(), 2);
        assert!(r.skipped.is_empty());
        assert!(!r.truncated());
        assert_eq!(r.project, "test-proj");
        assert!(r.used_tokens > 0);
    }

    #[test]
    fn records_not_found_recalls() {
        let s = store_with(&[("pref a", "content a")]);
        let r = execute_session_start(&config(2000, vec!["pref a", "does not exist"]), &s).unwrap();
        assert_eq!(r.loaded.len(), 1);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::NotFound);
        assert!(r.truncated());
    }

    #[test]
    fn skips_over_budget_but_continues_to_smaller_entries() {
        let big = "word ".repeat(400);
        let s = store_with(&[("big", &big), ("small", "tiny")]);
        let small_cost = crate::budget::count_entry_tokens("small", "tiny");
        let r = execute_session_start(&config(small_cost + 5, vec!["big", "small"]), &s).unwrap();
        assert_eq!(r.loaded.len(), 1, "only the small entry fits");
        assert_eq!(r.loaded[0].entry.title, "small");
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::BudgetExceeded);
        assert_eq!(r.skipped[0].query, "big");
    }
}
