//! GitHub Issues sync using the authenticated `gh` CLI.

use crate::storage::Storage;
use crate::types::{Comment, Issue, IssueType, Status};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};

const MARKER: &str = "MB_DO_NOT_SYNC";
static TRACE_GH_CALLS: AtomicBool = AtomicBool::new(false);

pub struct GhTraceGuard {
    previous: bool,
}

impl Drop for GhTraceGuard {
    fn drop(&mut self) {
        TRACE_GH_CALLS.store(self.previous, Ordering::Relaxed);
    }
}

pub fn trace_gh_calls(enabled: bool) -> GhTraceGuard {
    let previous = TRACE_GH_CALLS.swap(enabled, Ordering::Relaxed);
    GhTraceGuard { previous }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubSyncReport {
    pub linked: usize,
    pub created_remote: usize,
    pub pushed_issues: usize,
    pub pulled_issues: usize,
    pub imported_comments: usize,
    pub exported_comments: usize,
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub issues: Vec<GithubIssueSyncReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubIssueSyncReport {
    pub issue_id: String,
    pub github_url: String,
    pub title: String,
    pub action: String,
    pub comments_imported: usize,
    pub comments_exported: usize,
    #[serde(default)]
    pub details: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubStressReport {
    pub repo: String,
    pub iterations: usize,
    pub issues_created: usize,
    pub github_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GithubSyncState {
    #[serde(default)]
    issues: HashMap<String, GithubIssueState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubIssueState {
    local_id: String,
    local_hash: String,
    remote_hash: String,
    synced_at: DateTime<Utc>,
    #[serde(default)]
    synced_local_comment_ids: Vec<String>,
    #[serde(default)]
    synced_remote_comment_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct RemoteIssue {
    url: String,
    title: String,
    body: String,
    state: String,
    comments: Vec<RemoteComment>,
}

#[derive(Debug, Clone)]
struct RemoteComment {
    id: String,
    url: String,
    author: String,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct GithubStore {
    inner: Arc<GithubStoreInner>,
}

struct GithubStoreInner {
    program: String,
    repo: Option<String>,
    marker_repo: String,
    cache: Mutex<HashMap<String, Arc<Mutex<GithubIssueCache>>>>,
    gh_limit: Semaphore,
}

#[derive(Debug, Default)]
struct GithubIssueCache {
    remote: Option<RemoteIssue>,
    comments_dirty: bool,
}

#[derive(Clone)]
struct GithubIssueHandle {
    store: GithubStore,
    reference: String,
}

impl GithubStore {
    fn new(repo: Option<&str>) -> Self {
        Self {
            inner: Arc::new(GithubStoreInner {
                program: "gh".to_string(),
                repo: repo.map(ToString::to_string),
                marker_repo: local_repo_name().unwrap_or_else(|| "unknown repository".to_string()),
                cache: Mutex::new(HashMap::new()),
                gh_limit: Semaphore::new(1),
            }),
        }
    }

    #[cfg(test)]
    fn new_with_program(repo: Option<&str>, program: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(GithubStoreInner {
                program: program.into(),
                repo: repo.map(ToString::to_string),
                marker_repo: "test/repo".to_string(),
                cache: Mutex::new(HashMap::new()),
                gh_limit: Semaphore::new(1),
            }),
        }
    }

    fn issue(&self, reference: impl Into<String>) -> GithubIssueHandle {
        GithubIssueHandle {
            store: self.clone(),
            reference: reference.into(),
        }
    }

    async fn create_issue(&self, issue: &Issue) -> Result<RemoteIssue> {
        let mut args = vec![
            "issue".to_string(),
            "create".to_string(),
            "--title".to_string(),
            issue.title.clone(),
            "--body".to_string(),
            issue.description.clone(),
        ];
        self.push_repo_args(&mut args);
        let output = self.gh_output(args).await?;
        let url = output
            .lines()
            .find(|line| is_github_issue_url(line.trim()))
            .map(str::trim)
            .ok_or_else(|| anyhow!("gh issue create did not return a GitHub issue URL"))?;
        let remote = self.issue(url).refresh().await?;
        Ok(remote)
    }

    fn push_repo_args(&self, args: &mut Vec<String>) {
        if let Some(repo) = &self.inner.repo {
            args.push("--repo".to_string());
            args.push(repo.clone());
        }
    }

    async fn gh_json(&self, args: Vec<String>) -> Result<Value> {
        let printable = args.join(" ");
        let output = self.gh_output(args).await?;
        serde_json::from_str(&output)
            .with_context(|| format!("Failed to parse gh JSON from gh {printable}"))
    }

    async fn gh_status(&self, args: Vec<String>) -> Result<()> {
        self.gh_output(args).await.map(|_| ())
    }

    async fn gh_output(&self, args: Vec<String>) -> Result<String> {
        let _permit = self
            .inner
            .gh_limit
            .acquire()
            .await
            .context("GitHub command limiter closed")?;
        gh_output_async(&self.inner.program, args).await
    }

    fn marker_body(&self, issue: &Issue) -> String {
        marker_body_for_repo(issue, &self.inner.marker_repo)
    }
}

impl GithubIssueHandle {
    async fn get(&self) -> Result<RemoteIssue> {
        let entry = self.entry().await;
        let mut cache = entry.lock().await;
        if let Some(remote) = &cache.remote {
            return Ok(remote.clone());
        }

        let remote = self.fetch().await?;
        cache.remote = Some(remote.clone());
        drop(cache);
        self.alias_url(&remote.url, entry).await;
        Ok(remote)
    }

    async fn refresh(&self) -> Result<RemoteIssue> {
        let entry = self.entry().await;
        let mut cache = entry.lock().await;
        let remote = self.fetch().await?;
        cache.remote = Some(remote.clone());
        cache.comments_dirty = false;
        drop(cache);
        self.alias_url(&remote.url, entry).await;
        Ok(remote)
    }

    async fn ensure_marker(&self, issue: &Issue) -> Result<()> {
        let remote = self.get().await?;
        if remote
            .comments
            .iter()
            .any(|comment| is_marker_comment(comment) && comment.body.contains(&issue.id))
        {
            return Ok(());
        }

        let body = self.store.marker_body(issue);
        self.add_comment_raw(&body).await?;
        self.mutate_cached_remote(|remote| {
            remote.comments.push(RemoteComment {
                id: format!("synthetic-marker-{}", issue.id),
                url: remote.url.clone(),
                author: "minibeads".to_string(),
                body,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
            Ok(())
        })
        .await
    }

    async fn edit_fields(&self, issue: &Issue) -> Result<()> {
        let remote = self.get().await?;
        let mut args = vec![
            "issue".to_string(),
            "edit".to_string(),
            remote.url.clone(),
            "--title".to_string(),
            issue.title.clone(),
            "--body".to_string(),
            issue.description.clone(),
        ];
        self.store.push_repo_args(&mut args);
        self.store.gh_status(args).await?;
        self.mutate_cached_remote(|remote| {
            remote.title = issue.title.clone();
            remote.body = issue.description.clone();
            Ok(())
        })
        .await
    }

    async fn set_state(&self, status: Status) -> Result<()> {
        let remote = self.get().await?;
        let is_closed = remote.state.eq_ignore_ascii_case("closed");
        match status {
            Status::Closed if !is_closed => {
                let mut args = vec!["issue".to_string(), "close".to_string(), remote.url.clone()];
                self.store.push_repo_args(&mut args);
                self.store.gh_status(args).await?;
                self.mutate_cached_remote(|remote| {
                    remote.state = "CLOSED".to_string();
                    Ok(())
                })
                .await?;
            }
            status if status != Status::Closed && is_closed => {
                let mut args = vec![
                    "issue".to_string(),
                    "reopen".to_string(),
                    remote.url.clone(),
                ];
                self.store.push_repo_args(&mut args);
                self.store.gh_status(args).await?;
                self.mutate_cached_remote(|remote| {
                    remote.state = "OPEN".to_string();
                    Ok(())
                })
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn add_comment(&self, body: &str) -> Result<bool> {
        let remote = self.get().await?;
        if remote.comments.iter().any(|comment| comment.body == body) {
            return Ok(false);
        }
        self.add_comment_raw(body).await?;
        let entry = self.entry().await;
        let mut cache = entry.lock().await;
        cache.comments_dirty = true;
        Ok(true)
    }

    async fn snapshot_for_state(&self) -> Result<RemoteIssue> {
        let entry = self.entry().await;
        let comments_dirty = entry.lock().await.comments_dirty;
        if comments_dirty {
            self.refresh().await
        } else {
            self.get().await
        }
    }

    async fn entry(&self) -> Arc<Mutex<GithubIssueCache>> {
        let mut cache = self.store.inner.cache.lock().await;
        cache
            .entry(self.reference.clone())
            .or_insert_with(|| Arc::new(Mutex::new(GithubIssueCache::default())))
            .clone()
    }

    async fn alias_url(&self, url: &str, entry: Arc<Mutex<GithubIssueCache>>) {
        if url == self.reference {
            return;
        }
        let mut cache = self.store.inner.cache.lock().await;
        cache.entry(url.to_string()).or_insert(entry);
    }

    async fn fetch(&self) -> Result<RemoteIssue> {
        let mut args = vec![
            "issue".to_string(),
            "view".to_string(),
            self.reference.clone(),
            "--json".to_string(),
            "number,url,title,body,state,comments".to_string(),
        ];
        self.store.push_repo_args(&mut args);
        let value = self.store.gh_json(args).await?;
        parse_remote_issue(value)
    }

    async fn add_comment_raw(&self, body: &str) -> Result<()> {
        let remote = self.get().await?;
        let mut args = vec![
            "issue".to_string(),
            "comment".to_string(),
            remote.url,
            "--body".to_string(),
            body.to_string(),
        ];
        self.store.push_repo_args(&mut args);
        self.store.gh_status(args).await
    }

    async fn mutate_cached_remote(
        &self,
        f: impl FnOnce(&mut RemoteIssue) -> Result<()>,
    ) -> Result<()> {
        let entry = self.entry().await;
        let mut cache = entry.lock().await;
        let Some(remote) = cache.remote.as_mut() else {
            return Err(anyhow!("GitHub issue cache missing {}", self.reference));
        };
        f(remote)
    }
}

fn block_on_github<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to initialize GitHub async runtime")?
        .block_on(future)
}

pub fn link_issue(
    storage: &Storage,
    issue_id: &str,
    reference: &str,
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    block_on_github(link_issue_async(
        storage, issue_id, reference, repo, dry_run,
    ))
}

async fn link_issue_async(
    storage: &Storage,
    issue_id: &str,
    reference: &str,
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    let store = GithubStore::new(repo);
    let handle = store.issue(reference);
    let remote = handle.get().await?;

    if !dry_run {
        let mut updates = HashMap::new();
        updates.insert("external_ref".to_string(), remote.url.clone());
        storage.update_issue(issue_id, updates)?;

        let issue = storage
            .get_issue(issue_id)?
            .ok_or_else(|| anyhow!("Issue not found after link: {}", issue_id))?;
        handle.ensure_marker(&issue).await?;
        import_remote_comments(storage, &issue, &remote)?;
        let comments = storage.list_comments(issue_id)?;
        let remote = handle.snapshot_for_state().await?;
        remember_state(
            storage.get_beads_dir().as_path(),
            &issue,
            &remote,
            &comments,
        )?;
    }

    Ok(GithubSyncReport {
        linked: 1,
        created_remote: 0,
        pushed_issues: 0,
        pulled_issues: 0,
        imported_comments: if dry_run { 0 } else { remote.comments.len() },
        exported_comments: 0,
        conflicts: Vec::new(),
        issues: vec![GithubIssueSyncReport {
            issue_id: issue_id.to_string(),
            github_url: remote.url,
            title: remote.title,
            action: if dry_run { "would-link" } else { "linked" }.to_string(),
            comments_imported: if dry_run { 0 } else { remote.comments.len() },
            comments_exported: 0,
            details: vec!["stored GitHub URL in external_ref".to_string()],
            conflict: None,
        }],
    })
}

pub fn publish_issue(
    storage: &Storage,
    issue_id: &str,
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    block_on_github(publish_issue_async(storage, issue_id, repo, dry_run))
}

async fn publish_issue_async(
    storage: &Storage,
    issue_id: &str,
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    let issue = storage
        .get_issue(issue_id)?
        .ok_or_else(|| anyhow!("Issue not found: {}", issue_id))?;

    if issue
        .external_ref
        .as_deref()
        .is_some_and(is_github_issue_url)
    {
        return Err(anyhow!("{} is already linked to GitHub", issue_id));
    }

    if dry_run {
        return Ok(GithubSyncReport {
            linked: 0,
            created_remote: 1,
            pushed_issues: 0,
            pulled_issues: 0,
            imported_comments: 0,
            exported_comments: 0,
            conflicts: Vec::new(),
            issues: vec![GithubIssueSyncReport {
                issue_id: issue.id.clone(),
                github_url: String::new(),
                title: issue.title.clone(),
                action: "would-publish".to_string(),
                comments_imported: 0,
                comments_exported: 0,
                details: vec!["would create GitHub issue and store external_ref".to_string()],
                conflict: None,
            }],
        });
    }

    let store = GithubStore::new(repo);
    let handle_remote = store.create_issue(&issue).await?;
    let handle = store.issue(&handle_remote.url);
    let mut updates = HashMap::new();
    updates.insert("external_ref".to_string(), handle_remote.url.clone());
    storage.update_issue(issue_id, updates)?;

    let issue = storage
        .get_issue(issue_id)?
        .ok_or_else(|| anyhow!("Issue not found after publish: {}", issue_id))?;
    handle.ensure_marker(&issue).await?;
    let local_comments = storage.list_comments(issue_id)?;
    let exported =
        export_new_local_comments(&issue, &handle, &local_comments, &HashSet::new()).await?;
    let remote = handle.snapshot_for_state().await?;
    remember_state(
        storage.get_beads_dir().as_path(),
        &issue,
        &remote,
        &local_comments,
    )?;

    Ok(GithubSyncReport {
        linked: 1,
        created_remote: 1,
        pushed_issues: 0,
        pulled_issues: 0,
        imported_comments: 0,
        exported_comments: exported,
        conflicts: Vec::new(),
        issues: vec![GithubIssueSyncReport {
            issue_id: issue.id,
            github_url: remote.url,
            title: remote.title,
            action: "published".to_string(),
            comments_imported: 0,
            comments_exported: exported,
            details: vec!["created GitHub issue and stored external_ref".to_string()],
            conflict: None,
        }],
    })
}

pub fn sync_linked(
    storage: &Storage,
    issue_ids: &[String],
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    block_on_github(sync_linked_async(storage, issue_ids, repo, dry_run))
}

async fn sync_linked_async(
    storage: &Storage,
    issue_ids: &[String],
    repo: Option<&str>,
    dry_run: bool,
) -> Result<GithubSyncReport> {
    let store = GithubStore::new(repo);
    let beads_dir = storage.get_beads_dir();
    let mut state = load_state(&beads_dir)?;
    let mut report = GithubSyncReport {
        linked: 0,
        created_remote: 0,
        pushed_issues: 0,
        pulled_issues: 0,
        imported_comments: 0,
        exported_comments: 0,
        conflicts: Vec::new(),
        issues: Vec::new(),
    };

    let mut issues = storage.list_issues(None, None, None, None, None)?;
    if !issue_ids.is_empty() {
        let wanted: HashSet<&str> = issue_ids.iter().map(String::as_str).collect();
        issues.retain(|issue| wanted.contains(issue.id.as_str()));
    }

    for mut issue in issues {
        let Some(url) = issue.external_ref.clone() else {
            continue;
        };
        if !is_github_issue_url(&url) {
            continue;
        }

        let handle = store.issue(&url);
        let remote = handle.get().await?;
        if !dry_run {
            handle.ensure_marker(&issue).await?;
        }
        let local_comments = storage.list_comments(&issue.id)?;
        let old_state = state.issues.get(&url);
        let local_hash = hash_local_issue(&issue);
        let remote_hash = hash_remote_issue(&remote);
        let state_has_common_base = old_state
            .map(|s| s.local_hash == s.remote_hash)
            .unwrap_or(false);
        let local_changed = old_state.map(|s| s.local_hash.as_str()) != Some(local_hash.as_str());
        let remote_changed =
            old_state.map(|s| s.remote_hash.as_str()) != Some(remote_hash.as_str());
        let inherited_divergence = old_state.is_some() && !state_has_common_base;
        let mut item = GithubIssueSyncReport {
            issue_id: issue.id.clone(),
            github_url: url.clone(),
            title: issue.title.clone(),
            action: "unchanged".to_string(),
            comments_imported: 0,
            comments_exported: 0,
            details: Vec::new(),
            conflict: None,
        };
        let mut remote_written = false;

        match (
            old_state.is_some(),
            inherited_divergence,
            local_changed,
            remote_changed,
        ) {
            (false, _, _, _) => {
                if !dry_run {
                    handle.edit_fields(&issue).await?;
                    handle.set_state(issue.status.clone()).await?;
                    remote_written = true;
                }
                item.action = if dry_run {
                    "would-initialize-push".to_string()
                } else {
                    "initialized-pushed".to_string()
                };
                item.details
                    .push("no previous GitHub sync state; pushed local issue fields to GitHub as initial common base".to_string());
                report.pushed_issues += 1;
            }
            (true, true, false, false) => {
                if local_hash != remote_hash {
                    if !dry_run {
                        handle.edit_fields(&issue).await?;
                        handle.set_state(issue.status.clone()).await?;
                        remote_written = true;
                    }
                    item.action = if dry_run {
                        "would-repair-divergence".to_string()
                    } else {
                        "repaired-divergence".to_string()
                    };
                    item.details.push(
                        "previous sync state recorded divergent local/remote hashes; pushed local issue fields to establish a common base"
                            .to_string(),
                    );
                    if issue.status == Status::Closed
                        && !remote.state.eq_ignore_ascii_case("closed")
                    {
                        item.details.push("closed GitHub issue".to_string());
                    } else if issue.status != Status::Closed
                        && remote.state.eq_ignore_ascii_case("closed")
                    {
                        item.details.push("reopened GitHub issue".to_string());
                    }
                    report.pushed_issues += 1;
                } else {
                    item.details.push(
                        "previous divergent sync state has already converged; recording common base"
                            .to_string(),
                    );
                }
            }
            (true, _, true, true) if local_hash != remote_hash => {
                let conflict = format!(
                    "{} / {} changed on both sides; leaving issue fields unchanged",
                    issue.id, url
                );
                item.action = "conflict".to_string();
                item.conflict = Some(conflict.clone());
                item.details
                    .push("local and GitHub issue fields both changed since last sync".to_string());
                report.conflicts.push(conflict);
            }
            (_, _, true, false) => {
                if !dry_run {
                    handle.edit_fields(&issue).await?;
                    handle.set_state(issue.status.clone()).await?;
                    remote_written = true;
                }
                item.action = if dry_run {
                    "would-push".to_string()
                } else {
                    "pushed".to_string()
                };
                item.details
                    .push("pushed title/body from minibeads to GitHub".to_string());
                if issue.status == Status::Closed && !remote.state.eq_ignore_ascii_case("closed") {
                    item.details.push("closed GitHub issue".to_string());
                } else if issue.status != Status::Closed
                    && remote.state.eq_ignore_ascii_case("closed")
                {
                    item.details.push("reopened GitHub issue".to_string());
                }
                report.pushed_issues += 1;
            }
            (_, _, false, true) => {
                if !dry_run {
                    apply_remote_to_local(storage, &issue, &remote)?;
                    issue = storage
                        .get_issue(&issue.id)?
                        .ok_or_else(|| anyhow!("Issue not found after pull: {}", issue.id))?;
                }
                item.action = if dry_run {
                    "would-pull".to_string()
                } else {
                    "pulled".to_string()
                };
                item.details
                    .push("pulled title/body/state from GitHub into minibeads".to_string());
                report.pulled_issues += 1;
            }
            _ => {
                item.details
                    .push("issue fields already match last synced state".to_string());
            }
        }

        let synced_local_ids: HashSet<String> = old_state
            .map(|s| s.synced_local_comment_ids.iter().cloned().collect())
            .unwrap_or_default();
        let synced_remote_ids: HashSet<String> = old_state
            .map(|s| s.synced_remote_comment_ids.iter().cloned().collect())
            .unwrap_or_default();

        let exported = if dry_run {
            local_comments
                .iter()
                .filter(|c| c.source_id.is_none() && !synced_local_ids.contains(&c.id))
                .count()
        } else {
            let exported =
                export_new_local_comments(&issue, &handle, &local_comments, &synced_local_ids)
                    .await?;
            if exported > 0 {
                remote_written = true;
            }
            exported
        };
        let imported = if dry_run {
            remote
                .comments
                .iter()
                .filter(|c| !is_marker_comment(c))
                .filter(|c| !synced_remote_ids.contains(&c.id))
                .count()
        } else {
            import_remote_comments(storage, &issue, &remote)?
        };

        report.exported_comments += exported;
        report.imported_comments += imported;
        item.comments_exported = exported;
        item.comments_imported = imported;
        if exported > 0 {
            item.details
                .push(format!("exported {} local comment(s) to GitHub", exported));
        }
        if imported > 0 {
            item.details
                .push(format!("imported {} GitHub comment(s)", imported));
        }

        if !dry_run {
            let remote = if remote_written {
                handle.snapshot_for_state().await?
            } else {
                remote
            };
            let comments = storage.list_comments(&issue.id)?;
            update_state_entry(&mut state, &issue, &remote, &comments);
        }

        report.issues.push(item);
    }

    if !dry_run {
        save_state(&beads_dir, &state)?;
    }

    Ok(report)
}

pub fn stress_test(repo: &str, iterations: usize) -> Result<GithubStressReport> {
    if iterations == 0 {
        return Err(anyhow!("--iterations must be greater than zero"));
    }

    let tmp = tempfile::tempdir().context("Failed to create temporary stress workspace")?;
    let storage = Storage::init(
        tmp.path().join(".beads"),
        Some("ghstress".to_string()),
        false,
    )
    .context("Failed to initialize temporary stress minibeads database")?;
    let run_id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    let mut urls = Vec::new();
    for i in 0..iterations {
        let title = format!("mb gh sync stress {run_id} #{i}");
        let body = format!("initial local body {run_id} #{i}");
        let issue = storage.create_issue(
            title.clone(),
            body.clone(),
            None,
            None,
            2,
            IssueType::Task,
            None,
            Vec::new(),
            None,
            None,
            Vec::new(),
        )?;

        let publish = publish_issue(&storage, &issue.id, Some(repo), false)
            .with_context(|| format!("stress publish failed for {}", issue.id))?;
        let url = publish
            .issues
            .first()
            .map(|issue| issue.github_url.clone())
            .ok_or_else(|| anyhow!("publish did not return a GitHub URL"))?;
        urls.push(url.clone());

        let remote = gh_issue_view(&url, Some(repo))?;
        assert_remote_matches(&remote, &title, &body, Status::Open)?;
        assert_marker_present(&remote, &issue.id)?;

        let local_title = format!("local title {run_id} #{i}");
        let local_body = format!("local body {run_id} #{i}");
        storage.update_issue(
            &issue.id,
            HashMap::from([
                ("title".to_string(), local_title.clone()),
                ("description".to_string(), local_body.clone()),
            ]),
        )?;
        sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
            .with_context(|| format!("stress local-to-GitHub sync failed for {}", issue.id))?;
        let remote = gh_issue_view(&url, Some(repo))?;
        assert_remote_matches(&remote, &local_title, &local_body, Status::Open)?;
        assert_marker_present(&remote, &issue.id)?;
        assert_marker_not_imported(&storage, &issue.id)?;

        let mb_comment = format!("local comment {run_id} #{i}");
        storage.add_comment(&issue.id, "stress-local", &mb_comment)?;
        sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
            .with_context(|| format!("stress local comment sync failed for {}", issue.id))?;
        let remote = gh_issue_view(&url, Some(repo))?;
        let rendered = format!(
            "_From minibeads {} by stress-local_\n\n{}",
            issue.id, mb_comment
        );
        if !remote
            .comments
            .iter()
            .any(|comment| comment.body == rendered)
        {
            return Err(anyhow!("local comment was not exported for {}", issue.id));
        }
        let exported_count = remote
            .comments
            .iter()
            .filter(|comment| comment.body == rendered)
            .count();
        if exported_count != 1 {
            return Err(anyhow!(
                "local comment exported {} times for {}",
                exported_count,
                issue.id
            ));
        }

        let remote_title = format!("remote title {run_id} #{i}");
        let remote_body = format!("remote body {run_id} #{i}");
        gh_status(&[
            "issue",
            "edit",
            &url,
            "--repo",
            repo,
            "--title",
            &remote_title,
            "--body",
            &remote_body,
        ])?;
        let gh_comment = format!("remote comment {run_id} #{i}");
        let remote_for_comment = gh_issue_view(&url, Some(repo))?;
        gh_issue_comment(&remote_for_comment, &gh_comment, Some(repo))?;
        sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
            .with_context(|| format!("stress GitHub-to-local sync failed for {}", issue.id))?;
        let local = storage
            .get_issue(&issue.id)?
            .ok_or_else(|| anyhow!("local stress issue disappeared: {}", issue.id))?;
        if local.title != remote_title || local.description != remote_body {
            return Err(anyhow!("remote title/body was not pulled for {}", issue.id));
        }
        let local_comments = storage.list_comments(&issue.id)?;
        if !local_comments
            .iter()
            .any(|comment| comment.body == gh_comment)
        {
            return Err(anyhow!("remote comment was not imported for {}", issue.id));
        }
        assert_marker_not_imported(&storage, &issue.id)?;

        storage.close_issue(&issue.id, "stress complete")?;
        sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
            .with_context(|| format!("stress close sync failed for {}", issue.id))?;
        let remote = gh_issue_view(&url, Some(repo))?;
        assert_remote_matches(&remote, &remote_title, &remote_body, Status::Closed)?;
        assert_marker_present(&remote, &issue.id)?;
    }

    Ok(GithubStressReport {
        repo: repo.to_string(),
        iterations,
        issues_created: urls.len(),
        github_urls: urls,
    })
}

fn assert_remote_matches(
    remote: &RemoteIssue,
    title: &str,
    body: &str,
    status: Status,
) -> Result<()> {
    if remote.title != title {
        return Err(anyhow!(
            "remote title mismatch for {}: expected {:?}, got {:?}",
            remote.url,
            title,
            remote.title
        ));
    }
    if remote.body != body {
        return Err(anyhow!("remote body mismatch for {}", remote.url));
    }
    let expected_state = if status == Status::Closed {
        "closed"
    } else {
        "open"
    };
    if !remote.state.eq_ignore_ascii_case(expected_state) {
        return Err(anyhow!(
            "remote state mismatch for {}: expected {}, got {}",
            remote.url,
            expected_state,
            remote.state
        ));
    }
    Ok(())
}

fn assert_marker_present(remote: &RemoteIssue, issue_id: &str) -> Result<()> {
    if remote
        .comments
        .iter()
        .any(|comment| is_marker_comment(comment) && comment.body.contains(issue_id))
    {
        Ok(())
    } else {
        Err(anyhow!("marker comment missing for {}", remote.url))
    }
}

fn assert_marker_not_imported(storage: &Storage, issue_id: &str) -> Result<()> {
    let comments = storage.list_comments(issue_id)?;
    if comments.iter().any(|comment| comment.body.contains(MARKER)) {
        Err(anyhow!("marker comment was imported for {}", issue_id))
    } else {
        Ok(())
    }
}

fn apply_remote_to_local(storage: &Storage, issue: &Issue, remote: &RemoteIssue) -> Result<()> {
    let mut updates = HashMap::new();
    updates.insert("title".to_string(), remote.title.clone());
    updates.insert("description".to_string(), remote.body.clone());
    updates.insert(
        "status".to_string(),
        if remote.state.eq_ignore_ascii_case("closed") {
            Status::Closed
        } else {
            Status::Open
        }
        .to_string(),
    );
    storage.update_issue(&issue.id, updates)?;
    Ok(())
}

fn import_remote_comments(storage: &Storage, issue: &Issue, remote: &RemoteIssue) -> Result<usize> {
    let local_comments = storage.list_comments(&issue.id)?;
    let comments = remote
        .comments
        .iter()
        .filter(|remote_comment| !is_marker_comment(remote_comment))
        .filter(|remote_comment| {
            !local_comments.iter().any(|local| {
                local.source_id.is_none()
                    && remote_comment.body
                        == format!(
                            "_From minibeads {} by {}_\n\n{}",
                            issue.id, local.author, local.body
                        )
            })
        })
        .map(|c| Comment {
            id: format!("gh-{}", c.id),
            issue_id: issue.id.clone(),
            author: c.author.clone(),
            body: c.body.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
            source_url: Some(c.url.clone()),
            source_id: Some(c.id.clone()),
        })
        .collect();
    storage.upsert_comments(&issue.id, comments)
}

async fn export_new_local_comments(
    issue: &Issue,
    remote: &GithubIssueHandle,
    comments: &[Comment],
    synced_local_ids: &HashSet<String>,
) -> Result<usize> {
    let mut exported = 0;
    for comment in comments {
        if comment.source_id.is_some() || synced_local_ids.contains(&comment.id) {
            continue;
        }

        let body = format!(
            "_From minibeads {} by {}_\n\n{}",
            issue.id, comment.author, comment.body
        );
        if remote.add_comment(&body).await? {
            exported += 1;
        }
    }
    Ok(exported)
}

fn remember_state(
    beads_dir: &Path,
    issue: &Issue,
    remote: &RemoteIssue,
    comments: &[Comment],
) -> Result<()> {
    let mut state = load_state(beads_dir)?;
    update_state_entry(&mut state, issue, remote, comments);
    save_state(beads_dir, &state)
}

fn update_state_entry(
    state: &mut GithubSyncState,
    issue: &Issue,
    remote: &RemoteIssue,
    comments: &[Comment],
) {
    state.issues.insert(
        remote.url.clone(),
        GithubIssueState {
            local_id: issue.id.clone(),
            local_hash: hash_local_issue(issue),
            remote_hash: hash_remote_issue(remote),
            synced_at: Utc::now(),
            synced_local_comment_ids: comments.iter().map(|c| c.id.clone()).collect(),
            synced_remote_comment_ids: remote
                .comments
                .iter()
                .filter(|c| !is_marker_comment(c))
                .map(|c| c.id.clone())
                .collect(),
        },
    );
}

fn load_state(beads_dir: &Path) -> Result<GithubSyncState> {
    let path = beads_dir.join("github-sync-state.json");
    if !path.exists() {
        return Ok(GithubSyncState::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

fn save_state(beads_dir: &Path, state: &GithubSyncState) -> Result<()> {
    let path = beads_dir.join("github-sync-state.json");
    let content =
        serde_json::to_string_pretty(state).context("Failed to serialize GitHub sync state")?;
    std::fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))
}

fn hash_local_issue(issue: &Issue) -> String {
    hash_fields(&[&issue.title, &issue.description, issue.status.as_str()])
}

fn hash_remote_issue(issue: &RemoteIssue) -> String {
    hash_fields(&[
        &issue.title,
        &issue.body,
        if issue.state.eq_ignore_ascii_case("closed") {
            "closed"
        } else {
            "open"
        },
    ])
}

fn hash_fields(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field.as_bytes());
        hasher.update(b"\0");
    }
    format!("{:x}", hasher.finalize())
}

fn is_github_issue_url(url: &str) -> bool {
    url.starts_with("https://github.com/") && url.contains("/issues/")
}

fn gh_issue_view(reference: &str, repo: Option<&str>) -> Result<RemoteIssue> {
    let mut args = vec![
        "issue",
        "view",
        reference,
        "--json",
        "number,url,title,body,state,comments",
    ];
    if let Some(repo) = repo {
        args.extend(["--repo", repo]);
    }
    let value = gh_json(&args)?;
    parse_remote_issue(value)
}

fn gh_issue_comment(remote: &RemoteIssue, body: &str, repo: Option<&str>) -> Result<()> {
    let mut args = vec!["issue", "comment", &remote.url, "--body", body];
    if let Some(repo) = repo {
        args.extend(["--repo", repo]);
    }
    gh_status(&args)
}

#[cfg(test)]
fn marker_body(issue: &Issue) -> String {
    let repo = local_repo_name().unwrap_or_else(|| "unknown repository".to_string());
    marker_body_for_repo(issue, &repo)
}

fn marker_body_for_repo(issue: &Issue, repo: &str) -> String {
    format!(
        "{MARKER}\n\nThis GitHub issue is synced to local minibeads issue `{}` in repository `{}` at `.beads/issues/{}.md`.",
        issue.id, repo, issue.id
    )
}

fn is_marker_comment(comment: &RemoteComment) -> bool {
    comment.body.contains(MARKER)
}

fn local_repo_name() -> Option<String> {
    let output = Command::new("gh")
        .args(["repo", "view", "--json", "nameWithOwner"])
        .output()
        .ok()?;
    if output.status.success() {
        let value: Value = serde_json::from_slice(&output.stdout).ok()?;
        if let Some(name) = value.get("nameWithOwner").and_then(Value::as_str) {
            return Some(name.to_string());
        }
    }

    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if output.status.success() {
        let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !remote.is_empty() {
            return Some(remote);
        }
    }
    None
}

fn gh_json(args: &[&str]) -> Result<Value> {
    let output = gh_output(args)?;
    serde_json::from_str(&output)
        .with_context(|| format!("Failed to parse gh JSON from gh {}", args.join(" ")))
}

fn gh_status(args: &[&str]) -> Result<()> {
    gh_output(args).map(|_| ())
}

fn gh_output(args: &[&str]) -> Result<String> {
    let started = Instant::now();
    let output = Command::new("gh")
        .args(args)
        .output()
        .with_context(|| format!("Failed to run gh {}", args.join(" ")))?;
    let elapsed = started.elapsed();

    if TRACE_GH_CALLS.load(Ordering::Relaxed) {
        let status = if output.status.success() {
            "ok".to_string()
        } else {
            output
                .status
                .code()
                .map(|code| format!("exit {code}"))
                .unwrap_or_else(|| "terminated".to_string())
        };
        eprintln!(
            "gh {} -> {} in {:.3}s",
            shell_quote_args(args),
            status,
            elapsed.as_secs_f64()
        );
    }

    if !output.status.success() {
        return Err(anyhow!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn gh_output_async(program: &str, args: Vec<String>) -> Result<String> {
    let started = Instant::now();
    let output = tokio::process::Command::new(program)
        .args(&args)
        .output()
        .await
        .with_context(|| format!("Failed to run {} {}", program, args.join(" ")))?;
    let elapsed = started.elapsed();

    if TRACE_GH_CALLS.load(Ordering::Relaxed) {
        let status = if output.status.success() {
            "ok".to_string()
        } else {
            output
                .status
                .code()
                .map(|code| format!("exit {code}"))
                .unwrap_or_else(|| "terminated".to_string())
        };
        eprintln!(
            "gh {} -> {} in {:.3}s",
            shell_quote_string_args(&args),
            status,
            elapsed.as_secs_f64()
        );
    }

    if !output.status.success() {
        return Err(anyhow!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn shell_quote_args(args: &[&str]) -> String {
    args.iter()
        .map(|arg| shell_quote_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_string_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@'))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn parse_remote_issue(value: Value) -> Result<RemoteIssue> {
    let url = string_field(&value, "url")?;
    let title = string_field(&value, "title")?;
    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let state = string_field(&value, "state")?;
    let comments = value
        .get("comments")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(parse_remote_comment)
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(RemoteIssue {
        url,
        title,
        body,
        state,
        comments,
    })
}

fn parse_remote_comment(value: &Value) -> Result<RemoteComment> {
    let id = string_field(value, "id")?;
    let url = string_field(value, "url")?;
    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let author = value
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("github")
        .to_string();
    let created_at = parse_time(value, "createdAt")?;
    let updated_at = match value.get("updatedAt").and_then(Value::as_str) {
        Some(_) => parse_time(value, "updatedAt")?,
        None => created_at,
    };

    Ok(RemoteComment {
        id,
        url,
        author,
        body,
        created_at,
        updated_at,
    })
}

fn string_field(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("GitHub issue JSON missing {}", field))
}

fn parse_time(value: &Value, field: &str) -> Result<DateTime<Utc>> {
    let s = string_field(value, field)?;
    DateTime::parse_from_rfc3339(&s)
        .map(|t| t.with_timezone(&Utc))
        .with_context(|| format!("Invalid GitHub timestamp in {}", field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_gh(tmp: &tempfile::TempDir) -> (String, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let log = tmp.path().join("gh.log");
        let json = tmp.path().join("issue.json");
        let script = tmp.path().join("gh-fake");
        std::fs::write(
            &json,
            r#"{"url":"https://github.com/example/repo/issues/1","title":"Remote title","body":"Remote body","state":"OPEN","comments":[]}"#,
        )
        .unwrap();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\ncase \"$1 $2\" in\n  'issue view') cat {} ;;\n  'issue comment') exit 0 ;;\n  *) echo \"unexpected gh args: $*\" >&2; exit 99 ;;\nesac\n",
                shell_quote_arg(&log.to_string_lossy()),
                shell_quote_arg(&json.to_string_lossy()),
            ),
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        (script.to_string_lossy().to_string(), log)
    }

    fn remote_comment(id: &str, body: &str) -> RemoteComment {
        RemoteComment {
            id: id.to_string(),
            url: format!("https://github.com/example/repo/issues/1#issuecomment-{id}"),
            author: "github-user".to_string(),
            body: body.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn remote_issue(comments: Vec<RemoteComment>) -> RemoteIssue {
        RemoteIssue {
            url: "https://github.com/example/repo/issues/1".to_string(),
            title: "Remote title".to_string(),
            body: "Remote body".to_string(),
            state: "OPEN".to_string(),
            comments,
        }
    }

    fn storage_with_issue() -> (tempfile::TempDir, Storage, Issue) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = Storage::init(tmp.path().join(".beads"), None, false).unwrap();
        let issue = storage
            .create_issue(
                "Local title".to_string(),
                "Local body".to_string(),
                None,
                None,
                2,
                IssueType::Task,
                None,
                Vec::new(),
                None,
                None,
                Vec::new(),
            )
            .unwrap();
        (tmp, storage, issue)
    }

    #[test]
    fn marker_comments_are_not_imported() {
        let (_tmp, storage, issue) = storage_with_issue();
        let remote = remote_issue(vec![
            remote_comment("marker", &marker_body(&issue)),
            remote_comment("regular", "real GitHub comment"),
        ]);

        let imported = import_remote_comments(&storage, &issue, &remote).unwrap();
        let comments = storage.list_comments(&issue.id).unwrap();

        assert_eq!(imported, 1);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "real GitHub comment");
        assert!(!comments[0].body.contains(MARKER));
    }

    #[test]
    fn marker_comments_are_not_recorded_as_synced_remote_comments() {
        let (_tmp, _storage, issue) = storage_with_issue();
        let remote = remote_issue(vec![
            remote_comment("marker", &marker_body(&issue)),
            remote_comment("regular", "real GitHub comment"),
        ]);
        let mut state = GithubSyncState::default();

        update_state_entry(&mut state, &issue, &remote, &[]);

        let entry = state.issues.get(&remote.url).unwrap();
        assert_eq!(entry.synced_remote_comment_ids, vec!["regular"]);
    }

    #[cfg(unix)]
    #[test]
    fn github_store_reuses_cached_issue_and_does_not_refetch_for_marker() {
        let (tmp, _storage, issue) = storage_with_issue();
        let (program, log) = fake_gh(&tmp);

        block_on_github(async {
            let store = GithubStore::new_with_program(None, program);
            let handle = store.issue("https://github.com/example/repo/issues/1");

            handle.get().await?;
            handle.get().await?;
            handle.ensure_marker(&issue).await?;
            handle.snapshot_for_state().await?;
            Ok(())
        })
        .unwrap();

        let calls = std::fs::read_to_string(log).unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("issue view "))
                .count(),
            1,
            "{calls}"
        );
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("issue comment "))
                .count(),
            1,
            "{calls}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn github_store_refetches_after_exported_comment_for_remote_comment_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, log) = fake_gh(&tmp);

        block_on_github(async {
            let store = GithubStore::new_with_program(None, program);
            let handle = store.issue("https://github.com/example/repo/issues/1");

            handle.get().await?;
            assert!(handle.add_comment("real comment").await?);
            handle.snapshot_for_state().await?;
            Ok(())
        })
        .unwrap();

        let calls = std::fs::read_to_string(log).unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("issue view "))
                .count(),
            2,
            "{calls}"
        );
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("issue comment "))
                .count(),
            1,
            "{calls}"
        );
    }
}
