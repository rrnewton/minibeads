//! GitHub Issues sync using the authenticated `gh` CLI.

use crate::storage::Storage;
use crate::types::{Comment, Issue, IssueType, Status};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, rngs::StdRng, Rng, SeedableRng};
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
    pub steps: usize,
    pub seed: u64,
    pub issues_created: usize,
    pub github_urls: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GithubImportOptions {
    pub repo: Option<String>,
    pub state: Option<String>,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub mention: Option<String>,
    pub milestone: Option<String>,
    pub app: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubImportReport {
    pub imported: usize,
    pub skipped_existing: usize,
    #[serde(default)]
    pub issues: Vec<GithubImportedIssueReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubImportedIssueReport {
    pub github_url: String,
    pub title: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<String>,
    pub comments_imported: usize,
    #[serde(default)]
    pub details: Vec<String>,
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

pub fn import_issues(
    storage: &Storage,
    options: &GithubImportOptions,
) -> Result<GithubImportReport> {
    block_on_github(import_issues_async(storage, options))
}

async fn import_issues_async(
    storage: &Storage,
    options: &GithubImportOptions,
) -> Result<GithubImportReport> {
    let store = GithubStore::new(options.repo.as_deref());
    import_issues_with_store(storage, options, &store).await
}

async fn import_issues_with_store(
    storage: &Storage,
    options: &GithubImportOptions,
    store: &GithubStore,
) -> Result<GithubImportReport> {
    let remote_issues = list_remote_issues(options, store).await?;
    let existing_refs: HashSet<String> = storage
        .list_issues(None, None, None, None, None)?
        .into_iter()
        .filter_map(|issue| issue.external_ref)
        .collect();
    let mut report = GithubImportReport {
        imported: 0,
        skipped_existing: 0,
        issues: Vec::new(),
    };

    for remote in remote_issues {
        let mut item = GithubImportedIssueReport {
            github_url: remote.url.clone(),
            title: remote.title.clone(),
            action: "skipped-existing".to_string(),
            issue_id: None,
            comments_imported: 0,
            details: Vec::new(),
        };

        if existing_refs.contains(&remote.url) {
            report.skipped_existing += 1;
            item.details
                .push("GitHub URL is already linked to a minibeads issue".to_string());
            report.issues.push(item);
            continue;
        }

        if options.dry_run {
            report.imported += 1;
            item.action = "would-import".to_string();
            item.comments_imported = remote
                .comments
                .iter()
                .filter(|comment| !is_marker_comment(comment))
                .count();
            item.details
                .push("would create a new linked minibeads issue".to_string());
            report.issues.push(item);
            continue;
        }

        let issue = storage.create_issue(
            remote.title.clone(),
            remote.body.clone(),
            None,
            None,
            2,
            IssueType::Task,
            None,
            Vec::new(),
            Some(remote.url.clone()),
            None,
            Vec::new(),
        )?;
        let issue = if remote.state.eq_ignore_ascii_case("closed") {
            storage.close_issue(&issue.id, "Imported closed GitHub issue")?
        } else {
            issue
        };
        let comments_imported = import_remote_comments(storage, &issue, &remote)?;
        let handle = store.issue(&remote.url);
        handle.ensure_marker(&issue).await?;
        let remote_for_state = handle.snapshot_for_state().await?;
        let comments = storage.list_comments(&issue.id)?;
        remember_state(
            &storage.get_beads_dir(),
            &issue,
            &remote_for_state,
            &comments,
        )?;

        report.imported += 1;
        item.action = "imported".to_string();
        item.issue_id = Some(issue.id);
        item.comments_imported = comments_imported;
        item.details
            .push("created local issue with GitHub URL in external_ref".to_string());
        if remote.state.eq_ignore_ascii_case("closed") {
            item.details
                .push("created local issue as closed to match GitHub".to_string());
        }
        if comments_imported > 0 {
            item.details
                .push(format!("imported {} GitHub comment(s)", comments_imported));
        }
        report.issues.push(item);
    }

    Ok(report)
}

async fn list_remote_issues(
    options: &GithubImportOptions,
    store: &GithubStore,
) -> Result<Vec<RemoteIssue>> {
    let mut args = vec![
        "issue".to_string(),
        "list".to_string(),
        "--json".to_string(),
        "number,url,title,body,state,comments".to_string(),
    ];
    if let Some(repo) = &options.repo {
        args.push("--repo".to_string());
        args.push(repo.clone());
    }
    if let Some(state) = &options.state {
        args.push("--state".to_string());
        args.push(state.clone());
    }
    for label in &options.labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }
    if let Some(assignee) = &options.assignee {
        args.push("--assignee".to_string());
        args.push(assignee.clone());
    }
    if let Some(author) = &options.author {
        args.push("--author".to_string());
        args.push(author.clone());
    }
    if let Some(mention) = &options.mention {
        args.push("--mention".to_string());
        args.push(mention.clone());
    }
    if let Some(milestone) = &options.milestone {
        args.push("--milestone".to_string());
        args.push(milestone.clone());
    }
    if let Some(app) = &options.app {
        args.push("--app".to_string());
        args.push(app.clone());
    }
    if let Some(search) = &options.search {
        args.push("--search".to_string());
        args.push(search.clone());
    }
    if let Some(limit) = options.limit {
        args.push("--limit".to_string());
        args.push(limit.to_string());
    }

    let value = store.gh_json(args).await?;
    value
        .as_array()
        .ok_or_else(|| anyhow!("gh issue list returned non-array JSON"))?
        .iter()
        .cloned()
        .map(parse_remote_issue)
        .collect()
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
        let removed_marker_comments = if dry_run {
            0
        } else {
            purge_local_marker_comments(storage, &issue.id)?
        };
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
        let mut field_conflict = false;
        if removed_marker_comments > 0 {
            item.details.push(format!(
                "removed {} local marker comment(s)",
                removed_marker_comments
            ));
        }

        match (
            old_state.is_some(),
            inherited_divergence,
            local_changed,
            remote_changed,
        ) {
            (false, _, _, _) => {
                if !dry_run {
                    handle.edit_fields(&issue).await?;
                    handle.set_state(issue.status).await?;
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
                        handle.set_state(issue.status).await?;
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
                field_conflict = true;
            }
            (_, _, true, false) => {
                if !dry_run {
                    handle.edit_fields(&issue).await?;
                    handle.set_state(issue.status).await?;
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
                .filter(|c| {
                    c.source_id.is_none()
                        && !is_local_marker_comment(c)
                        && !synced_local_ids.contains(&c.id)
                })
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

        if !dry_run && !field_conflict {
            let remote = if remote_written {
                handle.snapshot_for_state().await?
            } else {
                remote
            };
            let comments = storage.list_comments(&issue.id)?;
            update_state_entry(&mut state, &issue, &remote, &comments);
        } else if field_conflict {
            item.details.push(
                "left GitHub sync ancestry unchanged until the conflict is resolved".to_string(),
            );
        }

        report.issues.push(item);
    }

    if !dry_run {
        save_state(&beads_dir, &state)?;
    }

    Ok(report)
}

pub fn stress_test(
    repo: &str,
    iterations: usize,
    steps: usize,
    seed: Option<u64>,
    adversarial: bool,
    verbose: bool,
) -> Result<GithubStressReport> {
    if iterations == 0 {
        return Err(anyhow!("--iterations must be greater than zero"));
    }
    if steps == 0 {
        return Err(anyhow!("--steps must be greater than zero"));
    }

    let tmp = tempfile::tempdir().context("Failed to create temporary stress workspace")?;
    let storage = Storage::init(
        tmp.path().join(".beads"),
        Some("ghstress".to_string()),
        false,
    )
    .context("Failed to initialize temporary stress minibeads database")?;
    let seed = seed.unwrap_or_else(|| rand::thread_rng().gen());
    let mut rng = StdRng::seed_from_u64(seed);
    let run_id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    if verbose {
        eprintln!(
            "GitHub sync stress test: repo={}, iterations={}, steps={}, seed={}, run={}, mode={}",
            repo,
            iterations,
            steps,
            seed,
            run_id,
            if adversarial {
                "adversarial"
            } else {
                "isolated"
            }
        );
    }

    if adversarial {
        let context = AdversarialStressContext {
            repo,
            iterations,
            steps,
            seed,
            run_id: &run_id,
            verbose,
        };
        return stress_test_adversarial(&storage, &context, &mut rng);
    }

    let mut urls = Vec::new();
    for i in 0..iterations {
        if verbose {
            eprintln!(
                "stress issue {}/{}: creating local issue",
                i + 1,
                iterations
            );
        }
        let title = format!("mb gh sync stress {run_id} issue {i}");
        let body = format!("initial local body {run_id} issue {i}");
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
        if verbose {
            eprintln!("stress issue {}/{}: published {}", i + 1, iterations, url);
        }

        let remote = gh_issue_view(&url, Some(repo))?;
        assert_remote_matches(&remote, &title, &body, Status::Open)?;
        assert_marker_present(&remote, &issue.id)?;

        let mut expected = StressExpected {
            title,
            body,
            status: Status::Open,
            local_comment_bodies: Vec::new(),
            remote_comment_bodies: Vec::new(),
        };
        assert_stress_converged(&storage, &issue.id, &url, Some(repo), &expected)?;
        if verbose {
            eprintln!(
                "stress issue {}/{}: initial convergence verified",
                i + 1,
                iterations
            );
        }

        for step in 0..steps {
            let action = rng.gen_range(0..8);
            let action_desc;
            match action {
                0 => {
                    expected.title = format!("local title {run_id} issue {i} step {step}");
                    expected.body = format!("local body {run_id} issue {i} step {step}");
                    action_desc = "local title/body edit";
                    storage.update_issue(
                        &issue.id,
                        HashMap::from([
                            ("title".to_string(), expected.title.clone()),
                            ("description".to_string(), expected.body.clone()),
                        ]),
                    )?;
                }
                1 => {
                    expected.title = format!("remote title {run_id} issue {i} step {step}");
                    expected.body = format!("remote body {run_id} issue {i} step {step}");
                    action_desc = "GitHub title/body edit";
                    gh_status(&[
                        "issue",
                        "edit",
                        &url,
                        "--repo",
                        repo,
                        "--title",
                        &expected.title,
                        "--body",
                        &expected.body,
                    ])?;
                }
                2 => {
                    let body = format!("local comment {run_id} issue {i} step {step}");
                    action_desc = "local comment";
                    storage.add_comment(&issue.id, "stress-local", &body)?;
                    expected.local_comment_bodies.push(body);
                }
                3 => {
                    let body = format!("remote comment {run_id} issue {i} step {step}");
                    action_desc = "GitHub comment";
                    let remote_for_comment = gh_issue_view(&url, Some(repo))?;
                    gh_issue_comment(&remote_for_comment, &body, Some(repo))?;
                    expected.remote_comment_bodies.push(body);
                }
                4 => {
                    action_desc = "local close";
                    storage.close_issue(&issue.id, "stress local close")?;
                    expected.status = Status::Closed;
                }
                5 => {
                    action_desc = "local reopen";
                    storage.reopen_issue(&issue.id)?;
                    expected.status = Status::Open;
                }
                6 => {
                    action_desc = "GitHub close";
                    let remote = gh_issue_view(&url, Some(repo))?;
                    if !remote.state.eq_ignore_ascii_case("closed") {
                        gh_status(&["issue", "close", &url, "--repo", repo])?;
                    }
                    expected.status = Status::Closed;
                }
                _ => {
                    action_desc = "GitHub reopen";
                    let remote = gh_issue_view(&url, Some(repo))?;
                    if remote.state.eq_ignore_ascii_case("closed") {
                        gh_status(&["issue", "reopen", &url, "--repo", repo])?;
                    }
                    expected.status = Status::Open;
                }
            }
            if verbose {
                eprintln!(
                    "stress issue {}/{} step {}/{}: {}",
                    i + 1,
                    iterations,
                    step + 1,
                    steps,
                    action_desc
                );
            }

            sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
                .with_context(|| format!("stress sync failed for {} at step {}", issue.id, step))?;
            assert_stress_converged(&storage, &issue.id, &url, Some(repo), &expected)
                .with_context(|| format!("stress convergence failed at step {}", step))?;
            if verbose {
                eprintln!(
                    "stress issue {}/{} step {}/{}: sync convergence verified",
                    i + 1,
                    iterations,
                    step + 1,
                    steps
                );
            }

            sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
                .with_context(|| {
                    format!("stress no-op sync failed for {} at step {}", issue.id, step)
                })?;
            assert_stress_converged(&storage, &issue.id, &url, Some(repo), &expected)
                .with_context(|| format!("stress no-op convergence failed at step {}", step))?;
            if verbose {
                eprintln!(
                    "stress issue {}/{} step {}/{}: no-op sync verified",
                    i + 1,
                    iterations,
                    step + 1,
                    steps
                );
            }
        }

        if verbose {
            eprintln!(
                "stress issue {}/{}: closing temporary issue",
                i + 1,
                iterations
            );
        }
        storage.close_issue(&issue.id, "stress complete")?;
        expected.status = Status::Closed;
        sync_linked(&storage, std::slice::from_ref(&issue.id), Some(repo), false)
            .with_context(|| format!("stress close sync failed for {}", issue.id))?;
        assert_stress_converged(&storage, &issue.id, &url, Some(repo), &expected)?;
        if verbose {
            eprintln!(
                "stress issue {}/{}: final closed convergence verified",
                i + 1,
                iterations
            );
        }
    }

    Ok(GithubStressReport {
        repo: repo.to_string(),
        iterations,
        steps,
        seed,
        issues_created: urls.len(),
        github_urls: urls,
    })
}

struct StressExpected {
    title: String,
    body: String,
    status: Status,
    local_comment_bodies: Vec<String>,
    remote_comment_bodies: Vec<String>,
}

struct AdversarialIssue {
    id: String,
    url: String,
    local_title: String,
    local_body: String,
    local_status: Status,
    remote_title: String,
    remote_body: String,
    remote_status: Status,
    field_conflict: bool,
    local_comment_bodies: Vec<String>,
    remote_comment_bodies: Vec<String>,
}

struct AdversarialStressContext<'a> {
    repo: &'a str,
    iterations: usize,
    steps: usize,
    seed: u64,
    run_id: &'a str,
    verbose: bool,
}

fn stress_test_adversarial(
    storage: &Storage,
    context: &AdversarialStressContext<'_>,
    rng: &mut StdRng,
) -> Result<GithubStressReport> {
    let mut issues = Vec::new();
    for i in 0..context.iterations {
        if context.verbose {
            eprintln!(
                "adversarial issue {}/{}: creating and publishing",
                i + 1,
                context.iterations
            );
        }
        let title = format!("mb gh sync adversarial {} issue {i}", context.run_id);
        let body = format!("initial adversarial body {} issue {i}", context.run_id);
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
        let publish = publish_issue(storage, &issue.id, Some(context.repo), false)
            .with_context(|| format!("adversarial publish failed for {}", issue.id))?;
        let url = publish
            .issues
            .first()
            .map(|issue| issue.github_url.clone())
            .ok_or_else(|| anyhow!("publish did not return a GitHub URL"))?;
        let model = AdversarialIssue {
            id: issue.id,
            url,
            local_title: title.clone(),
            local_body: body.clone(),
            local_status: Status::Open,
            remote_title: title,
            remote_body: body,
            remote_status: Status::Open,
            field_conflict: false,
            local_comment_bodies: Vec::new(),
            remote_comment_bodies: Vec::new(),
        };
        assert_adversarial_issue(storage, Some(context.repo), &model)?;
        issues.push(model);
    }

    let issue_ids = issues
        .iter()
        .map(|issue| issue.id.clone())
        .collect::<Vec<_>>();
    for step in 0..context.steps {
        if context.verbose {
            eprintln!(
                "adversarial round {}/{}: mutating {} issue(s) before one batch sync",
                step + 1,
                context.steps,
                issues.len()
            );
        }
        for (idx, model) in issues.iter_mut().enumerate() {
            apply_adversarial_mutation(storage, context, step, idx, model, rng)?;
        }

        let report = sync_linked(storage, &issue_ids, Some(context.repo), false)
            .with_context(|| format!("adversarial batch sync failed at round {}", step))?;
        assert_adversarial_batch(storage, Some(context.repo), &issues, &report)
            .with_context(|| format!("adversarial convergence failed at round {}", step))?;

        let noop = sync_linked(storage, &issue_ids, Some(context.repo), false)
            .with_context(|| format!("adversarial no-op sync failed at round {}", step))?;
        assert_adversarial_batch(storage, Some(context.repo), &issues, &noop)
            .with_context(|| format!("adversarial no-op check failed at round {}", step))?;
    }

    for model in &issues {
        let remote = gh_issue_view(&model.url, Some(context.repo))?;
        if !remote.state.eq_ignore_ascii_case("closed") {
            gh_status(&["issue", "close", &model.url, "--repo", context.repo])?;
        }
    }

    Ok(GithubStressReport {
        repo: context.repo.to_string(),
        iterations: context.iterations,
        steps: context.steps,
        seed: context.seed,
        issues_created: issues.len(),
        github_urls: issues.into_iter().map(|issue| issue.url).collect(),
    })
}

fn apply_adversarial_mutation(
    storage: &Storage,
    context: &AdversarialStressContext<'_>,
    step: usize,
    issue_index: usize,
    model: &mut AdversarialIssue,
    rng: &mut StdRng,
) -> Result<()> {
    let mut action = rng.gen_range(0..8);
    if model.field_conflict && action <= 5 {
        action = 6 + rng.gen_range(0..2);
    }

    let action_desc;
    match action {
        0 => {
            action_desc = "local title/body edit";
            let title = format!(
                "local adversarial title {} {issue_index} {step}",
                context.run_id
            );
            let body = format!(
                "local adversarial body {} {issue_index} {step}",
                context.run_id
            );
            storage.update_issue(
                &model.id,
                HashMap::from([
                    ("title".to_string(), title.clone()),
                    ("description".to_string(), body.clone()),
                ]),
            )?;
            model.local_title = title.clone();
            model.local_body = body.clone();
            model.remote_title = title;
            model.remote_body = body;
        }
        1 => {
            action_desc = "GitHub title/body edit";
            let title = format!(
                "remote adversarial title {} {issue_index} {step}",
                context.run_id
            );
            let body = format!(
                "remote adversarial body {} {issue_index} {step}",
                context.run_id
            );
            gh_status(&[
                "issue",
                "edit",
                &model.url,
                "--repo",
                context.repo,
                "--title",
                &title,
                "--body",
                &body,
            ])?;
            model.local_title = title.clone();
            model.local_body = body.clone();
            model.remote_title = title;
            model.remote_body = body;
        }
        2 => {
            action_desc = "both-side title/body conflict";
            let local_title = format!(
                "conflict local title {} {issue_index} {step}",
                context.run_id
            );
            let local_body = format!(
                "conflict local body {} {issue_index} {step}",
                context.run_id
            );
            let remote_title = format!(
                "conflict remote title {} {issue_index} {step}",
                context.run_id
            );
            let remote_body = format!(
                "conflict remote body {} {issue_index} {step}",
                context.run_id
            );
            storage.update_issue(
                &model.id,
                HashMap::from([
                    ("title".to_string(), local_title.clone()),
                    ("description".to_string(), local_body.clone()),
                ]),
            )?;
            gh_status(&[
                "issue",
                "edit",
                &model.url,
                "--repo",
                context.repo,
                "--title",
                &remote_title,
                "--body",
                &remote_body,
            ])?;
            model.local_title = local_title;
            model.local_body = local_body;
            model.remote_title = remote_title;
            model.remote_body = remote_body;
            model.field_conflict = true;
        }
        3 => {
            action_desc = "local close";
            storage.close_issue(&model.id, "adversarial local close")?;
            model.local_status = Status::Closed;
            model.remote_status = Status::Closed;
        }
        4 => {
            action_desc = "GitHub close";
            let remote = gh_issue_view(&model.url, Some(context.repo))?;
            if !remote.state.eq_ignore_ascii_case("closed") {
                gh_status(&["issue", "close", &model.url, "--repo", context.repo])?;
            }
            model.local_status = Status::Closed;
            model.remote_status = Status::Closed;
        }
        5 => {
            action_desc = "local status / GitHub field conflict";
            let remote_title = format!(
                "status conflict remote title {} {issue_index} {step}",
                context.run_id
            );
            let remote_body = format!(
                "status conflict remote body {} {issue_index} {step}",
                context.run_id
            );
            if model.local_status == Status::Closed {
                storage.reopen_issue(&model.id)?;
                model.local_status = Status::Open;
            } else {
                storage.close_issue(&model.id, "adversarial status conflict")?;
                model.local_status = Status::Closed;
            }
            gh_status(&[
                "issue",
                "edit",
                &model.url,
                "--repo",
                context.repo,
                "--title",
                &remote_title,
                "--body",
                &remote_body,
            ])?;
            model.remote_title = remote_title;
            model.remote_body = remote_body;
            model.field_conflict = true;
        }
        6 => {
            action_desc = "local comment";
            let body = format!(
                "local adversarial comment {} {issue_index} {step}",
                context.run_id
            );
            storage.add_comment(&model.id, "stress-local", &body)?;
            model.local_comment_bodies.push(body);
        }
        _ => {
            action_desc = "GitHub comment";
            let body = format!(
                "remote adversarial comment {} {issue_index} {step}",
                context.run_id
            );
            let remote = gh_issue_view(&model.url, Some(context.repo))?;
            gh_issue_comment(&remote, &body, Some(context.repo))?;
            model.remote_comment_bodies.push(body);
        }
    }

    if context.verbose {
        eprintln!(
            "adversarial round {} issue {}: {}{}",
            step + 1,
            model.id,
            action_desc,
            if model.field_conflict {
                " (conflicted)"
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn assert_adversarial_batch(
    storage: &Storage,
    repo: Option<&str>,
    issues: &[AdversarialIssue],
    report: &GithubSyncReport,
) -> Result<()> {
    for issue in issues {
        assert_adversarial_issue(storage, repo, issue)?;
        let has_report_conflict = report
            .issues
            .iter()
            .any(|item| item.issue_id == issue.id && item.conflict.is_some());
        if issue.field_conflict != has_report_conflict {
            return Err(anyhow!(
                "conflict report mismatch for {}: expected {}, got {}",
                issue.id,
                issue.field_conflict,
                has_report_conflict
            ));
        }
    }
    Ok(())
}

fn assert_adversarial_issue(
    storage: &Storage,
    repo: Option<&str>,
    expected: &AdversarialIssue,
) -> Result<()> {
    let local = storage
        .get_issue(&expected.id)?
        .ok_or_else(|| anyhow!("local adversarial issue disappeared: {}", expected.id))?;
    if local.title != expected.local_title
        || local.description != expected.local_body
        || local.status != expected.local_status
    {
        return Err(anyhow!(
            "local adversarial state mismatch for {}",
            expected.id
        ));
    }

    let remote = gh_issue_view(&expected.url, repo)?;
    assert_remote_matches(
        &remote,
        &expected.remote_title,
        &expected.remote_body,
        expected.remote_status,
    )?;
    assert_marker_present(&remote, &expected.id)?;
    assert_marker_not_imported(storage, &expected.id)?;

    let local_comments = storage.list_comments(&expected.id)?;
    for body in &expected.remote_comment_bodies {
        let count = local_comments
            .iter()
            .filter(|comment| comment.body == *body)
            .count();
        if count != 1 {
            return Err(anyhow!(
                "remote adversarial comment imported {} times for {}",
                count,
                expected.id
            ));
        }
    }

    for body in &expected.local_comment_bodies {
        let rendered = format!(
            "_From minibeads {} by stress-local_\n\n{}",
            expected.id, body
        );
        let count = remote
            .comments
            .iter()
            .filter(|comment| comment.body == rendered)
            .count();
        if count != 1 {
            return Err(anyhow!(
                "local adversarial comment exported {} times for {}",
                count,
                expected.id
            ));
        }
    }
    Ok(())
}

fn assert_stress_converged(
    storage: &Storage,
    issue_id: &str,
    url: &str,
    repo: Option<&str>,
    expected: &StressExpected,
) -> Result<()> {
    let local = storage
        .get_issue(issue_id)?
        .ok_or_else(|| anyhow!("local stress issue disappeared: {}", issue_id))?;
    if local.title != expected.title {
        return Err(anyhow!(
            "local title mismatch for {}: expected {:?}, got {:?}",
            issue_id,
            expected.title,
            local.title
        ));
    }
    if local.description != expected.body {
        return Err(anyhow!("local body mismatch for {}", issue_id));
    }
    if local.status != expected.status {
        return Err(anyhow!(
            "local status mismatch for {}: expected {}, got {}",
            issue_id,
            expected.status,
            local.status
        ));
    }

    let remote = gh_issue_view(url, repo)?;
    assert_remote_matches(&remote, &expected.title, &expected.body, expected.status)?;
    assert_marker_present(&remote, issue_id)?;
    assert_marker_not_imported(storage, issue_id)?;

    let local_comments = storage.list_comments(issue_id)?;
    for body in &expected.remote_comment_bodies {
        let count = local_comments
            .iter()
            .filter(|comment| comment.body == *body)
            .count();
        if count != 1 {
            return Err(anyhow!(
                "remote comment imported {} times for {}: {:?}",
                count,
                issue_id,
                body
            ));
        }
    }

    for body in &expected.local_comment_bodies {
        let rendered = format!("_From minibeads {} by stress-local_\n\n{}", issue_id, body);
        let count = remote
            .comments
            .iter()
            .filter(|comment| comment.body == rendered)
            .count();
        if count != 1 {
            return Err(anyhow!(
                "local comment exported {} times for {}: {:?}",
                count,
                issue_id,
                body
            ));
        }
    }

    Ok(())
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
    purge_local_marker_comments(storage, &issue.id)?;
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

fn purge_local_marker_comments(storage: &Storage, issue_id: &str) -> Result<usize> {
    storage.remove_comments_containing(issue_id, MARKER)
}

async fn export_new_local_comments(
    issue: &Issue,
    remote: &GithubIssueHandle,
    comments: &[Comment],
    synced_local_ids: &HashSet<String>,
) -> Result<usize> {
    let mut exported = 0;
    for comment in comments {
        if comment.source_id.is_some()
            || is_local_marker_comment(comment)
            || synced_local_ids.contains(&comment.id)
        {
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
            synced_local_comment_ids: comments
                .iter()
                .filter(|c| !is_local_marker_comment(c))
                .map(|c| c.id.clone())
                .collect(),
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

fn is_local_marker_comment(comment: &Comment) -> bool {
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

    #[cfg(unix)]
    fn fake_gh_for_import(tmp: &tempfile::TempDir) -> (String, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let log = tmp.path().join("gh-import.log");
        let list_json = tmp.path().join("issue-list.json");
        let view_json = tmp.path().join("issue-view.json");
        let script = tmp.path().join("gh-import-fake");
        let comment = r#"{"id":"comment-2","url":"https://github.com/example/repo/issues/2#issuecomment-2","author":{"login":"octo"},"body":"remote import comment","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}"#;
        std::fs::write(
            &list_json,
            format!(
                r#"[
{{"url":"https://github.com/example/repo/issues/1","title":"Existing remote","body":"Existing remote body","state":"OPEN","comments":[]}},
{{"url":"https://github.com/example/repo/issues/2","title":"New remote","body":"New remote body","state":"CLOSED","comments":[{}]}}
]"#,
                comment
            ),
        )
        .unwrap();
        std::fs::write(
            &view_json,
            format!(
                r#"{{"url":"https://github.com/example/repo/issues/2","title":"New remote","body":"New remote body","state":"CLOSED","comments":[{}]}}"#,
                comment
            ),
        )
        .unwrap();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\ncase \"$1 $2\" in\n  'issue list') cat {} ;;\n  'issue view') cat {} ;;\n  'issue comment') exit 0 ;;\n  *) echo \"unexpected gh args: $*\" >&2; exit 99 ;;\nesac\n",
                shell_quote_arg(&log.to_string_lossy()),
                shell_quote_arg(&list_json.to_string_lossy()),
                shell_quote_arg(&view_json.to_string_lossy()),
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
    fn stale_local_marker_comments_are_removed_during_import() {
        let (_tmp, storage, issue) = storage_with_issue();
        storage
            .upsert_comments(
                &issue.id,
                vec![Comment {
                    id: "gh-marker".to_string(),
                    issue_id: issue.id.clone(),
                    author: "github-user".to_string(),
                    body: marker_body(&issue),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    source_url: Some(
                        "https://github.com/example/repo/issues/1#issuecomment-marker".to_string(),
                    ),
                    source_id: Some("marker".to_string()),
                }],
            )
            .unwrap();

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
    fn github_import_creates_only_unlinked_issues() {
        let (tmp, storage, _issue) = storage_with_issue();
        storage
            .create_issue(
                "Already linked".to_string(),
                "Existing body".to_string(),
                None,
                None,
                2,
                IssueType::Task,
                None,
                Vec::new(),
                Some("https://github.com/example/repo/issues/1".to_string()),
                None,
                Vec::new(),
            )
            .unwrap();
        let (program, log) = fake_gh_for_import(&tmp);
        let store = GithubStore::new_with_program(Some("example/repo"), program);
        let options = GithubImportOptions {
            repo: Some("example/repo".to_string()),
            state: Some("all".to_string()),
            labels: vec!["bug".to_string()],
            limit: Some(10),
            ..GithubImportOptions::default()
        };

        let report = block_on_github(import_issues_with_store(&storage, &options, &store)).unwrap();

        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped_existing, 1);
        let imported = storage
            .list_issues(None, None, None, None, None)
            .unwrap()
            .into_iter()
            .find(|issue| {
                issue.external_ref.as_deref() == Some("https://github.com/example/repo/issues/2")
            })
            .unwrap();
        assert_eq!(imported.title, "New remote");
        assert_eq!(imported.description, "New remote body");
        assert_eq!(imported.status, Status::Closed);

        let comments = storage.list_comments(&imported.id).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "remote import comment");
        assert_eq!(comments[0].source_id.as_deref(), Some("comment-2"));

        let state = load_state(&storage.get_beads_dir()).unwrap();
        let entry = state
            .issues
            .get("https://github.com/example/repo/issues/2")
            .unwrap();
        assert_eq!(entry.local_id, imported.id);
        assert_eq!(entry.synced_remote_comment_ids, vec!["comment-2"]);

        let calls = std::fs::read_to_string(log).unwrap();
        assert!(
            calls.contains(
                "issue list --json number,url,title,body,state,comments --repo example/repo --state all --label bug --limit 10"
            ),
            "{calls}"
        );
        assert!(
            calls.contains("issue comment https://github.com/example/repo/issues/2 --body"),
            "{calls}"
        );
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
