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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_until: Option<String>,
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
        claimed_at: issue.claimed_at.map(|t| t.to_rfc3339()),
        claimed_until: issue.claimed_until.map(|t| t.to_rfc3339()),
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
        claimed_at: fm.claimed_at.as_ref().and_then(|s| parse_timestamp(s).ok()),
        claimed_until: fm
            .claimed_until
            .as_ref()
            .and_then(|s| parse_timestamp(s).ok()),
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
        // Check if this is a top-level header. Only column-0 "# " lines count:
        // `sanitize_section_content` escapes any column-0 "# " inside section
        // content to "## " at write time, so an INDENTED "# ..." line (e.g. a
        // shell comment in an indented code block) is body content, never a
        // header. Trimming before this check used to eat such lines as unknown
        // section headers and silently drop everything after them.
        if let Some(header) = line.strip_prefix("# ").map(str::trim) {
            // Save previous section
            if !current_section.is_empty() {
                let content = trim_blank_edge_lines(&current_content).to_string();
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
        let content = trim_blank_edge_lines(&current_content).to_string();
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

/// Trim leading/trailing BLANK lines (empty or whitespace-only) from section
/// content without touching the indentation of the content itself. A plain
/// `str::trim` here would strip the leading whitespace of an indented first
/// line (e.g. the opening line of an indented code block), which the next
/// write would then mis-escape as a top-level header.
fn trim_blank_edge_lines(content: &str) -> &str {
    let mut s = content;
    while let Some(i) = s.find('\n') {
        if !s[..i].trim().is_empty() {
            break;
        }
        s = &s[i + 1..];
    }
    while let Some(i) = s.rfind('\n') {
        if !s[i + 1..].trim().is_empty() {
            break;
        }
        s = &s[..i];
    }
    if s.trim().is_empty() {
        ""
    } else {
        s
    }
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
    fn test_claim_fields_roundtrip() {
        // A claimed issue must round-trip its assignee + claim window losslessly
        // through markdown so the claim survives sync to/from JSONL.
        let mut issue = Issue::new(
            "test-9".to_string(),
            "Claimed".to_string(),
            2,
            IssueType::Task,
        );
        issue.assignee = "buildbox/backend".to_string();
        issue.status = crate::types::Status::InProgress;
        let now = Utc::now();
        issue.claimed_at = Some(now);
        issue.claimed_until = Some(now + chrono::Duration::hours(48));

        let markdown = issue_to_markdown(&issue).unwrap();
        assert!(markdown.contains("assignee: buildbox/backend"));
        assert!(markdown.contains("claimed_until:"));

        let parsed = markdown_to_issue("test-9", &markdown).unwrap();
        assert_eq!(parsed.assignee, "buildbox/backend");
        assert_eq!(parsed.status, crate::types::Status::InProgress);
        assert_eq!(parsed.claimed_at, issue.claimed_at);
        assert_eq!(parsed.claimed_until, issue.claimed_until);
    }

    #[test]
    fn test_unclaimed_issue_omits_claim_fields() {
        let issue = Issue::new(
            "test-8".to_string(),
            "Plain".to_string(),
            2,
            IssueType::Task,
        );
        let markdown = issue_to_markdown(&issue).unwrap();
        assert!(!markdown.contains("claimed_at"));
        assert!(!markdown.contains("claimed_until"));
        assert!(!markdown.contains("assignee"));
    }

    #[test]
    fn test_indented_code_block_survives_roundtrip() {
        // Regression test for the mb-migrate description-truncation bug:
        // an indented "# ..." line (a shell/Python comment inside an
        // indented code block) was mistaken for a top-level section header,
        // and everything from that line onward was silently dropped the
        // next time the issue was parsed and rewritten (e.g. by
        // `mb mb-migrate --to numeric`).
        let mut issue = Issue::new(
            "test-1".to_string(),
            "Trunc".to_string(),
            2,
            IssueType::Task,
        );
        issue.description = "A\n    # x\nB".to_string();

        let markdown = issue_to_markdown(&issue).unwrap();
        let parsed = markdown_to_issue("test-1", &markdown).unwrap();
        assert_eq!(parsed.description, issue.description);
    }

    #[test]
    fn test_adversarial_descriptions_roundtrip() {
        // (input, expected after one write->parse cycle). `expected` differs
        // from `input` only where the format documents a normalization:
        // column-0 "# " lines are escaped to "## " by
        // sanitize_section_content, and leading/trailing blank lines are
        // trimmed by parse_sections. Nothing may ever be dropped or rerouted
        // to another field.
        let cases: Vec<(&str, &str)> = vec![
            // Indented code block, 4-space and 1-space indents
            (
                "Before\n\n    # comment in code\n    make install\n\nAfter",
                "Before\n\n    # comment in code\n    make install\n\nAfter",
            ),
            (" # one-space indent\nrest", " # one-space indent\nrest"),
            // Description STARTING with an indented code-block comment: the
            // first line's indentation must survive (a bare trim would turn
            // it into a column-0 "# " line and corrupt it on the next write).
            (
                "    # first line indented\n    cmd\nrest",
                "    # first line indented\n    cmd\nrest",
            ),
            // Fenced code block: indented comment, shebang, no-space hash
            (
                "```sh\n  # comment\n#!/bin/bash\n#nospace\n```\ntail",
                "```sh\n  # comment\n#!/bin/bash\n#nospace\n```\ntail",
            ),
            // Fenced block with a column-0 "# " line: documented escape to
            // "## ", but nothing after it may be lost.
            (
                "```sh\n# fenced comment\nmake\n```\ntail",
                "```sh\n## fenced comment\nmake\n```\ntail",
            ),
            // Indented line resembling a KNOWN section header: must stay in
            // the description, not reroute following content into Notes.
            (
                "intro\n    # Notes\nstill description",
                "intro\n    # Notes\nstill description",
            ),
            // Lines resembling frontmatter keys and a document separator
            (
                "title: fake\nstatus: open\n---\npriority: 9",
                "title: fake\nstatus: open\n---\npriority: 9",
            ),
            // An H2 that resembles a section header is not a boundary
            (
                "## Design\nnot the design section",
                "## Design\nnot the design section",
            ),
            // Trailing blank lines: trimmed, nothing else lost
            ("A\nB\n\n\n", "A\nB"),
        ];

        for (input, expected) in cases {
            let mut issue = Issue::new(
                "test-1".to_string(),
                "Adversarial".to_string(),
                2,
                IssueType::Task,
            );
            issue.description = input.to_string();
            issue.notes = "notes stay put".to_string();

            let markdown = issue_to_markdown(&issue).unwrap();
            let parsed = markdown_to_issue("test-1", &markdown).unwrap();
            assert_eq!(
                parsed.description, expected,
                "description round-trip for {:?}",
                input
            );
            assert_eq!(
                parsed.notes, "notes stay put",
                "notes leaked for {:?}",
                input
            );

            // A second cycle must be a fixpoint: nothing drifts further.
            let markdown2 = issue_to_markdown(&parsed).unwrap();
            let parsed2 = markdown_to_issue("test-1", &markdown2).unwrap();
            assert_eq!(
                parsed2.description, expected,
                "round-trip not stable for {:?}",
                input
            );
            assert_eq!(parsed2.notes, "notes stay put");
        }
    }

    #[test]
    fn test_all_sections_hold_indented_code_blocks() {
        let body = "text\n    # comment\n    cmd --flag\nmore text";
        let mut issue = Issue::new(
            "test-1".to_string(),
            "Sections".to_string(),
            2,
            IssueType::Task,
        );
        issue.description = body.to_string();
        issue.design = body.to_string();
        issue.acceptance_criteria = body.to_string();
        issue.notes = body.to_string();

        let markdown = issue_to_markdown(&issue).unwrap();
        let parsed = markdown_to_issue("test-1", &markdown).unwrap();
        assert_eq!(parsed.description, body);
        assert_eq!(parsed.design, body);
        assert_eq!(parsed.acceptance_criteria, body);
        assert_eq!(parsed.notes, body);
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
