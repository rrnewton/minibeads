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
