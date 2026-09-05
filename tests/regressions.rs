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
