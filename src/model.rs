use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Personal,
    Workspace,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Layer::Personal => write!(f, "personal"),
            Layer::Workspace => write!(f, "workspace"),
        }
    }
}

impl std::str::FromStr for Layer {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "personal" => Ok(Layer::Personal),
            "workspace" => Ok(Layer::Workspace),
            _ => Err(anyhow::anyhow!("invalid layer: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Preference,
    Project,
    History,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::Project => write!(f, "project"),
            MemoryType::History => write!(f, "history"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preference" => Ok(MemoryType::Preference),
            "project" => Ok(MemoryType::Project),
            "history" => Ok(MemoryType::History),
            _ => Err(anyhow::anyhow!("invalid memory type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub layer: Layer,
    pub memory_type: MemoryType,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub title: String,
    pub content: String,
    pub layer: Layer,
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    pub project: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreResult {
    pub id: String,
    pub auto_connected: usize,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub layer: Layer,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateMemory {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub merge_content: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship: String,
    pub weight: f64,
    pub inferred_by: String,
    pub status: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackItem {
    pub id: String,
    pub memory_id: Option<String>,
    pub edge_id: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub note: Option<String>,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictItem {
    pub id: String,
    pub memory_id: Option<String>,
    pub winner: String,
    pub loser: String,
    pub winner_src: String,
    pub loser_src: String,
    pub detected_at: i64,
    pub status: String,
}

/// Result of attempting to create an edge.
#[derive(Debug, PartialEq, Eq)]
pub enum EdgeCreate {
    Created(String),
    Duplicate,
    MissingEndpoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_roundtrips_through_string() {
        let l: Layer = "personal".parse().unwrap();
        assert_eq!(l.to_string(), "personal");

        let l: Layer = "workspace".parse().unwrap();
        assert_eq!(l.to_string(), "workspace");
    }

    #[test]
    fn layer_rejects_invalid_value() {
        assert!("unknown".parse::<Layer>().is_err());
    }

    #[test]
    fn memory_type_roundtrips() {
        assert_eq!("preference".parse::<MemoryType>().unwrap().to_string(), "preference");
        assert_eq!("project".parse::<MemoryType>().unwrap().to_string(), "project");
        assert_eq!("history".parse::<MemoryType>().unwrap().to_string(), "history");
    }
}
