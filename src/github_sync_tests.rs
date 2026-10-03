#![cfg(unix)]

use super::*;
use crate::github_ancestor::GithubAncestorStoreOutcome;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::SystemTime;

type LocalIssueId = String;
type GhCallLog = String;

const BASE_TITLE: &str = "Shared title";
const BASE_BODY: &str = "The opening paragraph stays here.\n\nThe middle paragraph stays here.\n\nThe closing paragraph stays here.";

#[derive(Clone, Copy)]
enum RemoteSlot {
    First,
    Second,
    Third,
}

impl RemoteSlot {
    fn url(self) -> &'static str {
        match self {
            Self::First => "https://github.com/example/repo/issues/1",
            Self::Second => "https://github.com/example/repo/issues/2",
            Self::Third => "https://github.com/example/repo/issues/3",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
            Self::Third => "third",
        }
    }
}

struct Fixture {
    temporary: tempfile::TempDir,
    storage: Storage,
}

#[derive(Debug, PartialEq, Eq)]
struct LocalSnapshot {
    issue: Value,
    comments: Vec<Comment>,
}

#[derive(Debug, PartialEq, Eq)]
struct ArchiveSnapshot {
    files: Vec<ArchiveFileSnapshot>,
}

#[derive(Debug, PartialEq, Eq)]
struct ArchiveFileSnapshot {
    name: String,
    bytes: Vec<u8>,
    modified: SystemTime,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let storage = Storage::init(
            temporary.path().join("store"),
            Some("sync-test".to_owned()),
            false,
            IssueStorageLayout::Flat,
        )
        .unwrap();
        Self { temporary, storage }
    }

    fn create_issue(&self, slot: RemoteSlot) -> Issue {
        let issue = self
            .storage
            .create_issue(
                BASE_TITLE.to_owned(),
                BASE_BODY.to_owned(),
                None,
                None,
                3,
                IssueType::Task,
                None,
                Vec::new(),
                Some(slot.url().to_owned()),
                None,
                Vec::new(),
            )
            .unwrap();
        self.current(&issue)
    }

    fn current(&self, issue: &Issue) -> Issue {
        self.storage.get_issue(&issue.id).unwrap().unwrap()
    }

    fn edit(&self, issue: &Issue, title: &str, body: &str, status: Status) -> Issue {
        self.storage
            .update_issue(
                &issue.id,
                HashMap::from([
                    ("title".to_owned(), title.to_owned()),
                    ("description".to_owned(), body.to_owned()),
                    ("status".to_owned(), status.to_string()),
                ]),
            )
            .unwrap();
        self.current(issue)
    }

    fn snapshot(&self, issue: &Issue) -> LocalSnapshot {
        LocalSnapshot {
            issue: serde_json::to_value(self.current(issue)).unwrap(),
            comments: self.storage.list_comments(&issue.id).unwrap(),
        }
    }

    fn archive_directory(&self) -> PathBuf {
        self.storage.get_beads_dir().join("sync_ancestors/github")
    }

    fn archive_snapshot(&self) -> Option<ArchiveSnapshot> {
        let directory = self.archive_directory();
        if !directory.try_exists().unwrap() {
            return None;
        }
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(directory).unwrap() {
            paths.push(entry.unwrap().path());
        }
        paths.sort();
        let mut files = Vec::new();
        for path in paths {
            files.push(ArchiveFileSnapshot {
                name: path.file_name().unwrap().to_string_lossy().into_owned(),
                bytes: std::fs::read(&path).unwrap(),
                modified: std::fs::metadata(path).unwrap().modified().unwrap(),
            });
        }
        Some(ArchiveSnapshot { files })
    }

    fn record_ancestor(&self, issue: &Issue, remote: &RemoteIssue) {
        let mut state = load_state(&self.storage.get_beads_dir()).unwrap();
        let comments = self.storage.list_comments(&issue.id).unwrap();
        update_state_entry(&mut state, issue, remote, &comments);
        save_state(&self.storage.get_beads_dir(), &state).unwrap();
        let remote_identity = CanonicalGithubIssue::parse(&remote.url).unwrap();
        let expected =
            crate::github_ancestor::load(&self.storage.get_beads_dir(), &remote_identity).unwrap();
        let record = GithubAncestorRecord::new(
            remote_identity.clone(),
            crate::github_ancestor::LocalIssueId::parse(issue.id.as_str()).unwrap(),
            fields_from_local(issue),
        );
        assert!(matches!(
            crate::github_ancestor::compare_and_store(
                &self.storage.get_beads_dir(),
                &remote_identity,
                expected.as_ref(),
                &record,
            )
            .unwrap(),
            GithubAncestorStoreOutcome::Stored | GithubAncestorStoreOutcome::Unchanged
        ));
    }

    fn sync(&self, mock: &MockGithub) -> Result<GithubSyncReport> {
        self.sync_filtered(mock, &[], GithubSyncFilter::default())
    }

    fn sync_filtered(
        &self,
        mock: &MockGithub,
        issue_ids: &[LocalIssueId],
        filter: GithubSyncFilter<'_>,
    ) -> Result<GithubSyncReport> {
        block_on_github(sync_linked_with_store(
            &self.storage,
            issue_ids,
            false,
            false,
            false,
            filter,
            &mock.store,
        ))
    }

    fn write_legacy(&self, state: &GithubSyncState) -> PathBuf {
        let mut legacy = serde_json::to_value(state).unwrap();
        legacy.as_object_mut().unwrap().remove("schema_version");
        for entry in legacy["issues"].as_object_mut().unwrap().values_mut() {
            entry.as_object_mut().unwrap().remove("ancestor");
        }
        let path = self.storage.get_beads_dir().join("github-sync-state.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        path
    }
}

struct MockGithub {
    directory: PathBuf,
    store: GithubStore,
}

impl MockGithub {
    fn new(fixture: &Fixture) -> Self {
        let directory = fixture.temporary.path().join("mock gh");
        std::fs::create_dir(&directory).unwrap();
        let program = directory.join("gh");
        let script = format!(
            r#"#!/bin/sh
set -eu
root={root}
printf '%s %s %s\n' "${{1-}}" "${{2-}}" "${{3-}}" >> "$root/calls"
case "${{3-}}" in
  {first}) slot=first ;;
  {second}) slot=second ;;
  {third}) slot=third ;;
  *) echo "unexpected issue: $*" >&2; exit 91 ;;
esac
case "$1 $2" in
  'issue view')
    if [ -f "$root/$slot.fail" ]; then
      echo 'injected offline read failure' >&2
      exit 92
    fi
    cat "$root/$slot.json"
    if [ -f "$root/$slot.next.json" ]; then
      mv "$root/$slot.next.json" "$root/$slot.json"
    fi
    ;;
  'issue edit')
    [ "$4" = '--title' ]
    [ "$6" = '--body' ]
    printf '%s' "$5" > "$root/$slot.actual-title"
    printf '%s' "$7" > "$root/$slot.actual-body"
    cmp "$root/$slot.expected-title" "$root/$slot.actual-title"
    cmp "$root/$slot.expected-body" "$root/$slot.actual-body"
    cp "$root/$slot.after.json" "$root/$slot.json"
    ;;
  *) echo "unexpected write or command: $*" >&2; exit 99 ;;
esac
"#,
            root = shell_quote_arg(&directory.to_string_lossy()),
            first = shell_quote_arg(RemoteSlot::First.url()),
            second = shell_quote_arg(RemoteSlot::Second.url()),
            third = shell_quote_arg(RemoteSlot::Third.url()),
        );
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let store = GithubStore::new_with_program(
            Some("example/repo"),
            program.to_string_lossy().into_owned(),
        );
        Self { directory, store }
    }

    fn path(&self, slot: RemoteSlot, suffix: &str) -> PathBuf {
        self.directory.join(format!("{}.{suffix}", slot.name()))
    }

    fn set_remote(&self, slot: RemoteSlot, remote: &RemoteIssue) {
        assert_eq!(slot.url(), remote.url);
        std::fs::write(self.path(slot, "json"), remote_json(remote)).unwrap();
    }

    fn expect_edit(&self, slot: RemoteSlot, expected: &RemoteIssue) {
        self.expect_edit_result(slot, expected, expected);
    }

    fn expect_edit_result(
        &self,
        slot: RemoteSlot,
        submitted: &RemoteIssue,
        persisted: &RemoteIssue,
    ) {
        std::fs::write(self.path(slot, "expected-title"), &submitted.title).unwrap();
        std::fs::write(self.path(slot, "expected-body"), &submitted.body).unwrap();
        std::fs::write(self.path(slot, "after.json"), remote_json(persisted)).unwrap();
    }

    fn change_after_view(&self, slot: RemoteSlot, remote: &RemoteIssue) {
        std::fs::write(self.path(slot, "next.json"), remote_json(remote)).unwrap();
    }

    fn fail_reads(&self, slot: RemoteSlot) {
        std::fs::write(self.path(slot, "fail"), b"").unwrap();
    }

    fn remote(&self, slot: RemoteSlot) -> RemoteIssue {
        parse_remote_issue(
            serde_json::from_slice(&std::fs::read(self.path(slot, "json")).unwrap()).unwrap(),
        )
        .unwrap()
    }

    fn calls(&self) -> GhCallLog {
        let path = self.directory.join("calls");
        if path.try_exists().unwrap() {
            std::fs::read_to_string(path).unwrap()
        } else {
            GhCallLog::new()
        }
    }

    fn assert_read_only(&self) {
        let calls = self.calls();
        assert!(!calls.is_empty());
        assert!(
            calls.lines().all(|call| call.starts_with("issue view ")),
            "{calls}"
        );
    }
}

fn remote_json(remote: &RemoteIssue) -> Vec<u8> {
    let mut comments = Vec::new();
    for comment in &remote.comments {
        comments.push(serde_json::json!({
            "id": comment.id,
            "url": comment.url,
            "author": { "login": comment.author },
            "body": comment.body,
            "createdAt": comment.created_at,
            "updatedAt": comment.updated_at,
        }));
    }
    serde_json::to_vec(&serde_json::json!({
        "url": remote.url,
        "title": remote.title,
        "body": remote.body,
        "state": remote.state,
        "comments": comments,
    }))
    .unwrap()
}

fn remote_comment_for(issue: &Issue, id: &str, body: &str) -> RemoteComment {
    RemoteComment {
        id: id.to_owned(),
        url: format!(
            "{}#issuecomment-{id}",
            issue.external_ref.as_deref().unwrap()
        ),
        author: "octo".to_owned(),
        body: body.to_owned(),
        created_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    }
}

fn remote_for(issue: &Issue) -> RemoteIssue {
    RemoteIssue {
        url: issue.external_ref.as_deref().unwrap().to_owned(),
        title: issue.title.to_owned(),
        body: issue.description.to_owned(),
        state: if issue.status == Status::Closed {
            "CLOSED"
        } else {
            "OPEN"
        }
        .to_owned(),
        comments: vec![remote_comment_for(
            issue,
            "marker",
            &format!("{MARKER} {}", issue.id),
        )],
    }
}

fn pending_comments(fixture: &Fixture, issue: &Issue, remote: &mut RemoteIssue) {
    fixture
        .storage
        .add_comment(&issue.id, "local", "Local work that must not be exported")
        .unwrap();
    fixture
        .storage
        .add_comment(&issue.id, "local", &format!("{MARKER} stale local marker"))
        .unwrap();
    remote.comments.push(remote_comment_for(
        issue,
        "101",
        "Remote work that must not be imported",
    ));
}

fn assert_conflict(report: &GithubSyncReport) {
    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0].conflict.is_some());
    assert_eq!(report.pushed_issues, 0);
    assert_eq!(report.pulled_issues, 0);
    assert_eq!(report.imported_comments, 0);
    assert_eq!(report.exported_comments, 0);
    assert_eq!(report.deleted_local_comments, 0);
    assert_eq!(report.deleted_remote_comments, 0);
}

fn assert_common_ancestor(fixture: &Fixture, issue: &Issue, remote: &RemoteIssue) {
    let state = load_state(&fixture.storage.get_beads_dir()).unwrap();
    let local = fixture.current(issue);
    let entry = &state.issues[&remote.url];
    let remote_identity = CanonicalGithubIssue::parse(&remote.url).unwrap();
    let ancestor = crate::github_ancestor::load(&fixture.storage.get_beads_dir(), &remote_identity)
        .unwrap()
        .unwrap();
    let expected = fields_from_local(&local);
    assert_eq!(entry.local_id, issue.id);
    assert_eq!(ancestor.local_id().as_str(), issue.id);
    assert_eq!(ancestor.common(), &expected);
    assert_eq!(entry.local_hash, hash_local_issue(&local));
    assert_eq!(entry.remote_hash, hash_remote_issue(remote));
    assert_eq!(entry.local_hash, entry.remote_hash);
}

#[test]
fn independent_local_title_and_remote_body_merge_and_remain_idempotent() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    fixture.edit(&issue, "Local title survives", BASE_BODY, Status::Open);
    remote.body = "Remote body survives.\n\nIncluding a second paragraph.".to_owned();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    remote.title = "Local title survives".to_owned();
    mock.expect_edit(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pushed_issues, 1);
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(fixture.current(&issue).title, remote.title);
    assert_eq!(fixture.current(&issue).description, remote.body);
    assert_common_ancestor(&fixture, &issue, &mock.remote(RemoteSlot::First));
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();
    let fresh_store =
        GithubStore::new_with_program(Some("example/repo"), mock.store.inner.program.as_str());

    let repeated = block_on_github(sync_linked_with_store(
        &fixture.storage,
        &[],
        false,
        false,
        false,
        GithubSyncFilter::default(),
        &fresh_store,
    ))
    .unwrap();

    assert!(repeated.conflicts.is_empty(), "{repeated:?}");
    assert_eq!(repeated.pushed_issues, 0);
    assert_eq!(repeated.pulled_issues, 0);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    assert_eq!(
        mock.calls()
            .lines()
            .filter(|call| call.starts_with("issue edit "))
            .count(),
        1
    );
}

#[test]
fn nonoverlapping_body_prose_edits_merge_without_losing_either_side() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    let local_body = BASE_BODY.replace(
        "opening paragraph stays",
        "opening paragraph changes locally",
    );
    fixture.edit(&issue, BASE_TITLE, &local_body, Status::Open);
    remote.body = BASE_BODY.replace(
        "closing paragraph stays",
        "closing paragraph changes remotely",
    );
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    remote.body = local_body.replace(
        "closing paragraph stays",
        "closing paragraph changes remotely",
    );
    mock.expect_edit(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pushed_issues, 1);
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(fixture.current(&issue).description, remote.body);
    assert_eq!(mock.remote(RemoteSlot::First).body, remote.body);
    assert_common_ancestor(&fixture, &issue, &mock.remote(RemoteSlot::First));
}

#[test]
fn retry_after_remote_write_before_local_checkpoint_keeps_paragraphs_once() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    fixture.edit(
        &issue,
        BASE_TITLE,
        &format!("{BASE_BODY}\n\nLocal addition."),
        Status::Open,
    );
    remote.body = format!("{BASE_BODY}\n\nLocal addition.\n\nRemote addition.");
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pushed_issues, 0);
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(fixture.current(&issue).description, remote.body);
    assert_common_ancestor(&fixture, &issue, &remote);
    mock.assert_read_only();
}

#[derive(Clone, Copy)]
enum ConflictingEdit {
    Title,
    Body,
}

fn check_field_conflict(edit: ConflictingEdit) {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    match edit {
        ConflictingEdit::Title => {
            fixture.edit(&issue, "Local replacement title", BASE_BODY, Status::Open);
            remote.title = "Remote replacement title".to_owned();
        }
        ConflictingEdit::Body => {
            fixture.edit(&issue, BASE_TITLE, "Local replacement body.", Status::Open);
            remote.body = "Remote replacement body.".to_owned();
        }
    }
    pending_comments(&fixture, &issue, &mut remote);
    remote
        .comments
        .retain(|comment| !is_marker_comment(comment));
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();

    let report = block_on_github(sync_linked_with_store(
        &fixture.storage,
        &[],
        false,
        false,
        true,
        GithubSyncFilter::default(),
        &mock.store,
    ))
    .unwrap();

    assert_conflict(&report);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    assert_eq!(
        remote_json(&mock.remote(RemoteSlot::First)),
        remote_json(&remote)
    );
    mock.assert_read_only();
}

#[test]
fn conflicting_titles_do_not_mutate_fields_comments_or_ancestor_even_with_force() {
    check_field_conflict(ConflictingEdit::Title);
}

#[test]
fn conflicting_bodies_do_not_mutate_fields_comments_or_ancestor_even_with_force() {
    check_field_conflict(ConflictingEdit::Body);
}

#[test]
fn field_conflict_blocks_comment_deletions_on_both_sides_even_with_force() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    remote
        .comments
        .push(remote_comment_for(&issue, "101", "Deleted only locally"));
    remote
        .comments
        .push(remote_comment_for(&issue, "102", "Deleted only remotely"));
    assert_eq!(
        import_remote_comments(&fixture.storage, &issue, &remote).unwrap(),
        2
    );
    fixture.record_ancestor(&issue, &remote);
    let comments = fixture.storage.list_comments(&issue.id).unwrap();
    let deleted_local = comments
        .iter()
        .find(|comment| comment.source_id.as_deref() == Some("101"))
        .unwrap();
    fixture
        .storage
        .delete_comment(&issue.id, &deleted_local.id)
        .unwrap();
    remote.comments.retain(|comment| comment.id != "102");
    fixture.edit(&issue, "Local conflicting title", BASE_BODY, Status::Open);
    remote.title = "Remote conflicting title".to_owned();
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);

    let report = block_on_github(sync_linked_with_store(
        &fixture.storage,
        &[],
        false,
        false,
        true,
        GithubSyncFilter::default(),
        &mock.store,
    ))
    .unwrap();

    assert_conflict(&report);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    assert_eq!(
        remote_json(&mock.remote(RemoteSlot::First)),
        remote_json(&remote)
    );
    mock.assert_read_only();
}

#[test]
fn missing_ancestor_refuses_divergence_without_creating_an_archive() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    remote.title = "Unrelated remote title".to_owned();
    remote.body = "Unrelated remote body".to_owned();
    pending_comments(&fixture, &issue, &mut remote);
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    let local_before = fixture.snapshot(&issue);

    let report = fixture.sync(&mock).unwrap();

    assert_conflict(&report);
    assert_eq!(report.issues[0].action, "conflict-no-ancestor");
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert!(fixture.archive_snapshot().is_none());
    mock.assert_read_only();
}

#[test]
fn dry_run_three_way_merge_does_not_write_fields_comments_remote_or_archive() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    fixture.edit(&issue, "Dry-run local title", BASE_BODY, Status::Open);
    remote.body = "Dry-run remote body".to_owned();
    pending_comments(&fixture, &issue, &mut remote);
    remote
        .comments
        .retain(|comment| !is_marker_comment(comment));
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();

    let report = block_on_github(sync_linked_with_store(
        &fixture.storage,
        &[],
        true,
        false,
        false,
        GithubSyncFilter::default(),
        &mock.store,
    ))
    .unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pushed_issues, 1);
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(report.imported_comments, 1);
    assert_eq!(report.exported_comments, 1);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    assert_eq!(
        remote_json(&mock.remote(RemoteSlot::First)),
        remote_json(&remote)
    );
    mock.assert_read_only();
}

#[test]
fn dry_run_forced_initial_pull_does_not_create_an_archive_or_import_comments() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    remote.body = "Explicitly selected remote bootstrap".to_owned();
    pending_comments(&fixture, &issue, &mut remote);
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    let local_before = fixture.snapshot(&issue);

    let report = block_on_github(sync_linked_with_store(
        &fixture.storage,
        &[],
        true,
        true,
        true,
        GithubSyncFilter::default(),
        &mock.store,
    ))
    .unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(report.pushed_issues, 0);
    assert_eq!(report.exported_comments, 0);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert!(fixture.archive_snapshot().is_none());
    mock.assert_read_only();
}

#[test]
fn labels_require_all_matches_and_intersect_explicit_issue_ids() {
    let fixture = Fixture::new();
    let selected = fixture.create_issue(RemoteSlot::First);
    let missing_label = fixture.create_issue(RemoteSlot::Second);
    let outside_ids = fixture.create_issue(RemoteSlot::Third);
    for issue in [&selected, &missing_label, &outside_ids] {
        fixture.storage.add_label(&issue.id, "sync").unwrap();
        fixture.record_ancestor(issue, &remote_for(issue));
    }
    for issue in [&selected, &outside_ids] {
        fixture.storage.add_label(&issue.id, "team").unwrap();
    }
    let selected = fixture.current(&selected);
    let missing_before = fixture.snapshot(&missing_label);
    let outside_before = fixture.snapshot(&outside_ids);
    let state_before = load_state(&fixture.storage.get_beads_dir()).unwrap();
    let mut remote = remote_for(&selected);
    remote.body = "Only the selected issue is pulled".to_owned();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    let labels: [GithubSyncLabel; 2] = ["sync".parse().unwrap(), "team".parse().unwrap()];
    let issue_ids = [selected.id.to_owned(), missing_label.id.to_owned()];

    let report = fixture
        .sync_filtered(
            &mock,
            &issue_ids,
            GithubSyncFilter {
                labels: &labels,
                since: None,
            },
        )
        .unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].issue_id, selected.id);
    assert_eq!(report.pulled_issues, 1);
    assert_eq!(fixture.current(&selected).description, remote.body);
    assert_eq!(fixture.snapshot(&missing_label), missing_before);
    assert_eq!(fixture.snapshot(&outside_ids), outside_before);
    let state_after = load_state(&fixture.storage.get_beads_dir()).unwrap();
    for slot in [RemoteSlot::Second, RemoteSlot::Third] {
        assert_eq!(
            state_after.issues[slot.url()],
            state_before.issues[slot.url()]
        );
    }
    assert_common_ancestor(&fixture, &selected, &remote);
    mock.assert_read_only();
    assert!(mock
        .calls()
        .lines()
        .all(|call| call.ends_with(RemoteSlot::First.url())));
}

#[test]
fn empty_label_selection_makes_no_gh_calls_and_creates_no_archive() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let local_before = fixture.snapshot(&issue);
    let mock = MockGithub::new(&fixture);
    let labels: [GithubSyncLabel; 1] = ["absent".parse().unwrap()];

    let report = fixture
        .sync_filtered(
            &mock,
            &[],
            GithubSyncFilter {
                labels: &labels,
                since: None,
            },
        )
        .unwrap();

    assert!(report.issues.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(mock.calls().is_empty());
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert!(fixture.archive_snapshot().is_none());
}

#[test]
fn empty_label_id_intersection_preserves_all_existing_archive_entries() {
    let fixture = Fixture::new();
    let labeled = fixture.create_issue(RemoteSlot::First);
    let requested = fixture.create_issue(RemoteSlot::Second);
    fixture.storage.add_label(&labeled.id, "sync").unwrap();
    for issue in [&labeled, &requested] {
        fixture.record_ancestor(issue, &remote_for(issue));
    }
    let labeled_before = fixture.snapshot(&labeled);
    let requested_before = fixture.snapshot(&requested);
    let archive_before = fixture.archive_snapshot();
    let mock = MockGithub::new(&fixture);
    let labels: [GithubSyncLabel; 1] = ["sync".parse().unwrap()];

    let report = fixture
        .sync_filtered(
            &mock,
            std::slice::from_ref(&requested.id),
            GithubSyncFilter {
                labels: &labels,
                since: None,
            },
        )
        .unwrap();

    assert!(report.issues.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(mock.calls().is_empty());
    assert_eq!(fixture.snapshot(&labeled), labeled_before);
    assert_eq!(fixture.snapshot(&requested), requested_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
}

#[test]
fn since_filter_excludes_older_local_issues_without_gh_calls_or_archive_creation() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let local_before = fixture.snapshot(&issue);
    let mock = MockGithub::new(&fixture);

    let report = fixture
        .sync_filtered(
            &mock,
            std::slice::from_ref(&issue.id),
            GithubSyncFilter {
                labels: &[],
                since: Some(issue.updated_at + chrono::Duration::days(1)),
            },
        )
        .unwrap();

    assert!(report.issues.is_empty());
    assert!(mock.calls().is_empty());
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert!(fixture.archive_snapshot().is_none());
}

#[test]
fn since_filter_includes_the_exact_local_updated_at_boundary() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let remote = remote_for(&issue);
    let local_before = fixture.snapshot(&issue);
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);

    let report = fixture
        .sync_filtered(
            &mock,
            std::slice::from_ref(&issue.id),
            GithubSyncFilter {
                labels: &[],
                since: Some(issue.updated_at),
            },
        )
        .unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].issue_id, issue.id);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_common_ancestor(&fixture, &issue, &remote);
    mock.assert_read_only();
}

#[test]
fn remote_open_preserves_in_progress_status_during_full_and_pull_only_sync() {
    for pull_only in [false, true] {
        let fixture = Fixture::new();
        let issue = fixture.create_issue(RemoteSlot::First);
        let mut remote = remote_for(&issue);
        fixture.record_ancestor(&issue, &remote);
        fixture.edit(&issue, BASE_TITLE, BASE_BODY, Status::InProgress);
        remote.body = "Remote body update while local work is in progress".to_owned();
        let mock = MockGithub::new(&fixture);
        mock.set_remote(RemoteSlot::First, &remote);

        let report = block_on_github(sync_linked_with_store(
            &fixture.storage,
            &[],
            false,
            pull_only,
            false,
            GithubSyncFilter::default(),
            &mock.store,
        ))
        .unwrap();

        assert!(report.conflicts.is_empty(), "{report:?}");
        assert_eq!(report.pulled_issues, 1);
        assert_eq!(report.pushed_issues, 0);
        assert_eq!(fixture.current(&issue).status, Status::InProgress);
        assert_eq!(fixture.current(&issue).description, remote.body);
        assert_common_ancestor(&fixture, &issue, &remote);
        mock.assert_read_only();
    }
}

#[test]
fn missing_archive_loads_empty_current_schema_without_writing_files() {
    let fixture = Fixture::new();

    let state = load_state(&fixture.storage.get_beads_dir()).unwrap();
    let remote = CanonicalGithubIssue::parse(RemoteSlot::First.url()).unwrap();

    assert!(state.issues.is_empty());
    assert!(
        crate::github_ancestor::load(&fixture.storage.get_beads_dir(), &remote)
            .unwrap()
            .is_none()
    );
    assert!(fixture.archive_snapshot().is_none());
    let ignore =
        std::fs::read_to_string(fixture.storage.get_beads_dir().join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|entry| entry == "sync_ancestors/"));
}

#[test]
fn legacy_common_hashes_support_one_sided_push_and_migrate_to_content_ancestor() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    let mut state = GithubSyncState::default();
    update_state_entry(&mut state, &issue, &remote, &[]);
    let legacy_path = fixture.write_legacy(&state);
    let remote_identity = CanonicalGithubIssue::parse(&remote.url).unwrap();
    assert!(
        crate::github_ancestor::load(&fixture.storage.get_beads_dir(), &remote_identity)
            .unwrap()
            .is_none()
    );
    assert!(fixture.archive_snapshot().is_none());
    fixture.edit(
        &issue,
        "Local edit after legacy sync",
        BASE_BODY,
        Status::Open,
    );
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    remote.title = "Local edit after legacy sync".to_owned();
    mock.expect_edit(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.pushed_issues, 1);
    assert_eq!(report.pulled_issues, 0);
    assert_common_ancestor(&fixture, &issue, &mock.remote(RemoteSlot::First));
    let updated_state = load_state(legacy_path.parent().unwrap()).unwrap();
    let entry = &updated_state.issues[&remote.url];
    assert_eq!(entry.local_hash, entry.remote_hash);
}

#[test]
fn legacy_divergent_hashes_never_become_a_fabricated_common_ancestor() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    remote.body = "Already divergent at the legacy sync".to_owned();
    let mut state = GithubSyncState::default();
    update_state_entry(&mut state, &issue, &remote, &[]);
    let legacy_path = fixture.write_legacy(&state);
    let legacy_before = std::fs::read(&legacy_path).unwrap();
    fixture.edit(&issue, "New local work", BASE_BODY, Status::Open);
    pending_comments(&fixture, &issue, &mut remote);
    let local_before = fixture.snapshot(&issue);
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert_conflict(&report);
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert!(fixture.archive_snapshot().is_none());
    assert_eq!(std::fs::read(legacy_path).unwrap(), legacy_before);
    mock.assert_read_only();
}

#[test]
fn archive_entry_for_a_different_local_id_cannot_authorize_a_push() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    let remote_identity = CanonicalGithubIssue::parse(&remote.url).unwrap();
    let expected =
        crate::github_ancestor::load(&fixture.storage.get_beads_dir(), &remote_identity).unwrap();
    let wrong = GithubAncestorRecord::new(
        remote_identity.clone(),
        crate::github_ancestor::LocalIssueId::parse("another-issue-999").unwrap(),
        fields_from_local(&issue),
    );
    crate::github_ancestor::compare_and_store(
        &fixture.storage.get_beads_dir(),
        &remote_identity,
        expected.as_ref(),
        &wrong,
    )
    .unwrap();
    fixture.edit(
        &issue,
        "New owner of this GitHub link",
        BASE_BODY,
        Status::Open,
    );
    pending_comments(&fixture, &issue, &mut remote);
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);

    let report = fixture.sync(&mock).unwrap();

    assert_conflict(&report);
    assert_eq!(report.issues[0].action, "conflict-local-id");
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    mock.assert_read_only();
}

#[test]
fn remote_edit_between_initial_read_and_pre_push_refresh_is_not_overwritten() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    fixture.edit(
        &issue,
        "Local title waiting to push",
        BASE_BODY,
        Status::Open,
    );
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    remote.body = "Concurrent remote work".to_owned();
    mock.change_after_view(RemoteSlot::First, &remote);
    let local_before = fixture.snapshot(&issue);
    let archive_before = fixture.archive_snapshot();

    let report = fixture.sync(&mock).unwrap();

    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    assert_eq!(report.issues[0].action, "conflict-remote-edit");
    assert_eq!(fixture.snapshot(&issue), local_before);
    assert_eq!(fixture.archive_snapshot(), archive_before);
    assert_eq!(mock.remote(RemoteSlot::First).body, remote.body);
    mock.assert_read_only();
    assert_eq!(mock.calls().lines().count(), 2);
}

#[test]
fn post_push_remote_edit_is_reconciled_before_checkpointing() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let remote = remote_for(&issue);
    fixture.record_ancestor(&issue, &remote);
    let local = fixture.edit(
        &issue,
        "Title successfully submitted",
        BASE_BODY,
        Status::Open,
    );
    let submitted = remote_for(&local);
    let mut persisted = remote_for(&local);
    persisted.body = "Concurrent remote edit after our write".to_owned();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &remote);
    mock.expect_edit_result(RemoteSlot::First, &submitted, &persisted);

    let report = fixture.sync(&mock).unwrap();

    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].action, "merged");
    assert_eq!(fixture.current(&issue).title, persisted.title);
    assert_eq!(fixture.current(&issue).description, persisted.body);
    assert_common_ancestor(&fixture, &fixture.current(&issue), &persisted);
    assert_eq!(mock.remote(RemoteSlot::First).body, persisted.body);
    let calls = mock.calls();
    assert_eq!(
        calls
            .lines()
            .filter(|call| call.starts_with("issue edit "))
            .count(),
        1,
        "{calls}"
    );
    assert_eq!(
        calls
            .lines()
            .filter(|call| call.starts_with("issue view "))
            .count(),
        6,
        "{calls}"
    );
}

#[test]
fn successful_issue_progress_is_archived_before_a_later_issue_fetch_fails() {
    let fixture = Fixture::new();
    let first = fixture.create_issue(RemoteSlot::First);
    let second = fixture.create_issue(RemoteSlot::Second);
    let mut first_remote = remote_for(&first);
    fixture.record_ancestor(&first, &first_remote);
    fixture.record_ancestor(&second, &remote_for(&second));
    let state_before = load_state(&fixture.storage.get_beads_dir()).unwrap();
    let second_before = fixture.snapshot(&second);
    first_remote.body = "Successfully pulled before the later failure".to_owned();
    let mock = MockGithub::new(&fixture);
    mock.set_remote(RemoteSlot::First, &first_remote);
    mock.fail_reads(RemoteSlot::Second);

    let error = fixture.sync(&mock).unwrap_err();

    assert!(
        format!("{error:#}").contains("injected offline read failure"),
        "{error:#}"
    );
    assert_eq!(fixture.current(&first).description, first_remote.body);
    assert_common_ancestor(&fixture, &first, &first_remote);
    assert_eq!(fixture.snapshot(&second), second_before);
    let state_after = load_state(&fixture.storage.get_beads_dir()).unwrap();
    assert_eq!(
        state_after.issues[RemoteSlot::Second.url()],
        state_before.issues[RemoteSlot::Second.url()]
    );
    mock.assert_read_only();
    assert_eq!(mock.calls().lines().count(), 4);
}

#[test]
fn hash_state_preserves_noop_timestamp_and_records_divergence() {
    let fixture = Fixture::new();
    let issue = fixture.create_issue(RemoteSlot::First);
    let mut remote = remote_for(&issue);
    let mut state = GithubSyncState::default();
    update_state_entry(&mut state, &issue, &remote, &[]);
    let previous = serde_json::to_value(&state).unwrap();

    update_state_entry(&mut state, &issue, &remote, &[]);

    assert_eq!(serde_json::to_value(&state).unwrap(), previous);
    remote.body = "No longer a common ancestor".to_owned();

    update_state_entry(&mut state, &issue, &remote, &[]);

    let entry = &state.issues[&remote.url];
    assert_ne!(entry.local_hash, entry.remote_hash);
}
