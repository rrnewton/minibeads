//! Validate identifiers before turning them into paths in the issue store.

use anyhow::{Context, Result};
use std::path::Path;

/// A single portable filename component, borrowed from an issue ID or prefix.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IssueId<'a>(&'a str);

impl<'a> IssueId<'a> {
    pub(crate) fn parse(value: &'a str) -> Result<Self> {
        let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
        if value.is_empty()
            || matches!(value, "." | "..")
            || value.trim() != value
            || value.ends_with('.')
            || device
            || value
                .chars()
                .any(|c| c.is_control() || r#"<>:"/\|?*"#.contains(c))
        {
            anyhow::bail!(
                "Invalid issue ID or prefix {value:?}: expected one portable filename component"
            );
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(self) -> &'a str {
        self.0
    }
}

/// Check existing ancestors too, so a symlink or Windows junction cannot send
/// an otherwise valid ID outside the explicitly selected database directory.
pub(crate) fn ensure_contained(root: &Path, path: &Path) -> Result<()> {
    anyhow::ensure!(
        path.starts_with(root),
        "Path is outside the database: {}",
        path.display()
    );
    let canonical_root = root
        .canonicalize()
        .context("Failed to resolve database directory")?;
    let mut existing = path;
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().context("Path has no existing ancestor")?;
            }
            Err(error) => return Err(error).context("Failed to inspect database path"),
        }
    }
    let resolved = existing
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", existing.display()))?;
    anyhow::ensure!(
        resolved.starts_with(canonical_root),
        "Path escapes the database: {}",
        path.display()
    );
    Ok(())
}
