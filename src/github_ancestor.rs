//! Versioned common-ancestor checkpoints for GitHub issue synchronization.

use crate::lock::Lock;
use crate::paths::{ensure_contained, IssueId};
use crate::transaction::FileTransaction;
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

const ANCESTOR_DIRECTORY: &str = "sync_ancestors/github";
const ANCESTOR_KEY_PREFIX: &str = "sha256-";
const SYNC_LEASE_DIRECTORY: &str = "sync_ancestors/github-leases";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct AncestorSchemaVersion(u64);

impl AncestorSchemaVersion {
    const CURRENT: Self = Self(1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GithubIssueNumber(NonZeroU64);

impl GithubIssueNumber {
    fn parse(value: &str) -> Result<Self> {
        let number = value
            .parse::<NonZeroU64>()
            .with_context(|| format!("Invalid GitHub issue number {value:?}"))?;
        Ok(Self(number))
    }
}

impl std::fmt::Display for GithubIssueNumber {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A canonical identity for one issue in GitHub's public issue namespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CanonicalGithubIssue(Box<str>);

impl CanonicalGithubIssue {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let path = value.strip_prefix("https://github.com/").with_context(|| {
            format!("Invalid GitHub issue URL {value:?}: expected https://github.com/")
        })?;
        let mut parts = path.split('/');
        let owner = parts.next().unwrap_or_default();
        let repository = parts.next().unwrap_or_default();
        let issues = parts.next().unwrap_or_default();
        let issue_number = parts.next().unwrap_or_default();
        anyhow::ensure!(
            parts.next().is_none() && issues == "issues",
            "Invalid GitHub issue URL {value:?}: expected owner/repository/issues/number"
        );
        validate_owner(owner)?;
        validate_repository(repository)?;
        let issue_number = GithubIssueNumber::parse(issue_number)?;
        let canonical = format!(
            "https://github.com/{}/{}/issues/{issue_number}",
            owner.to_ascii_lowercase(),
            repository.to_ascii_lowercase()
        );
        Ok(Self(canonical.into_boxed_str()))
    }

    pub(crate) fn as_url(&self) -> &str {
        &self.0
    }
}

impl Serialize for CanonicalGithubIssue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_url())
    }
}

impl<'de> Deserialize<'de> for CanonicalGithubIssue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Box::<str>::deserialize(deserializer)?;
        let identity = Self::parse(&value).map_err(serde::de::Error::custom)?;
        if identity.as_url() != value.as_ref() {
            return Err(serde::de::Error::custom(format!(
                "GitHub issue URL is not canonical: {value:?}"
            )));
        }
        Ok(identity)
    }
}

fn validate_owner(owner: &str) -> Result<()> {
    anyhow::ensure!(
        !owner.is_empty()
            && owner.len() <= 39
            && !owner.starts_with('-')
            && !owner.ends_with('-')
            && owner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "Invalid GitHub owner {owner:?}"
    );
    Ok(())
}

fn validate_repository(repository: &str) -> Result<()> {
    anyhow::ensure!(
        !repository.is_empty()
            && repository.len() <= 100
            && !matches!(repository, "." | "..")
            && repository
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') }),
        "Invalid GitHub repository {repository:?}"
    );
    Ok(())
}

/// A validated local issue identifier retained in an ancestor checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalIssueId(Box<str>);

impl LocalIssueId {
    pub(crate) fn parse(value: impl Into<Box<str>>) -> Result<Self> {
        let value = value.into();
        IssueId::parse(&value)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for LocalIssueId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LocalIssueId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Box::<str>::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// The title shared by the local and remote issue at the checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct CommonIssueTitle(Box<str>);

impl CommonIssueTitle {
    pub(crate) fn new(value: impl Into<Box<str>>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The body shared by the local and remote issue at the checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct CommonIssueBody(Box<str>);

impl CommonIssueBody {
    pub(crate) fn new(value: impl Into<Box<str>>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// GitHub's two-state issue lifecycle represented at a common checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommonIssueStatus {
    Open,
    Closed,
}

/// Fields synchronized through the common-ancestor protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommonIssueFields {
    title: CommonIssueTitle,
    body: CommonIssueBody,
    status: CommonIssueStatus,
}

impl CommonIssueFields {
    pub(crate) fn new(
        title: CommonIssueTitle,
        body: CommonIssueBody,
        status: CommonIssueStatus,
    ) -> Self {
        Self {
            title,
            body,
            status,
        }
    }

    pub(crate) fn title(&self) -> &CommonIssueTitle {
        &self.title
    }

    pub(crate) fn body(&self) -> &CommonIssueBody {
        &self.body
    }

    pub(crate) fn status(&self) -> CommonIssueStatus {
        self.status
    }
}

pub(crate) type AncestorFields = CommonIssueFields;

/// One durable common-ancestor checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GithubAncestorRecord {
    remote: CanonicalGithubIssue,
    local_id: LocalIssueId,
    common: AncestorFields,
}

impl GithubAncestorRecord {
    pub(crate) fn new(
        remote: CanonicalGithubIssue,
        local_id: LocalIssueId,
        common: AncestorFields,
    ) -> Self {
        Self {
            remote,
            local_id,
            common,
        }
    }

    pub(crate) fn remote(&self) -> &CanonicalGithubIssue {
        &self.remote
    }

    pub(crate) fn local_id(&self) -> &LocalIssueId {
        &self.local_id
    }

    pub(crate) fn common(&self) -> &AncestorFields {
        &self.common
    }

    /// Derive the next checkpoint for an ID migration while retaining this value
    /// as the expected side of `compare_and_store`.
    #[cfg(test)]
    pub(crate) fn with_local_id(&self, local_id: LocalIssueId) -> Self {
        Self {
            remote: self.remote.clone(),
            local_id,
            common: self.common.clone(),
        }
    }
}

pub(crate) type AncestorRecord = GithubAncestorRecord;

/// An exclusive project-local lease for one canonical GitHub issue.
///
/// The underlying lock is rooted in an issue-specific directory rather than at
/// the database root. Acquiring it therefore neither holds the coarse database
/// lock nor inspects or recovers the database transaction journal.
pub(crate) struct GithubIssueSyncLease {
    _lock: Lock,
}

/// Serialize mutating synchronization of one canonical GitHub issue.
///
/// Different issues use different content-derived lease directories and can
/// proceed concurrently. The returned guard releases the lease on drop.
pub(crate) fn acquire_sync_lease(
    database_root: &Path,
    remote: &CanonicalGithubIssue,
) -> Result<GithubIssueSyncLease> {
    let directory = sync_lease_directory(database_root, remote)?;
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "Failed to create GitHub sync lease directory {}",
            directory.display()
        )
    })?;
    ensure_contained(database_root, &directory)?;
    let lock = Lock::acquire_without_recovery(&directory).with_context(|| {
        format!(
            "Failed to acquire GitHub sync lease for {}",
            remote.as_url()
        )
    })?;
    Ok(GithubIssueSyncLease { _lock: lock })
}

/// Result of a compare-and-store publication attempt.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubAncestorStoreOutcome {
    Stored,
    Unchanged,
    ConcurrentChange(Option<GithubAncestorRecord>),
}

/// Result of validating and staging a compare-and-store operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubAncestorStageOutcome {
    Staged,
    Unchanged,
    ConcurrentChange(Option<GithubAncestorRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GithubAncestorKey([u8; 32]);

impl GithubAncestorKey {
    fn from_remote(remote: &CanonicalGithubIssue) -> Self {
        let digest = Sha256::digest(remote.as_url().as_bytes());
        Self(digest.into())
    }

    fn file_name(&self) -> String {
        let mut name = String::with_capacity(ANCESTOR_KEY_PREFIX.len() + self.0.len() * 2 + 5);
        name.push_str(ANCESTOR_KEY_PREFIX);
        for byte in self.0 {
            write!(&mut name, "{byte:02x}").expect("writing to String cannot fail");
        }
        name.push_str(".json");
        name
    }
}

#[derive(Deserialize)]
struct SchemaHeader {
    schema_version: AncestorSchemaVersion,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionOneRecord {
    schema_version: AncestorSchemaVersion,
    remote: CanonicalGithubIssue,
    local_id: LocalIssueId,
    common: CommonIssueFields,
}

impl From<VersionOneRecord> for GithubAncestorRecord {
    fn from(value: VersionOneRecord) -> Self {
        Self {
            remote: value.remote,
            local_id: value.local_id,
            common: value.common,
        }
    }
}

/// Load one ancestor under the database lock without recovering or writing state.
/// `None` means no versioned per-issue ancestor exists; legacy hash state is not
/// consulted or interpreted as a common-content checkpoint.
pub(crate) fn load(
    database_root: &Path,
    remote_url: &CanonicalGithubIssue,
) -> Result<Option<AncestorRecord>> {
    let _lock = Lock::acquire_without_recovery(database_root)?;
    load_without_lock(database_root, remote_url)
}

/// Load while the caller already holds the database lock.
pub(crate) fn load_under_lock(
    database_root: &Path,
    remote_url: &CanonicalGithubIssue,
) -> Result<Option<AncestorRecord>> {
    load_without_lock(database_root, remote_url)
}

/// Publish one checkpoint only if its previously loaded snapshot is still current.
/// Preview callers can construct `next` and compare it with `expected` without
/// invoking this write API.
#[cfg(test)]
pub(crate) fn compare_and_store(
    database_root: &Path,
    remote_url: &CanonicalGithubIssue,
    expected: Option<&AncestorRecord>,
    next: &AncestorRecord,
) -> Result<GithubAncestorStoreOutcome> {
    let _lock = Lock::acquire(database_root)?;
    compare_and_store_under_lock(database_root, remote_url, expected, next)
}

/// Compare and publish while the caller holds the database lock. This lets a
/// checked local issue snapshot and its ancestor checkpoint share one critical
/// section without recursively acquiring the coarse database lock.
#[cfg(test)]
pub(crate) fn compare_and_store_under_lock(
    database_root: &Path,
    remote_url: &CanonicalGithubIssue,
    expected: Option<&AncestorRecord>,
    next: &AncestorRecord,
) -> Result<GithubAncestorStoreOutcome> {
    let mut transaction = FileTransaction::new(database_root);
    match compare_and_stage_under_lock(database_root, remote_url, expected, next, &mut transaction)?
    {
        GithubAncestorStageOutcome::Staged => {
            transaction.commit()?;
            Ok(GithubAncestorStoreOutcome::Stored)
        }
        GithubAncestorStageOutcome::Unchanged => Ok(GithubAncestorStoreOutcome::Unchanged),
        GithubAncestorStageOutcome::ConcurrentChange(current) => {
            Ok(GithubAncestorStoreOutcome::ConcurrentChange(current))
        }
    }
}

/// Validate one ancestor compare-and-store and stage its write without
/// committing the caller's transaction.
///
/// The caller must hold the coarse database lock from the comparison through
/// the eventual transaction commit. This allows the ancestor and related
/// legacy state to become one recoverable multi-file publication.
pub(crate) fn compare_and_stage_under_lock(
    database_root: &Path,
    remote_url: &CanonicalGithubIssue,
    expected: Option<&AncestorRecord>,
    next: &AncestorRecord,
    transaction: &mut FileTransaction<'_>,
) -> Result<GithubAncestorStageOutcome> {
    ensure_same_remote(remote_url, next.remote())?;
    if let Some(expected) = expected {
        ensure_same_remote(remote_url, expected.remote())?;
    }
    let current = load_without_lock(database_root, remote_url)?;
    if current.as_ref() == Some(next) {
        return Ok(GithubAncestorStageOutcome::Unchanged);
    }
    if current.as_ref() != expected {
        return Ok(GithubAncestorStageOutcome::ConcurrentChange(current));
    }

    let path = ancestor_path(database_root, remote_url)?;
    let content = serialize_record(next)?;
    transaction.write(path, content);
    Ok(GithubAncestorStageOutcome::Staged)
}

fn load_without_lock(
    database_root: &Path,
    remote: &CanonicalGithubIssue,
) -> Result<Option<AncestorRecord>> {
    let expected_key = GithubAncestorKey::from_remote(remote);
    let path = ancestor_path_for_key(database_root, &expected_key)?;
    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to read GitHub ancestor {}", path.display()));
        }
    };
    let header: SchemaHeader = serde_json::from_slice(&content)
        .with_context(|| format!("Corrupt GitHub ancestor {}", path.display()))?;
    if header.schema_version.0 > AncestorSchemaVersion::CURRENT.0 {
        anyhow::bail!(
            "GitHub ancestor {} uses future schema version {} (current version is {})",
            path.display(),
            header.schema_version.0,
            AncestorSchemaVersion::CURRENT.0
        );
    }
    anyhow::ensure!(
        header.schema_version == AncestorSchemaVersion::CURRENT,
        "GitHub ancestor {} uses unsupported schema version {}",
        path.display(),
        header.schema_version.0
    );
    let wire: VersionOneRecord = serde_json::from_slice(&content)
        .with_context(|| format!("Corrupt GitHub ancestor {}", path.display()))?;
    anyhow::ensure!(
        wire.schema_version == AncestorSchemaVersion::CURRENT,
        "GitHub ancestor schema changed while parsing {}",
        path.display()
    );
    let stored_key = GithubAncestorKey::from_remote(&wire.remote);
    anyhow::ensure!(
        stored_key == expected_key && wire.remote == *remote,
        "GitHub ancestor URL/key mismatch in {}: expected {}, found {}",
        path.display(),
        remote.as_url(),
        wire.remote.as_url()
    );
    Ok(Some(wire.into()))
}

fn serialize_record(record: &GithubAncestorRecord) -> Result<Vec<u8>> {
    let wire = VersionOneRecord {
        schema_version: AncestorSchemaVersion::CURRENT,
        remote: record.remote.clone(),
        local_id: record.local_id.clone(),
        common: record.common.clone(),
    };
    let mut content = serde_json::to_vec_pretty(&wire)
        .context("Failed to serialize GitHub ancestor checkpoint")?;
    content.push(b'\n');
    Ok(content)
}

fn ancestor_path(database_root: &Path, remote: &CanonicalGithubIssue) -> Result<PathBuf> {
    ancestor_path_for_key(database_root, &GithubAncestorKey::from_remote(remote))
}

fn ancestor_path_for_key(database_root: &Path, key: &GithubAncestorKey) -> Result<PathBuf> {
    let path = database_root.join(ANCESTOR_DIRECTORY).join(key.file_name());
    ensure_contained(database_root, &path)?;
    Ok(path)
}

fn sync_lease_directory(database_root: &Path, remote: &CanonicalGithubIssue) -> Result<PathBuf> {
    let key = GithubAncestorKey::from_remote(remote);
    let name = key
        .file_name()
        .strip_suffix(".json")
        .expect("ancestor key filenames always end in .json")
        .to_owned();
    let path = database_root.join(SYNC_LEASE_DIRECTORY).join(name);
    ensure_contained(database_root, &path)?;
    Ok(path)
}

fn ensure_same_remote(
    expected: &CanonicalGithubIssue,
    proposed: &CanonicalGithubIssue,
) -> Result<()> {
    anyhow::ensure!(
        expected == proposed,
        "Cannot store GitHub ancestor for {} using snapshot for {}",
        proposed.as_url(),
        expected.as_url()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{mpsc, Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    fn remote(number: u64) -> CanonicalGithubIssue {
        CanonicalGithubIssue::parse(&format!(
            "https://github.com/example/minibeads/issues/{number}"
        ))
        .unwrap()
    }

    fn record(remote: CanonicalGithubIssue, local_id: &str, title: &str) -> GithubAncestorRecord {
        GithubAncestorRecord::new(
            remote,
            LocalIssueId::parse(local_id).unwrap(),
            CommonIssueFields::new(
                CommonIssueTitle::new(title),
                CommonIssueBody::new(format!("Body for {title}")),
                CommonIssueStatus::Open,
            ),
        )
    }

    fn write_record_at(root: &Path, remote: &CanonicalGithubIssue, value: &[u8]) {
        let path = ancestor_path(root, remote).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn sync_lease_serializes_one_issue_without_database_recovery() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(60);
        let other_issue = remote(61);
        let database_journal = root.path().join(crate::transaction::JOURNAL);
        fs::write(
            &database_journal,
            b"database recovery must remain untouched",
        )
        .unwrap();

        let first = acquire_sync_lease(root.path(), &issue).unwrap();
        let independent = acquire_sync_lease(root.path(), &other_issue).unwrap();
        drop(independent);
        assert_eq!(
            fs::read(&database_journal).unwrap(),
            b"database recovery must remain untouched"
        );

        let (started_sender, started_receiver) = mpsc::channel();
        let (acquired_sender, acquired_receiver) = mpsc::channel();
        let second_root = root.path().to_path_buf();
        let second_issue = issue.clone();
        let contender = thread::spawn(move || {
            started_sender.send(()).unwrap();
            let second = acquire_sync_lease(&second_root, &second_issue).unwrap();
            acquired_sender.send(()).unwrap();
            drop(second);
        });

        started_receiver.recv().unwrap();
        assert!(acquired_receiver
            .recv_timeout(Duration::from_millis(100))
            .is_err());
        drop(first);
        acquired_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        contender.join().unwrap();
        assert_eq!(
            fs::read(&database_journal).unwrap(),
            b"database recovery must remain untouched"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_sync_lease_directory_cannot_escape_database_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("sync_ancestors")).unwrap();
        symlink(
            outside.path(),
            root.path().join("sync_ancestors/github-leases"),
        )
        .unwrap();

        let error = acquire_sync_lease(root.path(), &remote(62))
            .err()
            .expect("escaping lease path must fail");
        assert!(error.to_string().contains("Path escapes the database"));
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn staged_cas_defers_publication_and_rolls_back_with_its_batch() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(70);
        let original = record(issue.clone(), "minibeads-70", "Original");
        compare_and_store(root.path(), &issue, None, &original).unwrap();
        let expected = load(root.path(), &issue).unwrap();
        let next = record(issue.clone(), "minibeads-70", "Next");
        let state_path = root.path().join("github-sync-state.json");

        {
            let _lock = Lock::acquire(root.path()).unwrap();
            let mut transaction = FileTransaction::new(root.path());
            assert_eq!(
                compare_and_stage_under_lock(
                    root.path(),
                    &issue,
                    expected.as_ref(),
                    &next,
                    &mut transaction,
                )
                .unwrap(),
                GithubAncestorStageOutcome::Staged
            );
            assert_eq!(load_without_lock(root.path(), &issue).unwrap(), expected);
            transaction.write(state_path.clone(), b"new legacy state".to_vec());
            transaction.commit().unwrap();
        }
        assert_eq!(load(root.path(), &issue).unwrap(), Some(next.clone()));
        assert_eq!(fs::read(&state_path).unwrap(), b"new legacy state");

        let rollback = record(issue.clone(), "minibeads-70", "Rollback");
        let blocking_parent = root.path().join("zz-blocked-parent");
        fs::write(&blocking_parent, b"not a directory").unwrap();
        {
            let _lock = Lock::acquire(root.path()).unwrap();
            let mut transaction = FileTransaction::new(root.path());
            assert_eq!(
                compare_and_stage_under_lock(
                    root.path(),
                    &issue,
                    Some(&next),
                    &rollback,
                    &mut transaction,
                )
                .unwrap(),
                GithubAncestorStageOutcome::Staged
            );
            transaction.write(
                blocking_parent.join("state.json"),
                b"cannot publish".to_vec(),
            );
            assert!(transaction.commit().is_err());
            assert_eq!(
                load_without_lock(root.path(), &issue).unwrap(),
                Some(next.clone())
            );
            assert!(!root.path().join(crate::transaction::JOURNAL).exists());
        }

        let stale = record(issue.clone(), "minibeads-70", "Stale");
        let _lock = Lock::acquire(root.path()).unwrap();
        let mut transaction = FileTransaction::new(root.path());
        assert_eq!(
            compare_and_stage_under_lock(
                root.path(),
                &issue,
                Some(&original),
                &stale,
                &mut transaction,
            )
            .unwrap(),
            GithubAncestorStageOutcome::ConcurrentChange(Some(next))
        );
        transaction.commit().unwrap();
    }

    #[test]
    fn missing_legacy_state_is_distinct_and_issue_records_are_isolated() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("github-sync-state.json"),
            b"{\"issues\":{}}\n",
        )
        .unwrap();
        let first_remote = remote(1);
        let second_remote = remote(2);
        let first_missing = load(root.path(), &first_remote).unwrap();
        assert!(first_missing.is_none());

        let first = record(first_remote.clone(), "minibeads-1", "First");
        assert_ne!(first_missing.as_ref(), Some(&first));
        assert_eq!(
            compare_and_store(root.path(), &first_remote, first_missing.as_ref(), &first).unwrap(),
            GithubAncestorStoreOutcome::Stored
        );
        let first_path = ancestor_path(root.path(), &first_remote).unwrap();
        let first_bytes = fs::read(&first_path).unwrap();
        assert!(load(root.path(), &second_remote).unwrap().is_none());

        let second_missing = load(root.path(), &second_remote).unwrap();
        let second = record(second_remote.clone(), "minibeads-2", "Second");
        assert_eq!(
            compare_and_store(
                root.path(),
                &second_remote,
                second_missing.as_ref(),
                &second
            )
            .unwrap(),
            GithubAncestorStoreOutcome::Stored
        );
        assert_eq!(fs::read(first_path).unwrap(), first_bytes);
        let loaded = load(root.path(), &first_remote).unwrap();
        let loaded_first = loaded.as_ref().unwrap();
        assert_eq!(loaded_first.remote().as_url(), first_remote.as_url());
        assert_eq!(loaded_first.local_id().as_str(), "minibeads-1");
        assert_eq!(loaded_first.common().title().as_str(), "First");
        assert_eq!(loaded_first.common().body().as_str(), "Body for First");
        assert_eq!(loaded_first.common().status(), CommonIssueStatus::Open);
        assert_eq!(
            compare_and_store(root.path(), &first_remote, loaded.as_ref(), &first).unwrap(),
            GithubAncestorStoreOutcome::Unchanged
        );
        let migrated = loaded_first.with_local_id(LocalIssueId::parse("minibeads-101").unwrap());
        assert_eq!(migrated.local_id().as_str(), "minibeads-101");
        assert_eq!(loaded_first.local_id().as_str(), "minibeads-1");
        assert_eq!(
            compare_and_store(root.path(), &first_remote, loaded.as_ref(), &migrated).unwrap(),
            GithubAncestorStoreOutcome::Stored
        );
    }

    #[test]
    fn concurrent_compare_and_store_allows_only_one_replacement() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(10);
        let missing = load(root.path(), &issue).unwrap();
        let initial = record(issue.clone(), "minibeads-10", "Initial");
        compare_and_store(root.path(), &issue, missing.as_ref(), &initial).unwrap();
        let expected = load(root.path(), &issue).unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let left_root = root.path().to_path_buf();
        let left_expected = expected.clone();
        let left_barrier = Arc::clone(&barrier);
        let left_issue = issue.clone();
        let left = thread::spawn(move || {
            let proposed = record(left_issue, "minibeads-10", "Left");
            left_barrier.wait();
            compare_and_store(
                &left_root,
                proposed.remote(),
                left_expected.as_ref(),
                &proposed,
            )
            .unwrap()
        });
        let right_root = root.path().to_path_buf();
        let right_expected = expected;
        let right_barrier = Arc::clone(&barrier);
        let right = thread::spawn(move || {
            let proposed = record(issue, "minibeads-10", "Right");
            right_barrier.wait();
            compare_and_store(
                &right_root,
                proposed.remote(),
                right_expected.as_ref(),
                &proposed,
            )
            .unwrap()
        });
        barrier.wait();
        let left_outcome = left.join().unwrap();
        let right_outcome = right.join().unwrap();

        assert!(matches!(
            (&left_outcome, &right_outcome),
            (
                GithubAncestorStoreOutcome::Stored,
                GithubAncestorStoreOutcome::ConcurrentChange(_)
            ) | (
                GithubAncestorStoreOutcome::ConcurrentChange(_),
                GithubAncestorStoreOutcome::Stored
            )
        ));
        let concurrent = match (&left_outcome, &right_outcome) {
            (GithubAncestorStoreOutcome::ConcurrentChange(current), _) => current,
            (_, GithubAncestorStoreOutcome::ConcurrentChange(current)) => current,
            _ => unreachable!(),
        };
        assert!(concurrent.is_some());
    }

    #[test]
    fn corrupt_record_is_never_treated_as_missing() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(20);
        write_record_at(root.path(), &issue, b"{not-json");
        let error = load(root.path(), &issue).unwrap_err();
        assert!(error.to_string().contains("Corrupt GitHub ancestor"));
    }

    #[test]
    fn future_schema_is_rejected_without_replacement() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(21);
        let future = json!({
            "schema_version": AncestorSchemaVersion::CURRENT.0 + 1,
            "remote": issue.as_url(),
            "local_id": "minibeads-21",
            "common": {
                "title": "Future",
                "body": "Future body",
                "status": "open"
            }
        });
        write_record_at(
            root.path(),
            &issue,
            &serde_json::to_vec_pretty(&future).unwrap(),
        );
        let before = fs::read(ancestor_path(root.path(), &issue).unwrap()).unwrap();
        let error = load(root.path(), &issue).unwrap_err();
        assert!(error.to_string().contains("future schema version"));
        assert_eq!(
            fs::read(ancestor_path(root.path(), &issue).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn record_url_must_match_its_content_derived_key() {
        let root = tempfile::tempdir().unwrap();
        let requested = remote(30);
        let different = record(remote(31), "minibeads-31", "Different");
        write_record_at(
            root.path(),
            &requested,
            &serialize_record(&different).unwrap(),
        );
        let error = load(root.path(), &requested).unwrap_err();
        assert!(error.to_string().contains("URL/key mismatch"));
    }

    #[test]
    fn identity_and_local_id_validation_reject_path_material() {
        assert!(CanonicalGithubIssue::parse("https://github.com/a/b/issues/../1").is_err());
        assert!(CanonicalGithubIssue::parse("https://github.com/a/b/issues/1/extra").is_err());
        assert!(LocalIssueId::parse("../minibeads-1").is_err());
        let canonical = CanonicalGithubIssue::parse(
            "https://github.com/Example/MiniBeads/issues/00000000000000000001",
        )
        .unwrap();
        assert_eq!(
            canonical.as_url(),
            "https://github.com/example/minibeads/issues/1"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_ancestor_directory_cannot_escape_database_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("sync_ancestors")).unwrap();
        symlink(outside.path(), root.path().join("sync_ancestors/github")).unwrap();
        let issue = remote(40);
        let error = load(root.path(), &issue).unwrap_err();
        assert!(error.to_string().contains("Path escapes the database"));
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn compare_and_store_recovers_an_interrupted_ancestor_publication() {
        let root = tempfile::tempdir().unwrap();
        let issue = remote(50);
        let missing = load(root.path(), &issue).unwrap();
        let original = record(issue.clone(), "minibeads-50", "Original");
        compare_and_store(root.path(), &issue, missing.as_ref(), &original).unwrap();
        let expected = load(root.path(), &issue).unwrap();
        let path = ancestor_path(root.path(), &issue).unwrap();
        let original_content = fs::read(&path).unwrap();

        let interrupted = record(issue.clone(), "minibeads-50", "Interrupted");
        fs::write(&path, serialize_record(&interrupted).unwrap()).unwrap();
        let relative_path = path.strip_prefix(root.path()).unwrap();
        let journal = json!([{
            "relative_path": relative_path,
            "content": original_content,
            "modified": null
        }]);
        fs::write(
            root.path().join(crate::transaction::JOURNAL),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();

        let recovered = record(issue.clone(), "minibeads-50", "Recovered");
        assert_eq!(
            compare_and_store(root.path(), &issue, expected.as_ref(), &recovered).unwrap(),
            GithubAncestorStoreOutcome::Stored
        );
        assert!(!root.path().join(crate::transaction::JOURNAL).exists());
        let loaded = load(root.path(), &issue).unwrap();
        assert_eq!(loaded, Some(recovered));
    }
}
