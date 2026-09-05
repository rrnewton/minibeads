//! File replacement and recovery for a database operation holding its Lock.
//!
//! Stage a complete desired file set, save original bytes in a recovery journal,
//! and atomically replace each destination. Readers taking Lock see one result;
//! a failed operation or the next lock holder rolls an incomplete commit back.

use crate::paths::ensure_contained;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const JOURNAL: &str = "minibeads-transaction.json";

pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().context("File has no parent directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::Builder::new()
        .prefix(".mb-write-")
        .tempfile_in(parent)?;
    file.write_all(content)?;
    if let Ok(metadata) = fs::metadata(path) {
        file.as_file().set_permissions(metadata.permissions())?;
    }
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Backup {
    relative_path: PathBuf,
    content: Option<Vec<u8>>,
    modified: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) struct FileTransaction<'a> {
    root: &'a Path,
    desired: BTreeMap<PathBuf, Option<Vec<u8>>>,
    modified: BTreeMap<PathBuf, std::time::SystemTime>,
}

impl<'a> FileTransaction<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self {
            root,
            desired: BTreeMap::new(),
            modified: BTreeMap::new(),
        }
    }

    pub(crate) fn write(&mut self, path: PathBuf, content: Vec<u8>) {
        self.desired.insert(path, Some(content));
    }

    pub(crate) fn set_mtime(&mut self, path: PathBuf, modified: std::time::SystemTime) {
        self.modified.insert(path, modified);
    }

    pub(crate) fn remove(&mut self, path: PathBuf) {
        self.desired.insert(path, None);
    }

    pub(crate) fn config(&mut self, file: &str, key: &str, value: &str) -> Result<()> {
        let path = self.root.join(file);
        ensure_contained(self.root, &path)?;
        let content = if let Some(Some(bytes)) = self.desired.get(&path) {
            std::str::from_utf8(bytes)?.to_owned()
        } else {
            fs::read_to_string(&path)?
        };
        let prefix = format!("{key}:");
        let rendered = serde_yaml::to_string(value)?;
        let mut found = false;
        let mut lines = Vec::new();
        for line in content.lines() {
            if !found && line.trim_start().starts_with(&prefix) {
                let indent = &line[..line.len() - line.trim_start().len()];
                lines.push(format!("{indent}{key}: {}", rendered.trim()));
                found = true;
            } else {
                lines.push(line.to_owned());
            }
        }
        if !found {
            lines.push(format!("{key}: {}", rendered.trim()));
        }
        self.write(path, (lines.join("\n") + "\n").into_bytes());
        Ok(())
    }

    pub(crate) fn commit(self) -> Result<()> {
        self.commit_with_hook(|_| Ok(()))
    }

    fn commit_with_hook(self, mut after_write: impl FnMut(usize) -> Result<()>) -> Result<()> {
        if self.desired.is_empty() {
            return Ok(());
        }
        let journal = self.root.join(JOURNAL);
        anyhow::ensure!(
            !journal.exists(),
            "An earlier database transaction needs recovery"
        );
        let mut backups = Vec::with_capacity(self.desired.len());
        // Capture every source before overwriting any destination, including
        // destinations that are another renamed issue's source.
        for path in self.desired.keys() {
            ensure_contained(self.root, path)?;
            let relative_path = path.strip_prefix(self.root)?.to_path_buf();
            validate_relative(&relative_path)?;
            let (content, modified) = match fs::read(path) {
                Ok(content) => (Some(content), Some(fs::metadata(path)?.modified()?.into())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, None),
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("Failed to back up {}", path.display()))
                }
            };
            backups.push(Backup {
                relative_path,
                content,
                modified,
            });
        }
        ensure_contained(self.root, &journal)?;
        atomic_write(&journal, &serde_json::to_vec(&backups)?)?;
        let result = (|| {
            for (index, (path, content)) in self.desired.iter().enumerate() {
                ensure_contained(self.root, path)?;
                replace_or_remove(path, content.as_deref())?;
                if let Some(modified) = self.modified.get(path) {
                    filetime::set_file_mtime(
                        path,
                        filetime::FileTime::from_system_time(*modified),
                    )?;
                }
                after_write(index)?;
            }
            fs::remove_file(&journal)?;
            Ok(())
        })();
        if let Err(error) = result {
            recover(self.root).context(
                "Transaction failed and rollback could not complete; keep the recovery journal",
            )?;
            return Err(error);
        }
        Ok(())
    }
}

fn validate_relative(path: &Path) -> Result<()> {
    anyhow::ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && path != Path::new(JOURNAL)
            && path != Path::new("minibeads.lock"),
        "Invalid transaction path: {}",
        path.display()
    );
    Ok(())
}

fn replace_or_remove(path: &Path, content: Option<&[u8]>) -> Result<()> {
    if let Some(content) = content {
        atomic_write(path, content)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("Failed to remove {}", path.display()))
            }
        }
    }
}

/// Called only after acquiring the database lock. Recovery is idempotent: leave
/// the journal in place if restoration fails so another attempt can resume it.
pub(crate) fn recover(root: &Path) -> Result<()> {
    let journal = root.join(JOURNAL);
    if !journal.exists() {
        return Ok(());
    }
    ensure_contained(root, &journal)?;
    let backups: Vec<Backup> = serde_json::from_slice(&fs::read(&journal)?)
        .context("Invalid database recovery journal")?;
    for backup in &backups {
        validate_relative(&backup.relative_path)?;
        ensure_contained(root, &root.join(&backup.relative_path))?;
    }
    for backup in backups {
        let path = root.join(backup.relative_path);
        replace_or_remove(&path, backup.content.as_deref())?;
        if let Some(modified) = backup.modified {
            filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(modified.into()))?;
        }
    }
    fs::remove_file(journal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_multi_file_commit_restores_every_original() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.md");
        let second = temp.path().join("second.md");
        fs::write(&first, b"first original").unwrap();
        fs::write(&second, b"second original").unwrap();
        let mut transaction = FileTransaction::new(temp.path());
        transaction.write(first.clone(), b"replacement".to_vec());
        transaction.remove(second.clone());
        assert!(transaction
            .commit_with_hook(|_| anyhow::bail!("simulated write failure"))
            .is_err());
        assert_eq!(fs::read(&first).unwrap(), b"first original");
        assert_eq!(fs::read(&second).unwrap(), b"second original");
        assert!(!temp.path().join(JOURNAL).exists());
    }

    #[test]
    fn next_lock_holder_recovers_an_interrupted_commit() {
        let temp = tempfile::tempdir().unwrap();
        let backups = vec![
            Backup {
                relative_path: "old.md".into(),
                content: Some(b"original".to_vec()),
                modified: None,
            },
            Backup {
                relative_path: "new.md".into(),
                content: None,
                modified: None,
            },
        ];
        fs::write(
            temp.path().join(JOURNAL),
            serde_json::to_vec(&backups).unwrap(),
        )
        .unwrap();
        fs::write(temp.path().join("new.md"), b"partial result").unwrap();
        let _lock = crate::lock::Lock::acquire(temp.path()).unwrap();
        assert_eq!(fs::read(temp.path().join("old.md")).unwrap(), b"original");
        assert!(!temp.path().join("new.md").exists());
        assert!(!temp.path().join(JOURNAL).exists());
    }
}
