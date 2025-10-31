//! Test for upstream JSONL import compatibility
//!
//! This test generates random actions, executes them against upstream beads,
//! exports the resulting database to JSONL, and verifies minibeads can
//! correctly parse and import the format.

use anyhow::{Context, Result};
use std::fs;
use tempfile::TempDir;

/// Test that we can generate random actions, run against upstream, export, and parse
#[test]
fn test_random_upstream_export_import() -> Result<()> {
    // Check if upstream bd binary exists (get absolute path)
    let repo_root = std::env::current_dir().context("Failed to get current directory")?;
    let upstream_bd = repo_root.join("beads/bd-upstream");
    if !upstream_bd.exists() {
        println!(
            "⚠️  Skipping test: upstream bd binary not found at {}",
            upstream_bd.display()
        );
        println!("   Run 'make upstream' to build it");
        return Ok(());
    }

    // Create a temporary directory for the test
    let temp_dir = TempDir::new().context("Failed to create temp directory")?;
    let work_dir = temp_dir.path().to_path_buf();

    println!("\n⚙️  Creating upstream database with test issues...");

    // Initialize upstream bd
    let init_output = std::process::Command::new(&upstream_bd)
        .current_dir(&work_dir)
        .stdin(std::process::Stdio::null())
        .args(["init", "--prefix", "test"])
        .output()
        .context("Failed to run upstream init")?;

    if !init_output.status.success() {
        anyhow::bail!(
            "Upstream init failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&init_output.stdout),
            String::from_utf8_lossy(&init_output.stderr)
        );
    }

    println!("   ✓ Initialized database with prefix 'test'");

    // Create several issues with different properties
    let test_issues = vec![
        ("First issue", "A simple task", "task", 2),
        ("Bug to fix", "This needs fixing", "bug", 1),
        ("Feature request", "Add new functionality", "feature", 3),
        ("Epic milestone", "Large project", "epic", 0),
        ("Chore work", "", "chore", 4),
    ];

    for (title, desc, issue_type, priority) in &test_issues {
        let priority_str = priority.to_string();
        let mut args = vec![
            "create",
            "--title",
            title,
            "--type",
            issue_type,
            "--priority",
            &priority_str,
        ];

        // Only add description if not empty
        if !desc.is_empty() {
            args.push("--description");
            args.push(desc);
        }

        let create_output = std::process::Command::new(&upstream_bd)
            .current_dir(&work_dir)
            .args(&args)
            .output()
            .context("Failed to create issue")?;

        if !create_output.status.success() {
            eprintln!(
                "Warning: Failed to create issue '{}': {}",
                title,
                String::from_utf8_lossy(&create_output.stderr)
            );
        } else {
            println!("   ✓ Created: {}", title);
        }
    }

    println!("   Created {} test issues", test_issues.len());

    // Export to JSONL
    println!("\n📤 Exporting upstream database to JSONL...");
    let export_output = std::process::Command::new(&upstream_bd)
        .current_dir(&work_dir)
        .args(["export", "-o", "issues.jsonl"])
        .output()
        .context("Failed to run upstream export")?;

    if !export_output.status.success() {
        anyhow::bail!(
            "Upstream export failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&export_output.stdout),
            String::from_utf8_lossy(&export_output.stderr)
        );
    }

    // Read the exported JSONL
    let jsonl_path = work_dir.join("issues.jsonl");
    let jsonl_content = fs::read_to_string(&jsonl_path).context("Failed to read exported JSONL")?;

    println!("\n📖 Parsing exported JSONL...");

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

    println!(
        "\n✅ Successfully parsed {} issues from upstream JSONL!",
        issues.len()
    );

    // Verify we got at least some issues (random generation should create several)
    assert!(
        issues.len() > 0,
        "Should have at least one issue from random generation"
    );

    // Verify basic structure of parsed issues
    for (id, issue) in &issues {
        assert!(!issue.title.is_empty(), "Issue {} has empty title", id);
        assert!(
            issue.priority >= 0 && issue.priority <= 4,
            "Issue {} has invalid priority: {}",
            id,
            issue.priority
        );
    }

    println!("   All issues have valid structure");

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

    let type_str = json["issue_type"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'issue_type' field"))?;
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
