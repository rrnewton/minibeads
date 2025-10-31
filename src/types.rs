use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

/// Issue status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Blocked,
    Closed,
}

impl Status {
    /// Get the string representation of this status
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "blocked" => Ok(Status::Blocked),
            "closed" => Ok(Status::Closed),
            _ => Err(anyhow::anyhow!(
                "Invalid status: '{}'. Valid values are: open, in_progress, blocked, closed",
                s
            )),
        }
    }
}

/// Issue type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueType {
    Bug,
    Feature,
    Task,
    Epic,
    Chore,
}

impl IssueType {
    /// Get the string representation of this issue type
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueType::Bug => "bug",
            IssueType::Feature => "feature",
            IssueType::Task => "task",
            IssueType::Epic => "epic",
            IssueType::Chore => "chore",
        }
    }
}

impl std::fmt::Display for IssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for IssueType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bug" => Ok(IssueType::Bug),
            "feature" => Ok(IssueType::Feature),
            "task" => Ok(IssueType::Task),
            "epic" => Ok(IssueType::Epic),
            "chore" => Ok(IssueType::Chore),
            _ => Err(anyhow::anyhow!(
                "Invalid issue type: '{}'. Valid values are: bug, feature, task, epic, chore",
                s
            )),
        }
    }
}

/// Dependency type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyType {
    Blocks,
    Related,
    ParentChild,
    DiscoveredFrom,
}

impl DependencyType {
    /// Get the string representation of this dependency type
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyType::Blocks => "blocks",
            DependencyType::Related => "related",
            DependencyType::ParentChild => "parent-child",
            DependencyType::DiscoveredFrom => "discovered-from",
        }
    }
}

impl std::fmt::Display for DependencyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for DependencyType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "blocks" => Ok(DependencyType::Blocks),
            "related" => Ok(DependencyType::Related),
            "parent-child" => Ok(DependencyType::ParentChild),
            "discovered-from" => Ok(DependencyType::DiscoveredFrom),
            _ => Err(anyhow::anyhow!(
                "Invalid dependency type: '{}'. Valid values are: blocks, related, parent-child, discovered-from",
                s
            )),
        }
    }
}

/// Dependency representation for JSON output (MCP compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    #[serde(rename = "type")]
    pub dep_type: String,
}

/// Custom serialization for depends_on HashMap -> dependencies array
fn serialize_dependencies<S>(
    map: &HashMap<String, DependencyType>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let deps: Vec<Dependency> = map
        .iter()
        .map(|(id, dep_type)| Dependency {
            id: id.clone(),
            dep_type: dep_type.to_string(),
        })
        .collect();
    deps.serialize(serializer)
}

/// Helper enum for deserializing either old HashMap or new array format
#[derive(Deserialize)]
#[serde(untagged)]
enum DependenciesFormat {
    Array(Vec<Dependency>),
    Map(HashMap<String, DependencyType>),
}

/// Custom deserialization for dependencies array -> depends_on HashMap
fn deserialize_dependencies<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, DependencyType>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    match DependenciesFormat::deserialize(deserializer)? {
        DependenciesFormat::Array(deps) => {
            let mut map = HashMap::new();
            for dep in deps {
                let dep_type = dep.dep_type.parse::<DependencyType>().map_err(|_| {
                    Error::custom(format!("Invalid dependency type: {}", dep.dep_type))
                })?;
                map.insert(dep.id, dep_type);
            }
            Ok(map)
        }
        DependenciesFormat::Map(map) => Ok(map),
    }
}

/// Issue structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub title: String,
    pub description: String,
    pub design: String,
    pub notes: String,
    pub acceptance_criteria: String,
    pub status: Status,
    pub priority: i32,
    pub issue_type: IssueType,
    pub assignee: String,
    pub external_ref: Option<String>,
    pub labels: Vec<String>,
    #[serde(
        rename = "dependencies",
        serialize_with = "serialize_dependencies",
        deserialize_with = "deserialize_dependencies"
    )]
    pub depends_on: HashMap<String, DependencyType>,
    #[serde(default, skip_deserializing)]
    pub dependents: Vec<Dependency>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl Issue {
    pub fn new(id: String, title: String, priority: i32, issue_type: IssueType) -> Self {
        let now = Utc::now();
        Self {
            id,
            title,
            description: String::new(),
            design: String::new(),
            notes: String::new(),
            acceptance_criteria: String::new(),
            status: Status::Open,
            priority,
            issue_type,
            assignee: String::new(),
            external_ref: None,
            labels: Vec::new(),
            depends_on: HashMap::new(),
            dependents: Vec::new(),
            created_at: now,
            updated_at: now,
            closed_at: None,
        }
    }

    /// Get dependencies of a specific type
    /// Returns an iterator to avoid unnecessary allocations
    pub fn get_blocking_dependencies(&self) -> impl Iterator<Item = &String> + '_ {
        self.depends_on
            .iter()
            .filter(|(_, dep_type)| **dep_type == DependencyType::Blocks)
            .map(|(id, _)| id)
    }

    /// Check if there are any blocking dependencies (zero-cost check)
    pub fn has_blocking_dependencies(&self) -> bool {
        self.depends_on
            .values()
            .any(|dep_type| *dep_type == DependencyType::Blocks)
    }
}

/// Statistics structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Stats {
    pub total_issues: usize,
    pub open_issues: usize,
    pub in_progress_issues: usize,
    pub blocked_issues: usize,
    pub closed_issues: usize,
    pub ready_issues: usize,
    pub average_lead_time_hours: f64,
}

/// Blocked issue structure (for blocked command)
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockedIssue {
    #[serde(flatten)]
    pub issue: Issue,
    pub blocked_by: Vec<String>,
    pub blocked_by_count: usize,
}

/// Tree node for dependency tree visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: i32,
    pub dep_type: Option<String>,
    pub children: Vec<TreeNode>,
    pub is_cycle: bool,
    pub depth_exceeded: bool,
}
