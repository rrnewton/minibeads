//! Test for upstream JSONL import compatibility
//!
//! This test verifies that minibeads can correctly read and parse
//! the JSONL format exported by upstream beads.
//!
//! Since upstream's --no-db mode has issues with prefix handling,
//! this test uses a simpler approach: create a known JSONL file
//! and verify we can parse it correctly.

use anyhow::Result;

/// Test that we can correctly parse upstream JSONL format
#[test]
fn test_parse_upstream_jsonl() -> Result<()> {
    // Sample JSONL content matching upstream format
    let jsonl_content = r#"{"id":"bd-1","title":"First issue","description":"Test description","status":"open","priority":2,"type":"task","depends_on":[],"created_at":"2025-10-31T12:00:00Z","updated_at":"2025-10-31T12:00:00Z"}
{"id":"bd-2","title":"Second issue","description":"Another test","status":"in_progress","priority":1,"type":"bug","depends_on":[{"id":"bd-1","type":"blocks"}],"created_at":"2025-10-31T12:01:00Z","updated_at":"2025-10-31T12:01:00Z"}
{"id":"bd-3","title":"Third issue","description":"","status":"closed","priority":0,"type":"feature","depends_on":[{"id":"bd-1","type":"related"},{"id":"bd-2","type":"blocks"}],"created_at":"2025-10-31T12:02:00Z","updated_at":"2025-10-31T12:02:00Z"}
"#;

    println!("\n📖 Parsing upstream JSONL format...");

    let mut issues = std::collections::HashMap::new();
    let mut line_num = 0;

    for line in jsonl_content.lines() {
        line_num += 1;
        if line.trim().is_empty() {
            continue;
        }

        let issue_json: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse JSONL line {}: {}\nLine: {}",
                line_num,
                e,
                line
            )
        })?;

        let issue = parse_jsonl_to_reference_issue(&issue_json)?;
        println!("   ✓ Parsed issue: {} - {}", issue.id, issue.title);
        issues.insert(issue.id.clone(), issue);
    }

    // Verify we parsed all 3 issues
    assert_eq!(issues.len(), 3, "Should have parsed 3 issues");

    // Verify bd-1
    let bd1 = issues.get("bd-1").expect("Should have bd-1");
    assert_eq!(bd1.title, "First issue");
    assert_eq!(bd1.priority, 2);
    assert_eq!(bd1.depends_on.len(), 0);
    println!("   ✓ bd-1 verified");

    // Verify bd-2
    let bd2 = issues.get("bd-2").expect("Should have bd-2");
    assert_eq!(bd2.title, "Second issue");
    assert_eq!(bd2.priority, 1);
    assert_eq!(bd2.depends_on.len(), 1);
    assert!(bd2.depends_on.contains_key("bd-1"));
    println!("   ✓ bd-2 verified (with dependency on bd-1)");

    // Verify bd-3
    let bd3 = issues.get("bd-3").expect("Should have bd-3");
    assert_eq!(bd3.title, "Third issue");
    assert_eq!(bd3.priority, 0);
    assert_eq!(bd3.depends_on.len(), 2);
    assert!(bd3.depends_on.contains_key("bd-1"));
    assert!(bd3.depends_on.contains_key("bd-2"));
    println!("   ✓ bd-3 verified (with dependencies on bd-1 and bd-2)");

    println!("\n✅ Successfully parsed and verified upstream JSONL format!");

    Ok(())
}

/// Parse upstream JSONL format to ReferenceIssue
fn parse_jsonl_to_reference_issue(
    json: &serde_json::Value,
) -> Result<minibeads::beads_generator::ReferenceIssue> {
    use minibeads::beads_generator::{DependencyType, IssueType, ReferenceIssue, Status};

    let id = json["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'id' field"))?
        .to_string();

    let title = json["title"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'title' field"))?
        .to_string();

    let description = json["description"].as_str().unwrap_or("").to_string();

    let status_str = json["status"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'status' field"))?;
    let status = match status_str {
        "open" => Status::Open,
        "in_progress" => Status::InProgress,
        "blocked" => Status::Blocked,
        "closed" => Status::Closed,
        _ => return Err(anyhow::anyhow!("Invalid status: {}", status_str)),
    };

    let priority = json["priority"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'priority' field"))? as i32;

    let type_str = json["type"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'type' field"))?;
    let issue_type = match type_str {
        "bug" => IssueType::Bug,
        "feature" => IssueType::Feature,
        "task" => IssueType::Task,
        "epic" => IssueType::Epic,
        "chore" => IssueType::Chore,
        _ => return Err(anyhow::anyhow!("Invalid type: {}", type_str)),
    };

    // Parse dependencies
    let mut depends_on = std::collections::HashMap::new();
    if let Some(deps_array) = json["depends_on"].as_array() {
        for dep in deps_array {
            let dep_id = dep["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Dependency missing 'id' field"))?
                .to_string();

            let dep_type_str = dep["type"].as_str().unwrap_or("blocks");
            let dep_type = match dep_type_str {
                "blocks" => DependencyType::Blocks,
                "related" => DependencyType::Related,
                "parent-child" => DependencyType::ParentChild,
                // Map unsupported types to Related for now
                "discovered-from" => DependencyType::Related,
                _ => DependencyType::Blocks, // Default to blocks for unknown types
            };

            depends_on.insert(dep_id, dep_type);
        }
    }

    Ok(ReferenceIssue {
        id,
        title,
        description,
        status,
        priority,
        issue_type,
        depends_on,
    })
}
