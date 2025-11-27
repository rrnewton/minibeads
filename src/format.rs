use crate::types::{DependencyType, Issue};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Frontmatter for markdown issues
#[derive(Debug, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub depends_on: HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
}

/// Convert an Issue to markdown format
pub fn issue_to_markdown(issue: &Issue) -> Result<String> {
    let mut output = String::new();

    // Build frontmatter
    let fm = Frontmatter {
        title: issue.title.clone(),
        status: issue.status.to_string(),
        priority: issue.priority,
        issue_type: issue.issue_type.to_string(),
        assignee: issue.assignee.clone(),
        external_ref: issue.external_ref.clone(),
        labels: issue.labels.clone(),
        depends_on: issue
            .depends_on
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect(),
        created_at: issue.created_at.to_rfc3339(),
        updated_at: issue.updated_at.to_rfc3339(),
        closed_at: issue.closed_at.map(|t| t.to_rfc3339()),
    };

    // Write YAML frontmatter
    output.push_str("---\n");
    output.push_str(&serde_yaml::to_string(&fm).context("Failed to serialize frontmatter")?);
    output.push_str("---\n");

    // Write markdown sections
    if !issue.description.is_empty() {
        output.push_str("\n# Description\n\n");
        output.push_str(&sanitize_section_content(&issue.description));
        output.push('\n');
    }

    if !issue.design.is_empty() {
        output.push_str("\n# Design\n\n");
        output.push_str(&sanitize_section_content(&issue.design));
        output.push('\n');
    }

    if !issue.acceptance_criteria.is_empty() {
        output.push_str("\n# Acceptance Criteria\n\n");
        output.push_str(&sanitize_section_content(&issue.acceptance_criteria));
        output.push('\n');
    }

    if !issue.notes.is_empty() {
        output.push_str("\n# Notes\n\n");
        output.push_str(&sanitize_section_content(&issue.notes));
        output.push('\n');
    }

    Ok(output)
}

/// Sanitize section content to prevent top-level headers from breaking the format
fn sanitize_section_content(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            if line.starts_with("# ") {
                format!("#{}", line) // Convert H1 to H2
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse markdown format into an Issue
pub fn markdown_to_issue(issue_id: &str, content: &str) -> Result<Issue> {
    // Split frontmatter and body
    let parts: Vec<&str> = content.splitn(3, "---\n").collect();
    if parts.len() < 3 {
        anyhow::bail!("Invalid markdown format: missing frontmatter");
    }

    // Parse frontmatter
    let fm: Frontmatter = serde_yaml::from_str(parts[1]).map_err(|e| {
        // Try to provide helpful context about what field might be missing
        let yaml_error = e.to_string();
        let mut error_msg = format!(
            "Failed to parse frontmatter in {}.md: {}",
            issue_id, yaml_error
        );

        // Show the frontmatter content for debugging
        error_msg.push_str("\n\nFrontmatter content (between --- markers):\n");
        for (i, line) in parts[1].lines().enumerate() {
            error_msg.push_str(&format!("{:3}: {}\n", i + 1, line));
        }

        // Check for common issues
        let mut missing_fields = Vec::new();
        if !parts[1].contains("title:") {
            missing_fields.push("title");
        }
        if !parts[1].contains("status:") {
            missing_fields.push("status");
        }
        if !parts[1].contains("priority:") {
            missing_fields.push("priority");
        }
        if !parts[1].contains("issue_type:") {
            missing_fields.push("issue_type");
        }
        if !parts[1].contains("created_at:") {
            missing_fields.push("created_at");
        }
        if !parts[1].contains("updated_at:") {
            missing_fields.push("updated_at");
        }

        if !missing_fields.is_empty() {
            error_msg.push_str("\nMissing required fields: ");
            error_msg.push_str(&missing_fields.join(", "));
            error_msg.push('\n');
        }

        // Check for common quoting issues
        if yaml_error.contains("did not find expected key") {
            error_msg.push_str("\nPossible cause: Improperly quoted string values.\n");
            error_msg.push_str(
                "If a value contains special characters (like colons), it must be fully quoted.\n",
            );
            error_msg.push_str("Example: title: \"This is: a properly quoted title\"\n");
        }

        anyhow::anyhow!(error_msg)
    })?;

    // Parse body sections
    let (description, design, acceptance_criteria, notes) = parse_sections(parts[2]);

    // Build Issue
    let mut issue = Issue {
        id: issue_id.to_string(),
        title: fm.title,
        description,
        design,
        notes,
        acceptance_criteria,
        status: fm.status.parse()?,
        priority: fm.priority,
        issue_type: fm.issue_type.parse()?,
        assignee: fm.assignee,
        external_ref: fm.external_ref,
        labels: fm.labels,
        depends_on: HashMap::new(),
        dependents: Vec::new(),
        created_at: parse_timestamp(&fm.created_at)?,
        updated_at: parse_timestamp(&fm.updated_at)?,
        closed_at: fm.closed_at.as_ref().and_then(|s| parse_timestamp(s).ok()),
    };

    // Convert dependencies
    for (depends_on_id, dep_type_str) in fm.depends_on {
        let dep_type: DependencyType = dep_type_str.parse()?;
        issue.depends_on.insert(depends_on_id, dep_type);
    }

    Ok(issue)
}

/// Parse markdown sections from the body
fn parse_sections(body: &str) -> (String, String, String, String) {
    let mut description = String::new();
    let mut design = String::new();
    let mut acceptance_criteria = String::new();
    let mut notes = String::new();

    let mut current_section = "";
    let mut current_content = String::new();

    for line in body.lines() {
        let trimmed = line.trim();

        // Check if this is a top-level header
        if let Some(header) = trimmed.strip_prefix("# ") {
            // Save previous section
            if !current_section.is_empty() {
                let content = current_content.trim().to_string();
                match current_section {
                    "Description" => description = content,
                    "Design" => design = content,
                    "Acceptance Criteria" => acceptance_criteria = content,
                    "Notes" => notes = content,
                    _ => {} // Ignore unknown sections
                }
            }

            // Start new section
            current_section = header;
            current_content.clear();
        } else if !current_section.is_empty() {
            // Add line to current section
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    // Save last section
    if !current_section.is_empty() {
        let content = current_content.trim().to_string();
        match current_section {
            "Description" => description = content,
            "Design" => design = content,
            "Acceptance Criteria" => acceptance_criteria = content,
            "Notes" => notes = content,
            _ => {}
        }
    }

    (description, design, acceptance_criteria, notes)
}

/// Parse a timestamp string
fn parse_timestamp(s: &str) -> Result<DateTime<Utc>> {
    // Try RFC3339 format
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&Utc));
    }

    // Try other formats
    let formats = [
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%d %H:%M:%S",
    ];

    for format in &formats {
        if let Ok(t) = DateTime::parse_from_str(s, format) {
            return Ok(t.with_timezone(&Utc));
        }
    }

    anyhow::bail!("Failed to parse timestamp: {}", s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IssueType;

    #[test]
    fn test_issue_roundtrip() {
        let mut issue = Issue::new(
            "test-1".to_string(),
            "Test Issue".to_string(),
            2,
            IssueType::Task,
        );
        issue.description = "Test description".to_string();
        issue
            .depends_on
            .insert("test-2".to_string(), DependencyType::Blocks);

        let markdown = issue_to_markdown(&issue).unwrap();
        let parsed = markdown_to_issue("test-1", &markdown).unwrap();

        assert_eq!(issue.id, parsed.id);
        assert_eq!(issue.title, parsed.title);
        assert_eq!(issue.description, parsed.description);
        assert_eq!(issue.depends_on, parsed.depends_on);
    }

    #[test]
    fn test_sanitize_headers() {
        let content = "# This is a header\nNormal text\n## This is h2";
        let sanitized = sanitize_section_content(content);
        assert!(sanitized.starts_with("## This is a header"));
    }

    #[test]
    fn test_title_with_special_chars() {
        // Test that titles with colons and other special chars are properly quoted
        let test_cases = vec![
            "Simple title",
            "Title: with colon",
            "Entity not found: 0",
            "Title with 'single quotes'",
            "Title with \"double quotes\"",
            "Title with #hash",
            "Multiple: colons: here",
        ];

        for title in test_cases {
            let mut issue = Issue::new("test-1".to_string(), title.to_string(), 2, IssueType::Bug);
            issue.description = "Test".to_string();

            // Serialize to markdown
            let markdown = issue_to_markdown(&issue).unwrap();

            // Parse it back
            let parsed = markdown_to_issue("test-1", &markdown)
                .unwrap_or_else(|e| panic!("Failed to parse title '{}': {}", title, e));

            // Verify the title round-tripped correctly
            assert_eq!(
                parsed.title, title,
                "Title '{}' did not round-trip correctly",
                title
            );
        }
    }
}
