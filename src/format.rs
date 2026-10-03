use crate::types::{DependencyType, Issue};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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
    /// Sorted so that rewriting an issue never reorders its dependency lines,
    /// which would otherwise create spurious diffs and merge conflicts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub depends_on: BTreeMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_until: Option<String>,
}

/// The Markdown body sections of an issue file, in the order they are written.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Section {
    Description,
    Design,
    AcceptanceCriteria,
    Notes,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Description,
        Section::Design,
        Section::AcceptanceCriteria,
        Section::Notes,
    ];

    pub const fn heading(self) -> &'static str {
        match self {
            Section::Description => "Description",
            Section::Design => "Design",
            Section::AcceptanceCriteria => "Acceptance Criteria",
            Section::Notes => "Notes",
        }
    }

    pub fn content(self, issue: &Issue) -> &str {
        match self {
            Section::Description => &issue.description,
            Section::Design => &issue.design,
            Section::AcceptanceCriteria => &issue.acceptance_criteria,
            Section::Notes => &issue.notes,
        }
    }

    pub fn content_mut(self, issue: &mut Issue) -> &mut String {
        match self {
            Section::Description => &mut issue.description,
            Section::Design => &mut issue.design,
            Section::AcceptanceCriteria => &mut issue.acceptance_criteria,
            Section::Notes => &mut issue.notes,
        }
    }
}

/// Serialize the YAML frontmatter of an issue (without the `---` delimiters).
pub fn frontmatter_yaml(issue: &Issue) -> Result<String> {
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
    serde_yaml::to_string(&fm).context("Failed to serialize frontmatter")
}

/// Append the blank-line-delimited heading that opens `section`.
pub fn push_section_heading(output: &mut String, section: Section) {
    output.push_str("\n# ");
    output.push_str(section.heading());
    output.push_str("\n\n");
}

/// Convert an Issue to markdown format
pub fn issue_to_markdown(issue: &Issue) -> Result<String> {
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&frontmatter_yaml(issue)?);
    output.push_str("---\n");

    for section in Section::ALL {
        let content = section.content(issue);
        if !content.is_empty() {
            push_section_heading(&mut output, section);
            output.push_str(&sanitize_section_content(content));
            output.push('\n');
        }
    }

    debug_assert_eq!(
        find_conflict_hunk(&output),
        None,
        "opening markers are escaped"
    );
    Ok(output)
}

/// Sanitize section content so it cannot break the file format: a column-0
/// "# " heading becomes "## ", and a line that opens a git conflict hunk gains
/// a backslash (see [`escape_marker_line`]) so that text which merely quotes a
/// conflict is never mistaken for an unresolved merge.
pub fn sanitize_section_content(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    for (index, line) in content.lines().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        if line.starts_with("# ") {
            output.push('#'); // Convert H1 to H2
        } else if escape_marker_line(line) {
            output.push('\\');
        }
        output.push_str(line);
    }
    output
}

/// Whether `line`, after any run of backslashes, starts with a conflict
/// opening marker (seven or more `<`, then a space or the end of the line).
///
/// Writing prefixes such a line with one more backslash and reading removes
/// one, so the escape is exactly reversible, even for text that itself starts
/// with backslashes. In rendered Markdown `\<` is a literal `<`, so the escaped
/// file still displays the original text.
fn escape_marker_line(line: &str) -> bool {
    marker_run(line.trim_start_matches('\\'), '<').is_some_and(is_marker_label)
}

/// Undo [`sanitize_section_content`]'s marker escape for one line read back.
fn unescape_marker_line(line: &str) -> &str {
    match line.strip_prefix('\\') {
        Some(rest) if escape_marker_line(rest) => rest,
        _ => line,
    }
}

/// 1-based line number in an issue file.
pub type LineNumber = usize;

fn marker_run(line: &str, marker: char) -> Option<&str> {
    let rest = line.trim_start_matches(marker);
    (line.len() - rest.len() >= 7).then_some(rest)
}

/// Git's conflict marker is followed by nothing or by a space and a label.
fn is_marker_label(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(' ')
}

/// First line of an unresolved `git merge` conflict hunk (`<<<<<<<`, then
/// `=======`, then `>>>>>>>`, each at least 7 long), ignoring fenced code
/// blocks so documentation about conflicts stays legal.
pub fn find_conflict_hunk(content: &str) -> Option<LineNumber> {
    #[derive(Clone, Copy)]
    enum State {
        Outside,
        Opened(LineNumber),
        Separated(LineNumber),
    }
    let mut state = State::Outside;
    let mut in_fence = false;
    for (index, line) in content.lines().enumerate() {
        if line.starts_with("# ") {
            // Column-0 "# " is always a section heading (content is escaped
            // to "## "), so an unclosed fence cannot hide later sections.
            in_fence = false;
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        state = match state {
            _ if marker_run(line, '<').is_some_and(is_marker_label) => State::Opened(index + 1),
            State::Opened(start) if marker_run(line, '=') == Some("") => State::Separated(start),
            State::Separated(start) if marker_run(line, '>').is_some_and(is_marker_label) => {
                return Some(start)
            }
            other => other,
        };
    }
    None
}

/// Parse markdown format into an Issue
pub fn markdown_to_issue(issue_id: &str, content: &str) -> Result<Issue> {
    if let Some(line) = find_conflict_hunk(content) {
        anyhow::bail!(
            "{issue_id}.md has an unresolved merge conflict starting at line {line}; \
             keep one side of each <<<<<<< ... >>>>>>> hunk and delete the markers. If the \
             text is meant to quote a conflict, write the opening line as \\<<<<<<<"
        );
    }
    let (frontmatter, body) = split_frontmatter(content)?;

    // Parse frontmatter
    let fm: Frontmatter = serde_yaml::from_str(frontmatter).map_err(|e| {
        // Try to provide helpful context about what field might be missing
        let yaml_error = e.to_string();
        let mut error_msg = format!(
            "Failed to parse frontmatter in {}.md: {}",
            issue_id, yaml_error
        );

        // Show the frontmatter content for debugging
        error_msg.push_str("\n\nFrontmatter content (between --- markers):\n");
        for (i, line) in frontmatter.lines().enumerate() {
            error_msg.push_str(&format!("{:3}: {}\n", i + 1, line));
        }

        // Check for common issues
        let mut missing_fields = Vec::new();
        if !frontmatter.contains("title:") {
            missing_fields.push("title");
        }
        if !frontmatter.contains("status:") {
            missing_fields.push("status");
        }
        if !frontmatter.contains("priority:") {
            missing_fields.push("priority");
        }
        if !frontmatter.contains("issue_type:") {
            missing_fields.push("issue_type");
        }
        if !frontmatter.contains("created_at:") {
            missing_fields.push("created_at");
        }
        if !frontmatter.contains("updated_at:") {
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
    let (description, design, acceptance_criteria, notes) =
        parse_sections(body).with_context(|| format!("Cannot safely parse {issue_id}.md"))?;

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
fn parse_sections(body: &str) -> Result<(String, String, String, String)> {
    let mut description = String::new();
    let mut design = String::new();
    let mut acceptance_criteria = String::new();
    let mut notes = String::new();

    let mut seen = std::collections::HashSet::new();
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
            anyhow::ensure!(matches!(header, "Description" | "Design" | "Acceptance Criteria" | "Notes"),
                "Unknown Markdown section '# {header}'. Use Description, Design, Acceptance Criteria or Notes; use ## for a heading within a section. The file was not rewritten.");
            anyhow::ensure!(seen.insert(header), "Duplicate Markdown section {header:?}; combine its content before updating the issue.");
            // Save previous section
            if !current_section.is_empty() {
                let content = trim_blank_edge_lines(&current_content).to_string();
                match current_section {
                    "Description" => description = content,
                    "Design" => design = content,
                    "Acceptance Criteria" => acceptance_criteria = content,
                    "Notes" => notes = content,
                    _ => unreachable!("section headers were validated"),
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
            current_content.push_str(unescape_marker_line(line));
        } else if !line.trim().is_empty() {
            anyhow::bail!("Text before the first Markdown section would be lost; place it under # Description before updating the issue");
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
            _ => unreachable!("section headers were validated"),
        }
    }

    Ok((description, design, acceptance_criteria, notes))
}

/// Trim leading/trailing BLANK lines (empty or whitespace-only) from section
/// content without touching the indentation of the content itself. A plain
/// `str::trim` here would strip the leading whitespace of an indented first
/// line (e.g. the opening line of an indented code block), which the next
/// write would then mis-escape as a top-level header.
pub fn trim_blank_edge_lines(content: &str) -> &str {
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

/// Delimit frontmatter by complete lines, accepting native LF/CRLF files and a
/// UTF-8 BOM. A "---" substring in a title or YAML value is not a delimiter.
fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.split_inclusive('\n');
    let first = lines
        .next()
        .context("Invalid markdown format: missing frontmatter")?;
    anyhow::ensure!(
        first.trim_end_matches(['\r', '\n']) == "---",
        "Invalid markdown format: missing opening frontmatter delimiter"
    );
    let start = first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok((&content[start..offset], &content[offset + line.len()..]));
        }
        offset += line.len();
    }
    anyhow::bail!("Invalid markdown format: missing closing frontmatter delimiter")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IssueType;

    #[test]
    fn quoted_conflict_markers_round_trip_through_an_escape() {
        let mut issue = Issue::new("test-1".into(), "T".into(), 2, IssueType::Task);
        for description in [
            "Before.\n\n<<<<<<< ours\nmine\n||||||| base\nold\n=======\nyours\n>>>>>>> theirs\n\nAfter.",
            "<<<<<<<\n\\<<<<<<< x\n\\\\<<<<<<<<<\n=======\n>>>>>>>",
            "\\<<<<<<<< not a marker\n<<<<<<<label\n <<<<<<< indented",
        ] {
            issue.description = description.into();
            let markdown = issue_to_markdown(&issue).unwrap();
            assert_eq!(find_conflict_hunk(&markdown), None, "{markdown}");
            assert_eq!(
                markdown_to_issue("test-1", &markdown).unwrap().description,
                description
            );
        }
        let markdown = issue_to_markdown(&issue).unwrap();
        assert!(
            markdown.contains("\n\\\\<<<<<<<< not a marker\n<<<<<<<label\n <<<<<<< indented"),
            "only lines that open a hunk are escaped: {markdown}"
        );
    }

    #[test]
    fn unresolved_conflict_hunks_are_rejected_outside_code_fences() {
        let mut issue = Issue::new("test-1".into(), "T".into(), 2, IssueType::Task);
        let clean =
            "Before.\n\n<<<<<<< ours\nmine\n||||||| base\nold\n=======\nyours\n>>>>>>> theirs\n\nAfter.";
        let markdown = format!(
            "{}\n# Description\n\n{clean}\n",
            issue_to_markdown(&issue).unwrap()
        );
        let error = markdown_to_issue("test-1", &markdown)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unresolved merge conflict"), "{error}");
        assert!(
            error.contains("\\<<<<<<<"),
            "the error explains the escape: {error}"
        );
        assert_eq!(
            find_conflict_hunk(&markdown),
            markdown
                .lines()
                .position(|line| line == "<<<<<<< ours")
                .map(|index| index + 1)
        );

        issue.description =
            "Git writes:\n\n```\n<<<<<<< ours\na\n=======\nb\n>>>>>>> theirs\n```".into();
        let markdown = issue_to_markdown(&issue).unwrap();
        assert_eq!(
            markdown_to_issue("test-1", &markdown).unwrap().description,
            issue.description
        );
        assert_eq!(find_conflict_hunk("<<<<<<< ours\n=======\n"), None);
        assert_eq!(
            find_conflict_hunk("<<<<<<<<<\n=========\n>>>>>>>>>\n"),
            Some(1)
        );
    }

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
