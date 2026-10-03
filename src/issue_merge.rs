//! Three-way merge of minibeads issue files and comment files.
//!
//! This is the engine behind `mb merge-driver`. Given the common ancestor and
//! two descendants of one `issues/<id>.md` or `comments/<id>.json` file it
//! produces either a clean merge or a file whose conflict hunks cover only the
//! fields or comments that genuinely disagree.
//!
//! Issue files merge field by field: scalars take whichever side changed them,
//! labels merge as a set, dependencies as a keyed map, timestamps by derivation
//! rules, and the four prose sections through the deterministic prose merger
//! shared with GitHub sync. Comment files merge as an append-only set keyed by
//! the stable comment ID: no comment present on either side is ever dropped and
//! no ID appears twice.
//!
//! Inputs that do not parse, or that would not be reproduced byte-for-byte by
//! re-serialising them (hand-edited or foreign files), fall back to a plain
//! line-based diff3 so no unknown content is silently discarded.

use crate::diff3::{merge_lines, push_conflict, MarkerSize, ThreeWay};
use crate::format::{
    frontmatter_yaml, issue_to_markdown, markdown_to_issue, push_section_heading,
    sanitize_section_content, trim_blank_edge_lines, Section,
};
use crate::github_merge::ProseMergeFailureKind;
use crate::prose_merge::{merge_prose, ProseMergeLimits, ProseMergeResult};
use crate::types::{Comment, Issue, Status};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

/// One of the three inputs to a merge.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Side {
    Base,
    Ours,
    Theirs,
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Side::Base => "base",
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        })
    }
}

/// A mergeable unit of an issue file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum IssueField {
    Title,
    Status,
    Priority,
    IssueType,
    Assignee,
    ExternalRef,
    ClaimedAt,
    ClaimedUntil,
    /// The dependency on the named issue.
    Dependency(String),
    Section(Section),
}

impl IssueField {
    fn is_frontmatter(&self) -> bool {
        !matches!(self, IssueField::Section(_))
    }

    /// Copy this field's value (and anything derived with it) from `source`.
    fn copy_value(&self, target: &mut Issue, source: &Issue) {
        match self {
            IssueField::Title => target.title.clone_from(&source.title),
            IssueField::Status => target.status = source.status,
            IssueField::Priority => target.priority = source.priority,
            IssueField::IssueType => target.issue_type = source.issue_type,
            IssueField::Assignee => target.assignee.clone_from(&source.assignee),
            IssueField::ExternalRef => target.external_ref.clone_from(&source.external_ref),
            IssueField::ClaimedAt => target.claimed_at = source.claimed_at,
            IssueField::ClaimedUntil => target.claimed_until = source.claimed_until,
            IssueField::Dependency(id) => match source.depends_on.get(id) {
                Some(dep_type) => {
                    target.depends_on.insert(id.clone(), *dep_type);
                }
                None => {
                    target.depends_on.remove(id);
                }
            },
            IssueField::Section(section) => {
                *section.content_mut(target) = section.content(source).to_owned();
            }
        }
    }
}

impl fmt::Display for IssueField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueField::Title => formatter.write_str("title"),
            IssueField::Status => formatter.write_str("status"),
            IssueField::Priority => formatter.write_str("priority"),
            IssueField::IssueType => formatter.write_str("issue_type"),
            IssueField::Assignee => formatter.write_str("assignee"),
            IssueField::ExternalRef => formatter.write_str("external_ref"),
            IssueField::ClaimedAt => formatter.write_str("claimed_at"),
            IssueField::ClaimedUntil => formatter.write_str("claimed_until"),
            IssueField::Dependency(id) => write!(formatter, "dependency {id}"),
            IssueField::Section(section) => write!(formatter, "section \"{}\"", section.heading()),
        }
    }
}

/// Why a file was merged as plain lines instead of structurally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextualFallback {
    Unparseable(Side),
    NonCanonical(Side),
    DuplicateCommentId(Side),
}

impl fmt::Display for TextualFallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextualFallback::Unparseable(side) => write!(formatter, "{side} does not parse"),
            TextualFallback::NonCanonical(side) => {
                write!(formatter, "{side} is not in canonical mb format")
            }
            TextualFallback::DuplicateCommentId(side) => {
                write!(formatter, "{side} repeats a comment ID")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConflictReason {
    /// Both sides changed the value differently since the common ancestor.
    BothChanged,
    /// The file has no common ancestor and the sides differ.
    NoCommonAncestor,
    /// The prose merger refused to combine the edits.
    Prose(ProseMergeFailureKind),
    /// The file was merged textually and this hunk did not merge.
    Textual(TextualFallback),
}

impl fmt::Display for ConflictReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictReason::BothChanged => formatter.write_str("both sides changed it"),
            ConflictReason::NoCommonAncestor => {
                formatter.write_str("no common ancestor and the sides differ")
            }
            ConflictReason::Prose(ProseMergeFailureKind::CompetingEdit(reason)) => {
                write!(formatter, "competing prose edits ({reason:?})")
            }
            ConflictReason::Prose(ProseMergeFailureKind::UnsupportedStructuredOverlap(reason)) => {
                write!(
                    formatter,
                    "overlapping structured Markdown edits ({reason:?})"
                )
            }
            ConflictReason::Prose(ProseMergeFailureKind::WorkExhausted(reason)) => {
                write!(formatter, "prose merge work limit reached ({reason:?})")
            }
            ConflictReason::Textual(fallback) => {
                write!(
                    formatter,
                    "overlapping line edits; merged as text because {fallback}"
                )
            }
        }
    }
}

/// What a conflict is about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConflictSubject {
    Field(IssueField),
    /// The comment with this ID.
    Comment(String),
    /// A hunk of a file merged as plain text.
    Lines,
}

impl fmt::Display for ConflictSubject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictSubject::Field(field) => write!(formatter, "field {field}"),
            ConflictSubject::Comment(id) => write!(formatter, "comment {id}"),
            ConflictSubject::Lines => formatter.write_str("lines"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MergeConflict {
    pub(crate) subject: ConflictSubject,
    pub(crate) reason: ConflictReason,
}

/// Something the merge resolved automatically that a user may want to know.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MergeNotice {
    /// Comments are append-only: a deletion on one branch does not remove the
    /// comment from the merge result.
    KeptCommentDeletedOnOneSide {
        comment_id: String,
        deleted_by: Side,
    },
    /// Two different revisions of a comment imported from GitHub; the one with
    /// the later `updated_at` is GitHub's newer state.
    NewerImportedCommentWins {
        comment_id: String,
        winner: Side,
    },
    TextualFallback(TextualFallback),
}

impl fmt::Display for MergeNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeNotice::KeptCommentDeletedOnOneSide {
                comment_id,
                deleted_by,
            } => write!(
                formatter,
                "kept comment {comment_id} deleted only by {deleted_by} (comments are append-only)"
            ),
            MergeNotice::NewerImportedCommentWins { comment_id, winner } => write!(
                formatter,
                "took {winner}'s newer GitHub revision of imported comment {comment_id}"
            ),
            MergeNotice::TextualFallback(reason) => {
                write!(formatter, "merged as plain text because {reason}")
            }
        }
    }
}

/// The result of merging one file.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct FileMerge {
    pub(crate) text: String,
    pub(crate) conflicts: Vec<MergeConflict>,
    pub(crate) notices: Vec<MergeNotice>,
}

impl FileMerge {
    fn clean(text: String) -> Self {
        Self {
            text,
            conflicts: Vec::new(),
            notices: Vec::new(),
        }
    }

    pub(crate) fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// The three versions of a file. An empty `base` means there is no common
/// ancestor (both sides added the file).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MergeInputs<'a> {
    pub(crate) base: &'a str,
    pub(crate) ours: &'a str,
    pub(crate) theirs: &'a str,
}

impl<'a> MergeInputs<'a> {
    fn region(self) -> ThreeWay<'a> {
        ThreeWay {
            base: self.base,
            ours: self.ours,
            theirs: self.theirs,
        }
    }

    /// Byte-level resolution when at most one side changed the file. Taking a
    /// side verbatim never re-serialises it, so formatting is preserved.
    fn trivial(self) -> Option<&'a str> {
        if self.ours == self.theirs || self.theirs == self.base {
            Some(self.ours)
        } else if self.ours == self.base {
            Some(self.theirs)
        } else {
            None
        }
    }

    fn has_base(self) -> bool {
        !self.base.trim().is_empty()
    }
}

fn textual_merge(inputs: MergeInputs<'_>, size: MarkerSize, reason: TextualFallback) -> FileMerge {
    let merged = merge_lines(inputs.region(), size);
    FileMerge {
        text: merged.text,
        conflicts: (0..merged.hunks.get())
            .map(|_| MergeConflict {
                subject: ConflictSubject::Lines,
                reason: ConflictReason::Textual(reason),
            })
            .collect(),
        notices: vec![MergeNotice::TextualFallback(reason)],
    }
}

/// Take the value changed by one side, or `None` when both changed it
/// differently (or there is no ancestor and they differ).
fn merge_scalar<'a, T: PartialEq + ?Sized>(
    base: Option<&T>,
    ours: &'a T,
    theirs: &'a T,
) -> Option<&'a T> {
    if ours == theirs || base == Some(theirs) {
        Some(ours)
    } else if base == Some(ours) {
        Some(theirs)
    } else {
        None
    }
}

fn prose_failure_kind(result: ProseMergeResult<'_>) -> Result<String, ProseMergeFailureKind> {
    match result {
        ProseMergeResult::Merged(merged) => Ok(merged.into_text().into_owned()),
        ProseMergeResult::CompetingEdit(failure) => {
            Err(ProseMergeFailureKind::CompetingEdit(failure.reason()))
        }
        ProseMergeResult::UnsupportedStructuredOverlap(failure) => Err(
            ProseMergeFailureKind::UnsupportedStructuredOverlap(failure.reason()),
        ),
        ProseMergeResult::WorkExhausted(failure) => {
            Err(ProseMergeFailureKind::WorkExhausted(failure.reason()))
        }
    }
}

/// Merge three label lists as sets. A base label survives only if both sides
/// kept it; a new label survives if either side added it. Surviving base
/// labels keep base order and additions follow in sorted order, so the result
/// does not depend on which side is "ours".
fn merge_labels(base: Option<&[String]>, ours: &[String], theirs: &[String]) -> Vec<String> {
    if let Some(unchanged) = merge_scalar(base, ours, theirs) {
        return unchanged.to_vec();
    }
    let base = base.unwrap_or_default();
    let mut merged: Vec<String> = Vec::with_capacity(ours.len().max(theirs.len()));
    for label in base {
        if ours.contains(label) && theirs.contains(label) && !merged.contains(label) {
            merged.push(label.clone());
        }
    }
    let added: BTreeSet<&String> = ours
        .iter()
        .chain(theirs)
        .filter(|label| !base.contains(label))
        .collect();
    merged.extend(added.into_iter().cloned());
    merged
}

fn parse_canonical_issue(issue_id: &str, text: &str, side: Side) -> Result<Issue, TextualFallback> {
    let issue =
        markdown_to_issue(issue_id, text).map_err(|_| TextualFallback::Unparseable(side))?;
    match issue_to_markdown(&issue) {
        Ok(rendered) if rendered == text => Ok(issue),
        _ => Err(TextualFallback::NonCanonical(side)),
    }
}

/// Parsed, canonical versions of an issue file.
pub(crate) struct IssueVersions {
    pub(crate) base: Option<Issue>,
    pub(crate) ours: Issue,
    pub(crate) theirs: Issue,
}

impl IssueVersions {
    pub(crate) fn parse(issue_id: &str, inputs: MergeInputs<'_>) -> Result<Self, TextualFallback> {
        let base = if inputs.has_base() {
            Some(parse_canonical_issue(issue_id, inputs.base, Side::Base)?)
        } else {
            None
        };
        Ok(Self {
            base,
            ours: parse_canonical_issue(issue_id, inputs.ours, Side::Ours)?,
            theirs: parse_canonical_issue(issue_id, inputs.theirs, Side::Theirs)?,
        })
    }
}

/// Merge three versions of an issue file.
pub(crate) fn merge_issue_files(
    issue_id: &str,
    inputs: MergeInputs<'_>,
    size: MarkerSize,
) -> FileMerge {
    if let Some(text) = inputs.trivial() {
        return FileMerge::clean(text.to_owned());
    }
    match IssueVersions::parse(issue_id, inputs) {
        Ok(versions) => merge_issue_versions(&versions, size)
            .unwrap_or_else(|reason| textual_merge(inputs, size, reason)),
        Err(reason) => textual_merge(inputs, size, reason),
    }
}

/// Field-level merge of parsed issues, without byte-level shortcuts.
pub(crate) fn merge_issue_versions(
    versions: &IssueVersions,
    size: MarkerSize,
) -> Result<FileMerge, TextualFallback> {
    let base = versions.base.as_ref();
    let (ours, theirs) = (&versions.ours, &versions.theirs);
    let unresolved = if base.is_some() {
        ConflictReason::BothChanged
    } else {
        ConflictReason::NoCommonAncestor
    };
    let mut merged = ours.clone();
    let mut conflicts: Vec<MergeConflict> = Vec::new();
    let mut conflict = |field: IssueField, reason: ConflictReason| {
        conflicts.push(MergeConflict {
            subject: ConflictSubject::Field(field),
            reason,
        });
    };

    macro_rules! merge_field {
        ($field:ident, $name:expr) => {
            match merge_scalar(
                base.map(|issue| &issue.$field),
                &ours.$field,
                &theirs.$field,
            ) {
                Some(value) => merged.$field.clone_from(value),
                None => conflict($name, unresolved),
            }
        };
    }
    merge_field!(title, IssueField::Title);
    merge_field!(priority, IssueField::Priority);
    merge_field!(issue_type, IssueField::IssueType);
    merge_field!(assignee, IssueField::Assignee);
    merge_field!(external_ref, IssueField::ExternalRef);
    merge_field!(claimed_at, IssueField::ClaimedAt);
    merge_field!(claimed_until, IssueField::ClaimedUntil);

    let status = merge_scalar(
        base.map(|issue| &issue.status),
        &ours.status,
        &theirs.status,
    );
    match status {
        Some(status) => merged.status = *status,
        None => conflict(IssueField::Status, unresolved),
    }
    // `closed_at` is derived bookkeeping, never a conflict of its own: when
    // both sides closed at different instants the later close wins, and a
    // one-sided value survives unless the merged status is known to be open.
    merged.closed_at = match merge_scalar(
        base.map(|issue| &issue.closed_at),
        &ours.closed_at,
        &theirs.closed_at,
    ) {
        Some(closed_at) => *closed_at,
        None => match (ours.closed_at, theirs.closed_at) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left, right) if status.is_none_or(|status| *status == Status::Closed) => {
                left.or(right)
            }
            _ => None,
        },
    };

    merged.created_at = *merge_scalar(
        base.map(|issue| &issue.created_at),
        &ours.created_at,
        &theirs.created_at,
    )
    .unwrap_or(&ours.created_at.min(theirs.created_at));
    merged.updated_at = *merge_scalar(
        base.map(|issue| &issue.updated_at),
        &ours.updated_at,
        &theirs.updated_at,
    )
    .unwrap_or(&ours.updated_at.max(theirs.updated_at));

    merged.labels = merge_labels(
        base.map(|issue| issue.labels.as_slice()),
        &ours.labels,
        &theirs.labels,
    );

    // A missing ancestor merges dependencies like an empty one: additions on
    // either side are kept and only differing types for one target conflict.
    let dependency_ids: BTreeSet<&String> = base
        .into_iter()
        .flat_map(|issue| issue.depends_on.keys())
        .chain(ours.depends_on.keys())
        .chain(theirs.depends_on.keys())
        .collect();
    let mut depends_on = HashMap::with_capacity(dependency_ids.len());
    for id in dependency_ids {
        let base_type = base.and_then(|issue| issue.depends_on.get(id));
        let ours_type = ours.depends_on.get(id);
        let theirs_type = theirs.depends_on.get(id);
        match merge_scalar(Some(&base_type), &ours_type, &theirs_type) {
            Some(Some(dep_type)) => {
                depends_on.insert(id.clone(), **dep_type);
            }
            Some(None) => {}
            None => {
                if let Some(dep_type) = ours_type {
                    depends_on.insert(id.clone(), *dep_type);
                }
                conflict(
                    IssueField::Dependency(id.clone()),
                    ConflictReason::BothChanged,
                );
            }
        }
    }
    merged.depends_on = depends_on;

    let limits = ProseMergeLimits::default();
    for section in Section::ALL {
        let (ours_text, theirs_text) = (section.content(ours), section.content(theirs));
        let result = match base.map(|issue| section.content(issue)) {
            Some(base_text) => {
                prose_failure_kind(merge_prose(base_text, ours_text, theirs_text, limits))
                    .map_err(ConflictReason::Prose)
            }
            None if ours_text == theirs_text => Ok(ours_text.to_owned()),
            None => Err(ConflictReason::NoCommonAncestor),
        };
        match result {
            Ok(text) => *section.content_mut(&mut merged) = trim_blank_edge_lines(&text).to_owned(),
            Err(reason) => conflict(IssueField::Section(section), reason),
        }
    }

    if conflicts.is_empty() {
        let text =
            issue_to_markdown(&merged).map_err(|_| TextualFallback::NonCanonical(Side::Ours))?;
        return Ok(FileMerge::clean(text));
    }
    let text = render_issue_conflicts(&merged, versions, &conflicts, size)?;
    Ok(FileMerge {
        text,
        conflicts,
        notices: Vec::new(),
    })
}

fn section_text(issue: &Issue, section: Section) -> String {
    let content = section.content(issue);
    if content.is_empty() {
        String::new()
    } else {
        let mut text = sanitize_section_content(content);
        text.push('\n');
        text
    }
}

/// Render a conflicted issue: every non-conflicting field takes its merged
/// value and each conflicting field becomes a diff3 hunk holding exactly the
/// lines in which the three versions differ.
fn render_issue_conflicts(
    merged: &Issue,
    versions: &IssueVersions,
    conflicts: &[MergeConflict],
    size: MarkerSize,
) -> Result<String, TextualFallback> {
    let Some(base) = versions.base.as_ref() else {
        // Two unrelated additions under one ID: show both whole files.
        let ours = issue_to_markdown(&versions.ours)
            .map_err(|_| TextualFallback::NonCanonical(Side::Ours))?;
        let theirs = issue_to_markdown(&versions.theirs)
            .map_err(|_| TextualFallback::NonCanonical(Side::Theirs))?;
        let mut text = String::new();
        push_conflict(
            &mut text,
            ThreeWay {
                base: "",
                ours: &ours,
                theirs: &theirs,
            },
            size,
        );
        return Ok(text);
    };

    let fields: BTreeSet<&IssueField> = conflicts
        .iter()
        .filter_map(|conflict| match &conflict.subject {
            ConflictSubject::Field(field) => Some(field),
            _ => None,
        })
        .collect();
    let mut base_view = merged.clone();
    let mut ours_view = merged.clone();
    let mut theirs_view = merged.clone();
    for field in &fields {
        field.copy_value(&mut base_view, base);
        field.copy_value(&mut ours_view, &versions.ours);
        field.copy_value(&mut theirs_view, &versions.theirs);
    }

    let rendered = |issue: &Issue, side| {
        frontmatter_yaml(issue).map_err(|_| TextualFallback::NonCanonical(side))
    };
    let base_frontmatter = rendered(&base_view, Side::Base)?;
    let ours_frontmatter = rendered(&ours_view, Side::Ours)?;
    let theirs_frontmatter = rendered(&theirs_view, Side::Theirs)?;
    let frontmatter_region = ThreeWay {
        base: &base_frontmatter,
        ours: &ours_frontmatter,
        theirs: &theirs_frontmatter,
    };
    let frontmatter = merge_lines(frontmatter_region, size);

    let mut text = String::with_capacity(frontmatter.text.len() * 2);
    text.push_str("---\n");
    if frontmatter.hunks.is_clean() && fields.iter().any(|field| field.is_frontmatter()) {
        // Line alignment merged what the field merge rejected; never let that
        // pass as clean.
        push_conflict(&mut text, frontmatter_region, size);
    } else {
        text.push_str(&frontmatter.text);
    }
    text.push_str("---\n");

    for section in Section::ALL {
        if fields.contains(&IssueField::Section(section)) {
            let base_text = section_text(&base_view, section);
            let ours_text = section_text(&ours_view, section);
            let theirs_text = section_text(&theirs_view, section);
            let region = ThreeWay {
                base: &base_text,
                ours: &ours_text,
                theirs: &theirs_text,
            };
            push_section_heading(&mut text, section);
            let lines = merge_lines(region, size);
            if lines.hunks.is_clean() {
                push_conflict(&mut text, region, size);
            } else {
                text.push_str(&lines.text);
            }
        } else if !section.content(merged).is_empty() {
            push_section_heading(&mut text, section);
            text.push_str(&section_text(merged, section));
        }
    }
    Ok(text)
}

/// Serialize comments exactly as `Storage` writes them.
fn comments_json(comments: &[&Comment]) -> String {
    serde_json::to_string_pretty(comments).expect("comments always serialize to JSON")
}

fn parse_canonical_comments(text: &str, side: Side) -> Result<Vec<Comment>, TextualFallback> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut comments: Vec<Comment> =
        serde_json::from_str(text).map_err(|_| TextualFallback::Unparseable(side))?;
    comments.sort_by_key(|comment| comment.created_at);
    let ids: BTreeSet<&str> = comments.iter().map(|comment| comment.id.as_str()).collect();
    if ids.len() != comments.len() {
        return Err(TextualFallback::DuplicateCommentId(side));
    }
    if comments_json(&comments.iter().collect::<Vec<_>>()) != text {
        return Err(TextualFallback::NonCanonical(side));
    }
    Ok(comments)
}

/// Parsed versions of a comment file.
pub(crate) struct CommentVersions {
    pub(crate) base: Vec<Comment>,
    pub(crate) ours: Vec<Comment>,
    pub(crate) theirs: Vec<Comment>,
}

impl CommentVersions {
    pub(crate) fn parse(inputs: MergeInputs<'_>) -> Result<Self, TextualFallback> {
        Ok(Self {
            base: parse_canonical_comments(inputs.base, Side::Base)?,
            ours: parse_canonical_comments(inputs.ours, Side::Ours)?,
            theirs: parse_canonical_comments(inputs.theirs, Side::Theirs)?,
        })
    }
}

/// Merge three versions of a comment file.
///
/// Unlike issue files there is no "one side unchanged" byte shortcut: the
/// comment set is append-only, so a comment deleted on one side must survive
/// even when the other side did not touch the file.
pub(crate) fn merge_comment_files(inputs: MergeInputs<'_>, size: MarkerSize) -> FileMerge {
    if inputs.ours == inputs.theirs {
        return FileMerge::clean(inputs.ours.to_owned());
    }
    match CommentVersions::parse(inputs) {
        Ok(versions) => merge_comment_versions(&versions, size),
        Err(reason) => textual_merge(inputs, size, reason),
    }
}

enum CommentResolution<'a> {
    Take(&'a Comment),
    Merged(Comment),
    Conflict {
        base: Option<&'a Comment>,
        ours: &'a Comment,
        theirs: &'a Comment,
    },
}

/// Resolve a comment ID that both sides still have.
fn resolve_shared_comment<'a>(
    base: Option<&'a Comment>,
    ours: &'a Comment,
    theirs: &'a Comment,
    notices: &mut Vec<MergeNotice>,
) -> CommentResolution<'a> {
    if let Some(comment) = merge_scalar(base, ours, theirs) {
        return CommentResolution::Take(comment);
    }
    if ours.source_id.is_some() && theirs.source_id.is_some() && ours.source_id == theirs.source_id
    {
        // GitHub owns imported comments; each side holds a snapshot of it and
        // the later edit is GitHub's current state.
        let winner = match ours.updated_at.cmp(&theirs.updated_at) {
            std::cmp::Ordering::Greater => Some((Side::Ours, ours)),
            std::cmp::Ordering::Less => Some((Side::Theirs, theirs)),
            std::cmp::Ordering::Equal => None,
        };
        if let Some((side, comment)) = winner {
            notices.push(MergeNotice::NewerImportedCommentWins {
                comment_id: comment.id.clone(),
                winner: side,
            });
            return CommentResolution::Take(comment);
        }
    }
    let same_identity = |left: &Comment, right: &Comment| {
        left.issue_id == right.issue_id
            && left.author == right.author
            && left.created_at == right.created_at
            && left.source_url == right.source_url
            && left.source_id == right.source_id
    };
    if let Some(base) = base.filter(|base| same_identity(base, ours) && same_identity(base, theirs))
    {
        if let Ok(body) = prose_failure_kind(merge_prose(
            &base.body,
            &ours.body,
            &theirs.body,
            ProseMergeLimits::default(),
        )) {
            let mut merged = ours.clone();
            merged.body = body;
            merged.updated_at = ours.updated_at.max(theirs.updated_at);
            return CommentResolution::Merged(merged);
        }
    }
    CommentResolution::Conflict { base, ours, theirs }
}

fn sorted_comments<'a>(comments: impl Iterator<Item = &'a Comment>) -> Vec<&'a Comment> {
    let mut sorted: Vec<&Comment> = comments.collect();
    sorted.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

fn comments_by_id(comments: &[Comment]) -> BTreeMap<&str, &Comment> {
    comments
        .iter()
        .map(|comment| (comment.id.as_str(), comment))
        .collect()
}

/// ID-keyed, append-only merge of comment lists.
pub(crate) fn merge_comment_versions(versions: &CommentVersions, size: MarkerSize) -> FileMerge {
    let base = comments_by_id(&versions.base);
    let ours = comments_by_id(&versions.ours);
    let theirs = comments_by_id(&versions.theirs);
    let ids: BTreeSet<&str> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .copied()
        .collect();

    let mut notices = Vec::new();
    let mut conflicts = Vec::new();
    let mut merged_owned: Vec<Comment> = Vec::new();
    // Per ID: the comment to emit in the (base, ours, theirs) views.
    let mut views: Vec<[Option<&Comment>; 3]> = Vec::with_capacity(ids.len());
    let mut owned_slots: Vec<usize> = Vec::new();
    for id in ids {
        let base_comment = base.get(id).copied();
        match (ours.get(id).copied(), theirs.get(id).copied()) {
            (Some(ours_comment), Some(theirs_comment)) => {
                match resolve_shared_comment(
                    base_comment,
                    ours_comment,
                    theirs_comment,
                    &mut notices,
                ) {
                    CommentResolution::Take(comment) => views.push([Some(comment); 3]),
                    CommentResolution::Merged(comment) => {
                        owned_slots.push(views.len());
                        merged_owned.push(comment);
                        views.push([None; 3]);
                    }
                    CommentResolution::Conflict { base, ours, theirs } => {
                        conflicts.push(MergeConflict {
                            subject: ConflictSubject::Comment(id.to_owned()),
                            reason: if base.is_some() {
                                ConflictReason::BothChanged
                            } else {
                                ConflictReason::NoCommonAncestor
                            },
                        });
                        views.push([base, Some(ours), Some(theirs)]);
                    }
                }
            }
            (Some(kept), None) | (None, Some(kept)) => {
                if base_comment.is_some() {
                    notices.push(MergeNotice::KeptCommentDeletedOnOneSide {
                        comment_id: id.to_owned(),
                        deleted_by: if ours.contains_key(id) {
                            Side::Theirs
                        } else {
                            Side::Ours
                        },
                    });
                }
                views.push([Some(kept); 3]);
            }
            // Deleted by both sides: both replicas agree it is gone.
            (None, None) => {}
        }
    }
    for (slot, comment) in owned_slots.into_iter().zip(&merged_owned) {
        views[slot] = [Some(comment); 3];
    }

    let view = |index: usize| sorted_comments(views.iter().filter_map(move |slots| slots[index]));
    let ours_json = comments_json(&view(1));
    if conflicts.is_empty() {
        return FileMerge {
            text: ours_json,
            conflicts,
            notices,
        };
    }
    let base_json = comments_json(&view(0));
    let theirs_json = comments_json(&view(2));
    let region = ThreeWay {
        base: &base_json,
        ours: &ours_json,
        theirs: &theirs_json,
    };
    let lines = merge_lines(region, size);
    let text = if lines.hunks.is_clean() {
        let mut text = String::new();
        push_conflict(&mut text, region, size);
        text
    } else {
        lines.text
    };
    FileMerge {
        text,
        conflicts,
        notices,
    }
}

#[cfg(test)]
#[path = "issue_merge_tests.rs"]
mod tests;
