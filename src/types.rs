use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

/// Default lifetime of a claim when no explicit duration is given (48 hours).
///
/// After this window elapses, the claim is considered stale and another worker
/// may reclaim the issue. This is the stale-recovery mechanism that upstream bd
/// lacks: a crashed or abandoned agent's claim does not pin an issue forever.
pub const DEFAULT_CLAIM_HOURS: i64 = 48;

/// How long a claim should be held, parsed from a compact duration string such
/// as `48h`, `2d`, or `90m`. A bare integer (e.g. `12`) is interpreted as hours.
///
/// We use a dedicated type rather than a bare integer so the unit is explicit at
/// every call site and the parsing/validation lives in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimDuration(pub Duration);

impl ClaimDuration {
    /// The default claim duration ([`DEFAULT_CLAIM_HOURS`] hours).
    pub fn default_duration() -> Self {
        ClaimDuration(Duration::hours(DEFAULT_CLAIM_HOURS))
    }
}

impl std::str::FromStr for ClaimDuration {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            anyhow::bail!("Empty claim duration");
        }

        // Split into the leading number and an optional unit suffix.
        let (num_part, unit) = match s.chars().last() {
            Some(c) if c.is_ascii_alphabetic() => (&s[..s.len() - 1], c.to_ascii_lowercase()),
            _ => (s, 'h'), // bare number => hours
        };

        let value: i64 = num_part.trim().parse().map_err(|_| {
            anyhow::anyhow!(
                "Invalid claim duration: '{}'. Use forms like '48h', '2d', '90m', or a bare number of hours.",
                s
            )
        })?;
        if value <= 0 {
            anyhow::bail!("Claim duration must be positive, got '{}'", s);
        }

        let duration = match unit {
            'm' => Duration::minutes(value),
            'h' => Duration::hours(value),
            'd' => Duration::days(value),
            other => anyhow::bail!(
                "Invalid claim duration unit '{}' in '{}'. Valid units: m (minutes), h (hours), d (days).",
                other,
                s
            ),
        };

        Ok(ClaimDuration(duration))
    }
}

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

/// A free-text field of an issue that a targeted search/replace edit can rewrite.
///
/// `mb update --search/--replace` swaps a substring of one of these fields rather
/// than overwriting the whole field, which is the safer interface for agents
/// editing long descriptions (the "aider" search/replace pattern). We use an
/// explicit enum rather than a bare string key so the set of editable fields is
/// closed and checked at compile time. (minibeads-specific)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Description,
    Design,
    Notes,
    Acceptance,
}

impl EditField {
    /// Canonical, user-facing name of this field.
    pub fn as_str(&self) -> &'static str {
        match self {
            EditField::Title => "title",
            EditField::Description => "description",
            EditField::Design => "design",
            EditField::Notes => "notes",
            EditField::Acceptance => "acceptance",
        }
    }
}

impl std::fmt::Display for EditField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for EditField {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "title" => Ok(EditField::Title),
            "description" | "desc" => Ok(EditField::Description),
            "design" => Ok(EditField::Design),
            "notes" => Ok(EditField::Notes),
            "acceptance" | "acceptance_criteria" => Ok(EditField::Acceptance),
            _ => Err(anyhow::anyhow!(
                "Invalid field: '{}'. Valid values are: title, description, design, notes, acceptance",
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

/// File-backed issue comment.
///
/// Comments are stored outside the markdown issue body so syncing them to
/// systems such as GitHub does not churn the main issue description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
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
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub design: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub acceptance_criteria: String,
    pub status: Status,
    pub priority: i32,
    pub issue_type: IssueType,
    #[serde(default)]
    pub assignee: String,
    pub external_ref: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(
        default,
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
    /// When the current claim was taken (minibeads-specific). `None` if unclaimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,
    /// When the current claim expires (minibeads-specific). After this instant
    /// the claim is stale and another worker may reclaim the issue. `None` if
    /// unclaimed (or claimed via a plain `--assignee` with no expiry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_until: Option<DateTime<Utc>>,
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
            claimed_at: None,
            claimed_until: None,
        }
    }

    /// Whether this issue is currently claimed and that claim is still active as
    /// of `now`. A claim with no `claimed_until` (e.g. a plain `--assignee`) is
    /// treated as held indefinitely; a claim past its `claimed_until` is stale
    /// and reclaimable.
    pub fn is_actively_claimed(&self, now: DateTime<Utc>) -> bool {
        if self.assignee.is_empty() {
            return false;
        }
        match self.claimed_until {
            Some(until) => until > now,
            None => true,
        }
    }

    /// Mutable access to one of the issue's free-text fields, selected by
    /// [`EditField`]. Used by targeted search/replace edits so the field name
    /// validated at the CLI maps to exactly one storage location.
    pub fn text_field_mut(&mut self, field: EditField) -> &mut String {
        match field {
            EditField::Title => &mut self.title,
            EditField::Description => &mut self.description,
            EditField::Design => &mut self.design,
            EditField::Notes => &mut self.notes,
            EditField::Acceptance => &mut self.acceptance_criteria,
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

#[cfg(test)]
mod claim_type_tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_duration_units() {
        assert_eq!(
            ClaimDuration::from_str("90m").unwrap().0,
            Duration::minutes(90)
        );
        assert_eq!(
            ClaimDuration::from_str("48h").unwrap().0,
            Duration::hours(48)
        );
        assert_eq!(ClaimDuration::from_str("2d").unwrap().0, Duration::days(2));
        // Bare number is hours.
        assert_eq!(
            ClaimDuration::from_str("12").unwrap().0,
            Duration::hours(12)
        );
        // Case-insensitive unit.
        assert_eq!(ClaimDuration::from_str("3H").unwrap().0, Duration::hours(3));
    }

    #[test]
    fn rejects_bad_durations() {
        assert!(ClaimDuration::from_str("").is_err());
        assert!(ClaimDuration::from_str("0h").is_err());
        assert!(ClaimDuration::from_str("-5h").is_err());
        assert!(ClaimDuration::from_str("abc").is_err());
        assert!(ClaimDuration::from_str("10y").is_err());
    }

    #[test]
    fn default_duration_is_48h() {
        assert_eq!(
            ClaimDuration::default_duration().0,
            Duration::hours(DEFAULT_CLAIM_HOURS)
        );
    }

    #[test]
    fn active_claim_detection() {
        let now = Utc::now();
        let mut issue = Issue::new("demo-1".to_string(), "t".to_string(), 2, IssueType::Task);

        // Unclaimed.
        assert!(!issue.is_actively_claimed(now));

        // Claimed with a future expiry => active.
        issue.assignee = "host-a".to_string();
        issue.claimed_until = Some(now + Duration::hours(1));
        assert!(issue.is_actively_claimed(now));

        // Past expiry => stale.
        issue.claimed_until = Some(now - Duration::hours(1));
        assert!(!issue.is_actively_claimed(now));

        // Assignee but no expiry => held indefinitely.
        issue.claimed_until = None;
        assert!(issue.is_actively_claimed(now));
    }
}
