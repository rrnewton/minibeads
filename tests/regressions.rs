use std::process::Command;

#[test]
fn cli_help_and_version_start_successfully() {
    for argument in ["--help", "--version", "github"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mb"));
        command.arg(argument);
        if argument == "github" {
            command.arg("--help");
        }
        let output = command.output().expect("run the actual CLI binary");
        assert!(
            output.status.success(),
            "{argument}: {:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty(), "{argument} produced no output");
    }
}

use minibeads::storage::{IssueStorageLayout, Storage};
use minibeads::types::{Issue, IssueType};
use std::fs;

fn fixture(layout: IssueStorageLayout) -> (tempfile::TempDir, Storage) {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::init(
        temp.path().join("database"),
        Some("r".into()),
        false,
        layout,
    )
    .unwrap();
    (temp, storage)
}

fn create(storage: &Storage, id: &str, title: &str) -> anyhow::Result<Issue> {
    storage.create_issue(
        title.into(),
        String::new(),
        None,
        None,
        2,
        IssueType::Task,
        None,
        Vec::new(),
        None,
        Some(id.into()),
        Vec::new(),
    )
}

#[test]
fn unsafe_ids_cannot_create_rename_read_or_write_comments() {
    for layout in [IssueStorageLayout::Flat, IssueStorageLayout::Sharded] {
        let (_temp, storage) = fixture(layout);
        create(&storage, "r-1", "Keep me").unwrap();
        for id in [
            "",
            ".",
            "..",
            "../../outside",
            r"..\..\outside",
            "/absolute",
            r"C:\outside",
            r"C:outside",
            r"\\server\share",
            "r-1:stream",
            "CON",
            "NUL.txt",
            "r-2.",
            "r-2 ",
        ] {
            assert!(
                create(&storage, id, "Bad").is_err(),
                "create accepted {id:?}"
            );
            assert!(
                storage.rename_issue("r-1", id, false).is_err(),
                "rename accepted {id:?}"
            );
            assert!(storage.get_issue(id).is_err(), "read accepted {id:?}");
            assert!(
                storage.list_comments(id).is_err(),
                "comments accepted {id:?}"
            );
        }
        assert_eq!(storage.get_issue("r-1").unwrap().unwrap().title, "Keep me");
    }
}

#[test]
fn unsafe_prefixes_are_rejected_before_initialization_or_config_update() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("uninitialized");
    assert!(Storage::init(
        path.clone(),
        Some("../escape".into()),
        false,
        IssueStorageLayout::Flat
    )
    .is_err());
    assert!(!path.exists());
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    assert!(storage
        .set_config_value("issue-prefix", r"..\escape")
        .is_err());
    assert_eq!(storage.get_prefix().unwrap(), "r");
}

#[test]
fn hostile_jsonl_does_not_overwrite_a_file_outside_the_database() {
    let (temp, storage) = fixture(IssueStorageLayout::Flat);
    let sentinel = temp.path().join("outside.md");
    fs::write(&sentinel, "keep this").unwrap();
    let issue = Issue::new(
        "../../outside".into(),
        "overwrite".into(),
        2,
        IssueType::Task,
    );
    let input = temp.path().join("input.jsonl");
    fs::write(&input, serde_json::to_string(&issue).unwrap()).unwrap();
    assert!(storage.import_from_jsonl(&input, true).is_err());
    assert!(minibeads::sync::load_jsonl_issues(&input).is_err());
    let output = Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(["--mb-no-cmd-logging", "--json", "--mb-beads-dir"])
        .arg(storage.get_beads_dir())
        .args(["sync", "--jsonl"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep this");
}

#[cfg(unix)]
#[test]
fn a_symlink_cannot_redirect_issue_writes_outside_the_database() {
    let (temp, storage) = fixture(IssueStorageLayout::Flat);
    let sentinel = temp.path().join("outside.md");
    fs::write(&sentinel, "keep this").unwrap();
    std::os::unix::fs::symlink(&sentinel, storage.get_beads_dir().join("issues/r-1.md")).unwrap();
    assert!(create(&storage, "r-1", "overwrite").is_err());
    assert!(storage.get_issue("r-1").is_err());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep this");
}

fn import_fixture_issues(storage: &Storage, ids: &[&str]) -> Vec<Issue> {
    let base = chrono::Utc::now() - chrono::Duration::days(1);
    let issues: Vec<Issue> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let mut issue = Issue::new(
                (*id).into(),
                format!("original {index}"),
                2,
                IssueType::Task,
            );
            issue.created_at = base + chrono::Duration::seconds(index as i64);
            issue.updated_at = issue.created_at;
            issue
        })
        .collect();
    let input = tempfile::NamedTempFile::new().unwrap();
    let content = issues
        .iter()
        .map(|issue| serde_json::to_string(issue).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(input.path(), content).unwrap();
    let (imported, skipped, errors) = storage.import_from_jsonl(input.path(), true).unwrap();
    assert_eq!(imported, issues.len());
    assert_eq!(skipped, 0);
    assert!(errors.is_empty());
    issues
}

#[test]
fn repacking_swaps_and_cycles_preserves_all_issues_dependencies_and_comments() {
    use minibeads::types::DependencyType;
    for layout in [IssueStorageLayout::Flat, IssueStorageLayout::Sharded] {
        for ids in [vec!["r-2", "r-1"], vec!["r-3", "r-1", "r-2"]] {
            let (_temp, storage) = fixture(layout);
            let issues = import_fixture_issues(&storage, &ids);
            storage
                .add_dependency(ids[0], ids[1], DependencyType::Blocks)
                .unwrap();
            let comment = storage
                .add_comment(ids[0], "review", "keep this comment")
                .unwrap();
            let (_, mapping) = storage.repack_numeric_ids(false, None).unwrap();
            for original in &issues {
                let id = mapping.get(&original.id).unwrap_or(&original.id);
                assert_eq!(
                    storage.get_issue(id).unwrap().unwrap().title,
                    original.title
                );
            }
            let new_first = &mapping[ids[0]];
            let new_second = mapping.get(ids[1]).map(String::as_str).unwrap_or(ids[1]);
            assert!(storage
                .get_issue(new_first)
                .unwrap()
                .unwrap()
                .depends_on
                .contains_key(new_second));
            let comments = storage.list_comments(new_first).unwrap();
            assert_eq!(comments.len(), 1);
            assert_eq!(comments[0].id, comment.id);
            assert_eq!(&comments[0].issue_id, new_first);
            let all = minibeads::sync::load_markdown_issues(&storage.get_beads_dir()).unwrap();
            assert_eq!(all.len(), ids.len());
        }
    }
}

#[test]
fn every_id_migration_preserves_comment_ownership_ancestry_and_jsonl_ids() {
    for mode in ["rename", "prefix", "hash", "numeric", "repack"] {
        let (_temp, storage) = fixture(IssueStorageLayout::Flat);
        let old_id = if mode == "numeric" { "r-abcd" } else { "r-2" };
        let originals = import_fixture_issues(&storage, &[old_id]);
        let comment = storage
            .add_comment(old_id, "review", "keep my identity")
            .unwrap();
        let state = serde_json::json!({"issues": {"https://github.com/review/fixture/issues/1": {
            "local_id": old_id,
            "synced_comments": [{"local_id": comment.id, "remote_id": "101"}]
        }}});
        fs::write(
            storage.get_beads_dir().join("github-sync-state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        fs::write(
            storage.get_beads_dir().join("issues.jsonl"),
            serde_json::to_vec(&originals[0]).unwrap(),
        )
        .unwrap();
        let new_id = match mode {
            "rename" => {
                storage.rename_issue(old_id, "r-9", false).unwrap();
                "r-9".to_owned()
            }
            "prefix" => {
                storage.rename_prefix("new", false, false).unwrap();
                "new-2".to_owned()
            }
            "hash" => storage.migrate_to_hash_ids(false, true).unwrap().1[old_id].clone(),
            "numeric" => storage.migrate_to_numeric_ids(false, true).unwrap().1[old_id].clone(),
            _ => storage.repack_numeric_ids(false, None).unwrap().1[old_id].clone(),
        };
        let comments = storage.list_comments(&new_id).unwrap();
        assert_eq!(comments.len(), 1, "{mode}");
        assert_eq!(comments[0].id, comment.id, "{mode}");
        assert_eq!(comments[0].issue_id, new_id, "{mode}");
        assert!(storage.list_comments(old_id).unwrap().is_empty(), "{mode}");
        let state: serde_json::Value = serde_json::from_slice(
            &fs::read(storage.get_beads_dir().join("github-sync-state.json")).unwrap(),
        )
        .unwrap();
        let ancestry = &state["issues"]["https://github.com/review/fixture/issues/1"];
        assert_eq!(ancestry["local_id"], new_id, "{mode}");
        assert_eq!(
            ancestry["synced_comments"][0]["local_id"], comment.id,
            "{mode}"
        );
        let jsonl =
            minibeads::sync::load_jsonl_issues(&storage.get_beads_dir().join("issues.jsonl"))
                .unwrap();
        assert!(jsonl.contains_key(&new_id), "{mode}");
        assert!(!jsonl.contains_key(old_id), "{mode}");
        assert!(!storage
            .get_beads_dir()
            .join("minibeads-transaction.json")
            .exists());
    }
}

#[test]
fn migration_preflight_failure_preserves_sources_and_config() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "keep issue").unwrap();
    fs::create_dir_all(storage.get_beads_dir().join("comments")).unwrap();
    fs::write(
        storage.get_beads_dir().join("comments/new-1.json"),
        b"orphan comments",
    )
    .unwrap();
    assert!(storage.rename_prefix("new", false, false).is_err());
    assert_eq!(
        storage.get_issue("r-1").unwrap().unwrap().title,
        "keep issue"
    );
    assert_eq!(storage.get_prefix().unwrap(), "r");
    assert_eq!(
        fs::read(storage.get_beads_dir().join("comments/new-1.json")).unwrap(),
        b"orphan comments"
    );
}

#[test]
fn repacking_closed_issues_can_swap_their_ids_with_open_issues() {
    for layout in [IssueStorageLayout::Flat, IssueStorageLayout::Sharded] {
        let (_temp, storage) = fixture(layout);
        import_fixture_issues(&storage, &["r-1", "r-2"]);
        storage.close_issue("r-1", "done").unwrap();
        storage.repack_numeric_ids(false, Some(2)).unwrap();
        let open = storage.get_issue("r-1").unwrap().unwrap();
        let closed = storage.get_issue("r-2").unwrap().unwrap();
        assert_eq!(open.title, "original 1");
        assert_eq!(open.status, minibeads::types::Status::Open);
        assert_eq!(closed.title, "original 0");
        assert_eq!(closed.status, minibeads::types::Status::Closed);
    }
}

#[test]
fn closing_and_reopening_prerequisites_updates_ready_blocked_and_stats() {
    use minibeads::types::DependencyType;
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    for id in ["r-1", "r-2", "r-3", "r-4"] {
        create(&storage, id, id).unwrap();
    }
    storage
        .add_dependency("r-2", "r-1", DependencyType::Blocks)
        .unwrap();
    storage
        .add_dependency("r-3", "r-missing", DependencyType::Blocks)
        .unwrap();
    storage
        .add_dependency("r-4", "r-1", DependencyType::Related)
        .unwrap();
    assert_eq!(storage.get_stats().unwrap().blocked_issues, 2);
    storage.close_issue("r-1", "done").unwrap();
    let ready = storage.get_ready(None, None, None, "priority").unwrap();
    assert!(ready.iter().any(|issue| issue.id == "r-2"));
    assert!(ready.iter().any(|issue| issue.id == "r-4"));
    assert!(!ready.iter().any(|issue| issue.id == "r-3"));
    let blocked = storage.get_blocked().unwrap();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].issue.id, "r-3");
    assert_eq!(blocked[0].blocked_by, ["r-missing"]);
    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.ready_issues, ready.len());
    assert_eq!(stats.blocked_issues, blocked.len());
    storage.reopen_issue("r-1").unwrap();
    let ready = storage.get_ready(None, None, None, "priority").unwrap();
    assert!(!ready.iter().any(|issue| issue.id == "r-2"));
    assert_eq!(storage.get_stats().unwrap().blocked_issues, 2);
}

#[test]
fn ready_filters_do_not_hide_a_prerequisite_in_another_assignee_or_priority() {
    use minibeads::types::DependencyType;
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "prerequisite").unwrap();
    create(&storage, "r-2", "dependent").unwrap();
    storage
        .update_issue(
            "r-1",
            std::collections::HashMap::from([
                ("priority".to_owned(), "0".to_owned()),
                ("assignee".to_owned(), "other".to_owned()),
            ]),
        )
        .unwrap();
    storage
        .update_issue(
            "r-2",
            std::collections::HashMap::from([("assignee".to_owned(), "worker".to_owned())]),
        )
        .unwrap();
    storage
        .add_dependency("r-2", "r-1", DependencyType::Blocks)
        .unwrap();
    assert!(storage
        .get_ready(
            Some("worker"),
            Some(vec![2]),
            Some(IssueType::Task),
            "priority"
        )
        .unwrap()
        .is_empty());
    storage.close_issue("r-1", "done").unwrap();
    let ready = storage
        .get_ready(
            Some("worker"),
            Some(vec![2]),
            Some(IssueType::Task),
            "priority",
        )
        .unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "r-2");
}

#[test]
fn updates_refuse_unrecognized_or_ambiguous_sections_without_losing_text() {
    for extra in [
        "\n# User Report\n\nirreplaceable investigation\n",
        "\n# Notes\n\nfirst notes\n\n# Notes\n\nsecond notes\n",
    ] {
        let (_temp, storage) = fixture(IssueStorageLayout::Flat);
        create(&storage, "r-1", "keep").unwrap();
        let path = storage.get_beads_dir().join("issues/r-1.md");
        let original = fs::read_to_string(&path).unwrap() + extra;
        fs::write(&path, &original).unwrap();
        let error = storage
            .update_issue(
                "r-1",
                std::collections::HashMap::from([("priority".to_owned(), "1".to_owned())]),
            )
            .expect_err("a partial parse must not be rewritten");
        assert!(format!("{error:#}").contains("Markdown section"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
}

#[test]
fn crlf_and_bom_frontmatter_survive_an_ordinary_update() {
    for bom in ["", "\u{feff}"] {
        let (_temp, storage) = fixture(IssueStorageLayout::Flat);
        create(&storage, "r-1", "Title ending in ---").unwrap();
        let path = storage.get_beads_dir().join("issues/r-1.md");
        let original = fs::read_to_string(&path).unwrap() + "\n# Description\n\n  indented text\n";
        fs::write(&path, format!("{bom}{}", original.replace('\n', "\r\n"))).unwrap();
        let updated = storage
            .update_issue(
                "r-1",
                std::collections::HashMap::from([("priority".to_owned(), "1".to_owned())]),
            )
            .unwrap();
        assert_eq!(updated.title, "Title ending in ---");
        assert_eq!(updated.description, "  indented text");
        assert_eq!(storage.get_issue("r-1").unwrap().unwrap().priority, 1);
    }
}

#[test]
fn unstructured_preamble_is_rejected_instead_of_discarded() {
    let issue = Issue::new("r-1".into(), "title".into(), 2, IssueType::Task);
    let markdown = minibeads::format::issue_to_markdown(&issue).unwrap();
    let malformed = markdown + "\nimportant prose before any section\n";
    let error = minibeads::format::markdown_to_issue("r-1", &malformed).unwrap_err();
    assert!(format!("{error:#}").contains("before the first Markdown section"));
}

fn run_cli(storage: &Storage, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(["--mb-no-cmd-logging", "--json", "--mb-beads-dir"])
        .arg(storage.get_beads_dir())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn jsonl_sync_waits_for_the_database_lock_instead_of_writing_through_it() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "keep").unwrap();
    let _lock = minibeads::lock::Lock::acquire(&storage.get_beads_dir()).unwrap();
    let output = run_cli(&storage, &["sync"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to acquire lock"));
    assert!(!storage.get_beads_dir().join("issues.jsonl").exists());
}

#[test]
fn a_stale_jsonl_sync_plan_cannot_overwrite_a_new_local_edit() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "original").unwrap();
    let engine = minibeads::sync::SyncEngine::new();
    let markdown = minibeads::sync::load_markdown_issues(&storage.get_beads_dir()).unwrap();
    let jsonl = std::collections::HashMap::new();
    let plan = engine.analyze_ref(&markdown, &jsonl).unwrap();
    storage
        .update_issue(
            "r-1",
            std::collections::HashMap::from([("title".to_owned(), "concurrent edit".to_owned())]),
        )
        .unwrap();
    let error = engine
        .apply(&plan, &markdown, &jsonl, &storage.get_beads_dir(), false)
        .unwrap_err();
    assert!(error.to_string().contains("changed after analysis"));
    assert_eq!(
        storage.get_issue("r-1").unwrap().unwrap().title,
        "concurrent edit"
    );
    assert!(!storage.get_beads_dir().join("issues.jsonl").exists());
}

#[test]
fn custom_jsonl_paths_are_honored_and_dry_runs_return_json_with_planned_counts() {
    let (temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "local").unwrap();
    let custom = temp.path().join("custom.jsonl");
    fs::write(&custom, "").unwrap();
    let dry = run_cli(
        &storage,
        &["sync", "--jsonl", custom.to_str().unwrap(), "--dry-run"],
    );
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(report["created_in_jsonl"], 1);
    assert_eq!(report["dry_run"], true);
    assert!(fs::read(&custom).unwrap().is_empty());
    assert!(!storage.get_beads_dir().join("issues.jsonl").exists());
    let applied = run_cli(&storage, &["sync", "--jsonl", custom.to_str().unwrap()]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(report["created_in_jsonl"], 1);
    assert_eq!(report["dry_run"], false);
    assert!(minibeads::sync::load_jsonl_issues(&custom)
        .unwrap()
        .contains_key("r-1"));
    assert!(!storage.get_beads_dir().join("issues.jsonl").exists());
}

#[test]
fn equal_time_content_conflicts_are_reported_without_overwriting_either_copy() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    let issue = create(&storage, "r-1", "markdown title").unwrap();
    let mut remote = issue.clone();
    remote.title = "JSONL title".into();
    let markdown_path = storage.get_beads_dir().join("issues/r-1.md");
    let jsonl_path = storage.get_beads_dir().join("issues.jsonl");
    let jsonl_bytes = serde_json::to_vec(&remote).unwrap();
    fs::write(&jsonl_path, &jsonl_bytes).unwrap();
    filetime::set_file_mtime(
        &markdown_path,
        filetime::FileTime::from_system_time(issue.updated_at.into()),
    )
    .unwrap();
    let original_markdown = fs::read(&markdown_path).unwrap();
    let output = run_cli(&storage, &["sync"]);
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["skipped_conflicts"], 1);
    assert!(report["errors"][0].as_str().unwrap().contains("Conflict"));
    assert_eq!(fs::read(markdown_path).unwrap(), original_markdown);
    assert_eq!(fs::read(jsonl_path).unwrap(), jsonl_bytes);
}

#[test]
fn jsonl_sync_ignores_timestamp_and_label_order_when_content_is_unchanged() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "same content").unwrap();
    let issue = storage
        .set_labels("r-1", vec!["alpha".into(), "beta".into()])
        .unwrap();
    let mut jsonl_issue = issue.clone();
    jsonl_issue.labels.reverse();
    jsonl_issue.updated_at += chrono::Duration::hours(1);
    let jsonl_path = storage.get_beads_dir().join("issues.jsonl");
    fs::write(&jsonl_path, serde_json::to_vec(&jsonl_issue).unwrap()).unwrap();
    let output = run_cli(&storage, &["sync", "--dry-run"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["updated_jsonl"], 0);
    assert_eq!(report["updated_markdown"], 0);
    assert_eq!(report["skipped_conflicts"], 0);
}

#[test]
fn duplicate_records_are_rejected_before_sync() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    let issue = create(&storage, "r-1", "keep").unwrap();
    let record = serde_json::to_string(&issue).unwrap();
    let jsonl_path = storage.get_beads_dir().join("issues.jsonl");
    fs::write(&jsonl_path, format!("{record}\n{record}\n")).unwrap();
    assert!(minibeads::sync::load_jsonl_issues(&jsonl_path).is_err());
    let duplicate_dir = storage.get_beads_dir().join("issues/duplicate");
    fs::create_dir_all(&duplicate_dir).unwrap();
    fs::copy(
        storage.get_beads_dir().join("issues/r-1.md"),
        duplicate_dir.join("r-1.md"),
    )
    .unwrap();
    assert!(minibeads::sync::load_markdown_issues(&storage.get_beads_dir()).is_err());
}

#[cfg(windows)]
#[test]
fn an_unsuccessful_jsonl_write_returns_failure() {
    let (temp, storage) = fixture(IssueStorageLayout::Flat);
    create(&storage, "r-1", "keep").unwrap();
    let path = temp.path().join("readonly.jsonl");
    fs::write(&path, "").unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let output = run_cli(&storage, &["sync", "--jsonl", path.to_str().unwrap()]);
    fs::set_permissions(&path, original_permissions).unwrap();
    assert!(!output.status.success());
    assert!(fs::read(path).unwrap().is_empty());
}

#[test]
fn conditional_updates_preserve_manual_edits_even_without_a_timestamp_change() {
    let (_temp, storage) = fixture(IssueStorageLayout::Flat);
    let snapshot = create(&storage, "r-1", "original").unwrap();
    let path = storage.get_beads_dir().join("issues/r-1.md");
    let mut changed = snapshot.clone();
    changed.description = "manually edited without touching updated_at".into();
    fs::write(
        &path,
        minibeads::format::issue_to_markdown(&changed).unwrap(),
    )
    .unwrap();
    let result = storage
        .update_issue_if_unchanged(
            &snapshot,
            std::collections::HashMap::from([(
                "description".to_owned(),
                "remote overwrite".to_owned(),
            )]),
        )
        .unwrap();
    assert!(result.is_none());
    assert_eq!(
        storage.get_issue("r-1").unwrap().unwrap().description,
        changed.description
    );
}
