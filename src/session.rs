use crate::budget::count_entry_tokens;
use crate::config::{HiveMindConfig, RecallSource};
use crate::store::{MemoryEntry, SqliteStore};
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
    pub memories_recalled: usize,
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
/// smaller entry may still fit. Recalls are resolved title -> FTS.
pub async fn execute_session_start(
    config: &HiveMindConfig,
    store: &SqliteStore,
) -> Result<SessionStartResult> {
    let max_tokens = config.max_tokens;
    let mut used_tokens = 0usize;
    let mut loaded = Vec::new();
    let mut skipped = Vec::new();

    for recall in &config.recalls {
        let entries = store.resolve_recall(&recall.query).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HiveMindConfig, Recall, RecallSource};
    use crate::{db, store::SqliteStore};
    use tempfile::TempDir;

    async fn store_with(memories: &[(&str, &str, &str, Vec<String>)]) -> (SqliteStore, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = crate::config::SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);
        for (id, title, content, tags) in memories {
            store.store(id, title, content, tags, None).await.unwrap();
        }
        (store, dir)
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
            memories_recalled: 0,
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
            memories_recalled: 0,
        };
        assert_eq!(
            result_with_skip.remaining(),
            0,
            "saturating_sub prevents underflow"
        );
        assert!(result_with_skip.truncated());
    }

    #[tokio::test]
    async fn loads_all_recalls_within_budget() {
        let (s, _dir) = store_with(&[
            ("id_a", "pref a", "short content a", vec![]),
            ("id_b", "pref b", "short content b", vec![]),
        ])
        .await;
        let r = execute_session_start(&config(2000, vec!["pref a", "pref b"]), &s)
            .await
            .unwrap();
        assert_eq!(r.loaded.len(), 2);
        assert!(r.skipped.is_empty());
        assert!(!r.truncated());
        assert_eq!(r.project, "test-proj");
        assert!(r.used_tokens > 0);
    }

    #[tokio::test]
    async fn records_not_found_recalls() {
        let (s, _dir) = store_with(&[("id_a", "pref a", "content a", vec![])]).await;
        let r = execute_session_start(&config(2000, vec!["pref a", "does not exist"]), &s)
            .await
            .unwrap();
        assert_eq!(r.loaded.len(), 1);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::NotFound);
        assert!(r.truncated());
    }

    #[tokio::test]
    async fn skips_over_budget_but_continues_to_smaller_entries() {
        let big = "word ".repeat(400);
        let (s, _dir) = store_with(&[
            ("id_big", "big", &big, vec![]),
            ("id_small", "small", "tiny", vec![]),
        ])
        .await;
        let small_cost = crate::budget::count_entry_tokens("small", "tiny");
        let r = execute_session_start(&config(small_cost + 5, vec!["big", "small"]), &s)
            .await
            .unwrap();
        assert_eq!(r.loaded.len(), 1, "only the small entry fits");
        assert_eq!(r.loaded[0].entry.title, "small");
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::BudgetExceeded);
        assert_eq!(r.skipped[0].query, "big");
    }
}
