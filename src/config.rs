use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::Deserialize;

pub const DEFAULT_MAX_TOKENS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallSource {
    Project,
    Local,
}

#[derive(Debug, Clone)]
pub struct Recall {
    pub query: String,
    pub source: RecallSource,
}

#[derive(Debug, Clone)]
pub struct HiveMindConfig {
    pub project_name: String,
    pub max_tokens: usize,
    pub recalls: Vec<Recall>,
    pub condition_paths: Vec<String>,
    pub file_open_rule_count: usize,
    pub mention_trigger_count: usize,
}

#[derive(Debug, Default, Deserialize)]
struct RawProject {
    #[serde(default)]
    project: RawProjectMeta,
    #[serde(default)]
    hooks: RawHooks,
}

#[derive(Debug, Default, Deserialize)]
struct RawProjectMeta {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawHooks {
    #[serde(default)]
    on_session_start: RawSessionStart,
    #[serde(default)]
    on_file_open: RawFileOpen,
    #[serde(default)]
    on_mention: RawMention,
}

#[derive(Debug, Default, Deserialize)]
struct RawSessionStart {
    max_tokens: Option<usize>,
    #[serde(default)]
    recalls: Vec<String>,
    #[serde(default)]
    conditions: RawConditions,
}

#[derive(Debug, Default, Deserialize)]
struct RawConditions {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawFileOpen {
    #[serde(default)]
    rules: Vec<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMention {
    #[serde(default)]
    triggers: Vec<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct RawLocal {
    #[serde(default)]
    hooks: RawLocalHooks,
}

#[derive(Debug, Default, Deserialize)]
struct RawLocalHooks {
    #[serde(default)]
    on_session_start: RawLocalSessionStart,
}

#[derive(Debug, Default, Deserialize)]
struct RawLocalSessionStart {
    max_tokens: Option<usize>,
    #[serde(default)]
    recalls: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawGlobal {
    #[serde(default)]
    defaults: RawDefaults,
}

#[derive(Debug, Default, Deserialize)]
struct RawDefaults {
    max_inject_tokens: Option<usize>,
}

pub fn discover_project_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir: &Path = &start;
    loop {
        if dir.join(".hivemind.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

pub fn global_config_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("hivemind");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("hivemind")
}

pub fn global_config_path() -> PathBuf {
    global_config_dir().join("config.toml")
}

pub fn load_config(project_path: &Path) -> Result<HiveMindConfig> {
    let root = discover_project_root(project_path).ok_or_else(|| {
        anyhow::anyhow!("no .hivemind.toml found at or above {}", project_path.display())
    })?;
    load_config_with_global(&root, &global_config_path())
}

pub fn load_config_with_global(project_root: &Path, global_path: &Path) -> Result<HiveMindConfig> {
    let global_default = if global_path.is_file() {
        let raw: RawGlobal = toml::from_str(&std::fs::read_to_string(global_path)?)
            .with_context(|| format!("parsing {}", global_path.display()))?;
        raw.defaults.max_inject_tokens
    } else {
        None
    };

    let project_file = project_root.join(".hivemind.toml");
    let raw_project: RawProject = toml::from_str(
        &std::fs::read_to_string(&project_file)
            .with_context(|| format!("reading {}", project_file.display()))?,
    )
    .with_context(|| format!("parsing {}", project_file.display()))?;

    let base_max = raw_project
        .hooks
        .on_session_start
        .max_tokens
        .or(global_default)
        .unwrap_or(DEFAULT_MAX_TOKENS);

    let mut recalls: Vec<Recall> = raw_project
        .hooks
        .on_session_start
        .recalls
        .iter()
        .map(|q| Recall { query: q.clone(), source: RecallSource::Project })
        .collect();

    let local_file = project_root.join(".hivemind.local.toml");
    let mut max_tokens = base_max;
    if local_file.is_file() {
        let raw_local: RawLocal = toml::from_str(&std::fs::read_to_string(&local_file)?)
            .with_context(|| format!("parsing {}", local_file.display()))?;
        max_tokens += raw_local.hooks.on_session_start.max_tokens.unwrap_or(0);
        for q in &raw_local.hooks.on_session_start.recalls {
            recalls.push(Recall { query: q.clone(), source: RecallSource::Local });
        }
    }

    let project_name = if raw_project.project.name.is_empty() {
        project_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string())
    } else {
        raw_project.project.name
    };

    Ok(HiveMindConfig {
        project_name,
        max_tokens,
        recalls,
        condition_paths: raw_project.hooks.on_session_start.conditions.paths,
        file_open_rule_count: raw_project.hooks.on_file_open.rules.len(),
        mention_trigger_count: raw_project.hooks.on_mention.triggers.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn discover_walks_up_to_find_project_config() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, ".hivemind.toml", "[project]\nname=\"x\"\n");
        let nested = root.join("internal").join("svc");
        fs::create_dir_all(&nested).unwrap();
        let found = discover_project_root(&nested).unwrap();
        assert_eq!(found, root.canonicalize().unwrap());
    }

    #[test]
    fn discover_returns_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover_project_root(tmp.path()).is_none());
    }

    #[test]
    fn load_uses_project_name_recalls_and_max_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".hivemind.toml",
            "[project]\nname=\"oxhive-api\"\n[hooks.on_session_start]\nmax_tokens=1500\nrecalls=[\"a\",\"b\"]\n");
        let missing_global = tmp.path().join("no-global.toml");
        let cfg = load_config_with_global(tmp.path(), &missing_global).unwrap();
        assert_eq!(cfg.project_name, "oxhive-api");
        assert_eq!(cfg.max_tokens, 1500);
        assert_eq!(cfg.recalls.len(), 2);
        assert_eq!(cfg.recalls[0].query, "a");
        assert!(matches!(cfg.recalls[0].source, RecallSource::Project));
    }

    #[test]
    fn local_config_is_additive() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".hivemind.toml",
            "[project]\nname=\"p\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"team\"]\n");
        write(tmp.path(), ".hivemind.local.toml",
            "[hooks.on_session_start]\nmax_tokens=500\nrecalls=[\"mine\"]\n");
        let missing_global = tmp.path().join("no-global.toml");
        let cfg = load_config_with_global(tmp.path(), &missing_global).unwrap();
        assert_eq!(cfg.max_tokens, 2500, "local max_tokens adds to team budget");
        assert_eq!(cfg.recalls.len(), 2);
        assert_eq!(cfg.recalls[1].query, "mine");
        assert!(matches!(cfg.recalls[1].source, RecallSource::Local));
    }

    #[test]
    fn default_max_tokens_is_2000_when_unset() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".hivemind.toml", "[project]\nname=\"p\"\n");
        let missing_global = tmp.path().join("no-global.toml");
        let cfg = load_config_with_global(tmp.path(), &missing_global).unwrap();
        assert_eq!(cfg.max_tokens, 2000);
        assert_eq!(cfg.recalls.len(), 0);
    }

    #[test]
    fn counts_file_open_and_mention_rules() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), ".hivemind.toml",
            "[project]\nname=\"p\"\n\
             [hooks.on_file_open]\nrules=[{pattern=\"*.go\",recall=\"x\"},{pattern=\"*.rs\",recall=\"y\"}]\n\
             [hooks.on_mention]\ntriggers=[{keyword=\"@db\",recall=\"z\"}]\n");
        let missing_global = tmp.path().join("no-global.toml");
        let cfg = load_config_with_global(tmp.path(), &missing_global).unwrap();
        assert_eq!(cfg.file_open_rule_count, 2);
        assert_eq!(cfg.mention_trigger_count, 1);
    }
}
