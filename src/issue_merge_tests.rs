use super::*;
use crate::diff3::MarkerSize;
use crate::types::{DependencyType, IssueType};
use chrono::{DateTime, Duration, TimeZone, Utc};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

type Seed = u64;
type Minute = i64;

const ISSUE_ID: &str = "proj-7";
const PROPERTY_CASES: Seed = 2500;

fn at(minute: Minute) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap() + Duration::minutes(minute)
}

fn base_issue() -> Issue {
    let mut issue = Issue::new(ISSUE_ID.into(), "Shared title".into(), 2, IssueType::Task);
    issue.created_at = at(0);
    issue.updated_at = at(1);
    issue.description = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.".into();
    issue.labels = vec!["alpha".into(), "beta".into()];
    issue
        .depends_on
        .insert("proj-1".into(), DependencyType::Blocks);
    issue
}

fn render(issue: &Issue) -> String {
    issue_to_markdown(issue).unwrap()
}

fn parse(text: &str) -> Issue {
    markdown_to_issue(ISSUE_ID, text).unwrap_or_else(|error| panic!("{error:#}\n{text}"))
}

fn merge_texts(base: &str, ours: &str, theirs: &str) -> FileMerge {
    merge_issue_files(
        ISSUE_ID,
        MergeInputs { base, ours, theirs },
        MarkerSize::DEFAULT,
    )
}

fn merge(base: &Issue, ours: &Issue, theirs: &Issue) -> FileMerge {
    merge_texts(&render(base), &render(ours), &render(theirs))
}

/// The structured engine alone, bypassing the byte-level shortcuts.
fn structured(base: &Issue, ours: &Issue, theirs: &Issue) -> FileMerge {
    let (base, ours, theirs) = (render(base), render(ours), render(theirs));
    let versions = IssueVersions::parse(
        ISSUE_ID,
        MergeInputs {
            base: &base,
            ours: &ours,
            theirs: &theirs,
        },
    )
    .unwrap();
    merge_issue_versions(&versions, MarkerSize::DEFAULT).unwrap()
}

/// Resolve every conflict hunk by taking one side, as a tool or agent would.
fn resolve(text: &str, side: Side) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Region {
        Outside,
        In(Side),
    }
    let mut region = Region::Outside;
    let mut resolved = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match (region, line.trim_end_matches('\n')) {
            (Region::Outside, "<<<<<<< ours") => region = Region::In(Side::Ours),
            (Region::In(Side::Ours), "||||||| base") => region = Region::In(Side::Base),
            (Region::In(Side::Base), "=======") => region = Region::In(Side::Theirs),
            (Region::In(Side::Theirs), ">>>>>>> theirs") => region = Region::Outside,
            (Region::Outside, _) => resolved.push_str(line),
            (Region::In(current), _) if current == side => resolved.push_str(line),
            (Region::In(_), _) => {}
        }
    }
    assert!(region == Region::Outside, "unterminated hunk in\n{text}");
    resolved
}

fn hunk_count(text: &str) -> usize {
    text.lines().filter(|line| *line == "<<<<<<< ours").count()
}

fn conflict_fields(merge: &FileMerge) -> Vec<IssueField> {
    let mut fields: Vec<IssueField> = merge
        .conflicts
        .iter()
        .map(|conflict| match &conflict.subject {
            ConflictSubject::Field(field) => field.clone(),
            other => panic!("unexpected conflict subject {other:?}"),
        })
        .collect();
    fields.sort();
    fields
}

fn field_value(issue: &Issue, field: &IssueField) -> String {
    match field {
        IssueField::Title => issue.title.clone(),
        IssueField::Status => issue.status.to_string(),
        IssueField::Priority => issue.priority.to_string(),
        IssueField::IssueType => issue.issue_type.to_string(),
        IssueField::Assignee => issue.assignee.clone(),
        IssueField::ExternalRef => format!("{:?}", issue.external_ref),
        IssueField::ClaimedAt => format!("{:?}", issue.claimed_at),
        IssueField::ClaimedUntil => format!("{:?}", issue.claimed_until),
        IssueField::Dependency(id) => format!("{:?}", issue.depends_on.get(id)),
        IssueField::Section(section) => section.content(issue).to_owned(),
    }
}

const FRONTMATTER_FIELDS: [IssueField; 8] = [
    IssueField::Title,
    IssueField::Status,
    IssueField::Priority,
    IssueField::IssueType,
    IssueField::Assignee,
    IssueField::ExternalRef,
    IssueField::ClaimedAt,
    IssueField::ClaimedUntil,
];

// ---------------------------------------------------------------------------
// Explicit cases
// ---------------------------------------------------------------------------

#[test]
fn disjoint_field_edits_merge_cleanly() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.title = "Ours title".into();
    ours.updated_at = at(10);
    let mut theirs = base.clone();
    theirs.priority = 0;
    theirs.assignee = "bob".into();
    theirs.updated_at = at(20);

    let result = merge(&base, &ours, &theirs);
    assert!(result.is_clean(), "{result:?}");
    let merged = parse(&result.text);
    assert_eq!(merged.title, "Ours title");
    assert_eq!(merged.priority, 0);
    assert_eq!(merged.assignee, "bob");
    assert_eq!(merged.updated_at, at(20));
    assert_eq!(result.text, render(&merged), "clean output is canonical");
}

#[test]
fn competing_title_edits_produce_one_hunk_around_only_the_title() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.title = "Ours title".into();
    let mut theirs = base.clone();
    theirs.title = "Theirs title".into();
    theirs.priority = 4;

    let result = merge(&base, &ours, &theirs);
    assert_eq!(conflict_fields(&result), vec![IssueField::Title]);
    assert_eq!(result.conflicts[0].reason, ConflictReason::BothChanged);
    assert_eq!(hunk_count(&result.text), 1);
    assert!(result.text.contains(
        "<<<<<<< ours\ntitle: Ours title\n||||||| base\ntitle: Shared title\n=======\ntitle: Theirs title\n>>>>>>> theirs\n"
    ), "{}", result.text);
    assert!(
        result.text.contains("\npriority: 4\n"),
        "theirs' clean edit applied"
    );
    assert_eq!(
        parse(&resolve(&result.text, Side::Theirs)).title,
        "Theirs title"
    );
    assert_eq!(parse(&resolve(&result.text, Side::Ours)).priority, 4);
}

#[test]
fn independent_paragraph_edits_in_one_section_merge() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.description =
        "First paragraph, edited by ours.\n\nSecond paragraph.\n\nThird paragraph.".into();
    let mut theirs = base.clone();
    theirs.description =
        "First paragraph.\n\nSecond paragraph.\n\nThird paragraph, edited by theirs.".into();

    let result = merge(&base, &ours, &theirs);
    assert!(result.is_clean(), "{result:?}");
    assert_eq!(
        parse(&result.text).description,
        "First paragraph, edited by ours.\n\nSecond paragraph.\n\nThird paragraph, edited by theirs."
    );
}

#[test]
fn competing_paragraph_edits_conflict_only_on_that_paragraph() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.description =
        "First paragraph.\n\nSecond paragraph rewritten by ours.\n\nThird paragraph.".into();
    let mut theirs = base.clone();
    theirs.description =
        "First paragraph.\n\nThe second paragraph, as theirs prefers.\n\nThird paragraph.".into();

    let result = merge(&base, &ours, &theirs);
    assert_eq!(
        conflict_fields(&result),
        vec![IssueField::Section(Section::Description)]
    );
    assert!(matches!(
        result.conflicts[0].reason,
        ConflictReason::Prose(_)
    ));
    assert_eq!(hunk_count(&result.text), 1);
    assert!(result.text.contains(
        "First paragraph.\n\n<<<<<<< ours\nSecond paragraph rewritten by ours.\n||||||| base\nSecond paragraph.\n=======\nThe second paragraph, as theirs prefers.\n>>>>>>> theirs\n\nThird paragraph.\n"
    ), "{}", result.text);
}

#[test]
fn labels_merge_as_a_set() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.labels = vec!["alpha".into(), "beta".into(), "zeta".into()];
    let mut theirs = base.clone();
    theirs.labels = vec!["gamma".into(), "beta".into()];

    let result = merge(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(parse(&result.text).labels, vec!["beta", "gamma", "zeta"]);
}

#[test]
fn dependencies_merge_per_target_and_conflict_on_change_versus_removal() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.depends_on
        .insert("proj-1".into(), DependencyType::Related);
    ours.depends_on
        .insert("proj-2".into(), DependencyType::Blocks);
    let mut theirs = base.clone();
    theirs.depends_on.remove("proj-1");
    theirs
        .depends_on
        .insert("proj-3".into(), DependencyType::ParentChild);

    let result = merge(&base, &ours, &theirs);
    assert_eq!(
        conflict_fields(&result),
        vec![IssueField::Dependency("proj-1".into())]
    );
    let resolved = parse(&resolve(&result.text, Side::Theirs));
    assert!(!resolved.depends_on.contains_key("proj-1"));
    assert_eq!(resolved.depends_on["proj-2"], DependencyType::Blocks);
    assert_eq!(resolved.depends_on["proj-3"], DependencyType::ParentChild);
}

#[test]
fn dependency_lines_serialize_in_sorted_order() {
    let mut issue = base_issue();
    for id in ["proj-9", "proj-3", "proj-5", "proj-2"] {
        issue.depends_on.insert(id.into(), DependencyType::Blocks);
    }
    let text = render(&issue);
    let order: Vec<usize> = ["proj-1", "proj-2", "proj-3", "proj-5", "proj-9"]
        .iter()
        .map(|id| text.find(&format!("{id}: blocks")).unwrap())
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
}

#[test]
fn both_sides_closing_keeps_the_later_close_time() {
    let base = base_issue();
    let mut ours = base.clone();
    ours.status = Status::Closed;
    ours.closed_at = Some(at(30));
    ours.updated_at = at(30);
    let mut theirs = base.clone();
    theirs.status = Status::Closed;
    theirs.closed_at = Some(at(40));
    theirs.updated_at = at(40);

    let merged = parse(&merge(&base, &ours, &theirs).text);
    assert_eq!(merged.status, Status::Closed);
    assert_eq!(merged.closed_at, Some(at(40)));
    assert_eq!(merged.updated_at, at(40));
}

#[test]
fn identical_additions_without_an_ancestor_merge() {
    let issue = base_issue();
    let result = merge_texts("", &render(&issue), &render(&issue));
    assert!(result.is_clean());
    assert_eq!(result.text, render(&issue));
}

#[test]
fn different_additions_without_an_ancestor_conflict_as_whole_files() {
    let ours = base_issue();
    let mut theirs = base_issue();
    theirs.title = "A different issue that reused the ID".into();
    theirs.updated_at = at(9);

    let result = merge_texts("", &render(&ours), &render(&theirs));
    assert_eq!(conflict_fields(&result), vec![IssueField::Title]);
    assert_eq!(result.conflicts[0].reason, ConflictReason::NoCommonAncestor);
    assert_eq!(resolve(&result.text, Side::Ours), render(&ours));
    assert_eq!(resolve(&result.text, Side::Theirs), render(&theirs));
}

#[test]
fn non_canonical_files_fall_back_to_a_textual_merge() {
    let base = render(&base_issue());
    let ours = base.replace(
        "title: Shared title",
        "title:   Shared title  # hand edited",
    );
    let theirs = base.replace("priority: 2", "priority: 3");
    let result = merge_texts(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(
        result.notices,
        vec![MergeNotice::TextualFallback(TextualFallback::NonCanonical(
            Side::Ours
        ))]
    );
    assert!(result.text.contains("# hand edited") && result.text.contains("priority: 3"));
}

#[test]
fn one_sided_changes_are_taken_byte_for_byte() {
    let base = render(&base_issue());
    let theirs = base.replace(
        "Third paragraph.",
        "Third paragraph.\n\n\n\nOdd spacing kept.",
    );
    assert_eq!(merge_texts(&base, &base, &theirs).text, theirs);
    assert_eq!(merge_texts(&base, &theirs, &base).text, theirs);
}

// ---------------------------------------------------------------------------
// Comment files
// ---------------------------------------------------------------------------

fn comment(id: &str, body: &str, minute: Minute) -> Comment {
    Comment {
        id: id.into(),
        issue_id: ISSUE_ID.into(),
        author: "alice".into(),
        body: body.into(),
        created_at: at(minute),
        updated_at: at(minute),
        source_url: None,
        source_id: None,
    }
}

fn imported(remote_id: u32, body: &str, minute: Minute, edited: Minute) -> Comment {
    Comment {
        id: format!("gh-{remote_id}"),
        issue_id: ISSUE_ID.into(),
        author: "octocat".into(),
        body: body.into(),
        created_at: at(minute),
        updated_at: at(edited),
        source_url: Some(format!(
            "https://github.com/o/r/issues/1#issuecomment-{remote_id}"
        )),
        source_id: Some(remote_id.to_string()),
    }
}

fn render_comments(comments: &[Comment]) -> String {
    comments_json(&sorted_comments(comments.iter()))
}

fn parse_comments(text: &str) -> Vec<Comment> {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn merge_comment_lists(base: &[Comment], ours: &[Comment], theirs: &[Comment]) -> FileMerge {
    let (base, ours, theirs) = (
        render_comments(base),
        render_comments(ours),
        render_comments(theirs),
    );
    merge_comment_files(
        MergeInputs {
            base: &base,
            ours: &ours,
            theirs: &theirs,
        },
        MarkerSize::DEFAULT,
    )
}

fn structured_comments(base: &[Comment], ours: &[Comment], theirs: &[Comment]) -> FileMerge {
    let (base, ours, theirs) = (
        render_comments(base),
        render_comments(ours),
        render_comments(theirs),
    );
    let versions = CommentVersions::parse(MergeInputs {
        base: &base,
        ours: &ours,
        theirs: &theirs,
    })
    .unwrap();
    merge_comment_versions(&versions, MarkerSize::DEFAULT)
}

fn ids(comments: &[Comment]) -> Vec<&str> {
    comments.iter().map(|comment| comment.id.as_str()).collect()
}

#[test]
fn comments_appended_on_both_sides_are_all_kept_in_time_order() {
    let base = vec![comment("c-base", "base", 1)];
    let mut ours = base.clone();
    ours.push(comment("c-ours", "from ours", 5));
    let mut theirs = base.clone();
    theirs.push(comment("c-theirs", "from theirs", 3));

    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(
        ids(&parse_comments(&result.text)),
        vec!["c-base", "c-theirs", "c-ours"]
    );
}

#[test]
fn the_same_imported_comment_on_both_sides_is_kept_once() {
    let base = Vec::new();
    let ours = vec![comment("c-ours", "mine", 2), imported(100, "hello", 1, 1)];
    let theirs = vec![imported(100, "hello", 1, 1)];
    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(ids(&parse_comments(&result.text)), vec!["gh-100", "c-ours"]);
}

#[test]
fn a_comment_deleted_on_one_side_is_kept_and_reported() {
    let base = vec![comment("c-1", "one", 1), comment("c-2", "two", 2)];
    let ours = vec![comment("c-1", "one", 1)];
    let mut theirs = base.clone();
    theirs.push(comment("c-3", "three", 3));

    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(
        ids(&parse_comments(&result.text)),
        vec!["c-1", "c-2", "c-3"]
    );
    assert_eq!(
        result.notices,
        vec![MergeNotice::KeptCommentDeletedOnOneSide {
            comment_id: "c-2".into(),
            deleted_by: Side::Ours,
        }]
    );
}

#[test]
fn a_comment_deleted_on_both_sides_stays_deleted() {
    let base = vec![comment("c-1", "one", 1), comment("c-2", "two", 2)];
    let mut ours = vec![comment("c-1", "one", 1)];
    ours.push(comment("c-3", "three", 3));
    let theirs = vec![comment("c-1", "one", 1)];
    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(ids(&parse_comments(&result.text)), vec!["c-1", "c-3"]);
}

#[test]
fn the_newer_revision_of_an_imported_comment_wins() {
    let base = vec![imported(7, "draft", 1, 1)];
    let ours = vec![imported(7, "edited later on GitHub", 1, 9)];
    let theirs = vec![imported(7, "edited on GitHub", 1, 5)];
    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean());
    assert_eq!(
        parse_comments(&result.text)[0].body,
        "edited later on GitHub"
    );
    let swapped = merge_comment_lists(&base, &theirs, &ours);
    assert_eq!(swapped.text, result.text);
}

#[test]
fn independent_edits_to_one_local_comment_merge_as_prose() {
    let base = vec![comment("c-1", "Para one.\n\nPara two.", 1)];
    let mut ours = base.clone();
    ours[0].body = "Para one, amended.\n\nPara two.".into();
    let mut theirs = base.clone();
    theirs[0].body = "Para one.\n\nPara two, amended.".into();
    let result = merge_comment_lists(&base, &ours, &theirs);
    assert!(result.is_clean(), "{result:?}");
    assert_eq!(
        parse_comments(&result.text)[0].body,
        "Para one, amended.\n\nPara two, amended."
    );
}

#[test]
fn competing_edits_to_one_comment_conflict_on_its_body_line_only() {
    let base = vec![comment("c-1", "original", 1), comment("c-2", "other", 2)];
    let mut ours = base.clone();
    ours[0].body = "ours wording".into();
    ours.push(comment("c-3", "new from ours", 3));
    let mut theirs = base.clone();
    theirs[0].body = "theirs wording".into();

    let result = merge_comment_lists(&base, &ours, &theirs);
    assert_eq!(
        result.conflicts,
        vec![MergeConflict {
            subject: ConflictSubject::Comment("c-1".into()),
            reason: ConflictReason::BothChanged,
        }]
    );
    assert_eq!(hunk_count(&result.text), 1);
    assert!(result.text.contains(
        "<<<<<<< ours\n    \"body\": \"ours wording\",\n||||||| base\n    \"body\": \"original\",\n=======\n    \"body\": \"theirs wording\",\n>>>>>>> theirs\n"
    ), "{}", result.text);
    for side in [Side::Ours, Side::Theirs] {
        let resolved = parse_comments(&resolve(&result.text, side));
        assert_eq!(ids(&resolved), vec!["c-1", "c-2", "c-3"]);
    }
}

#[test]
fn unparseable_comment_files_fall_back_to_text() {
    let base = render_comments(&[comment("c-1", "one", 1)]);
    let ours = format!("{base}\n// trailing junk\n");
    let theirs = render_comments(&[comment("c-1", "one", 1), comment("c-2", "two", 2)]);
    let result = merge_comment_files(
        MergeInputs {
            base: &base,
            ours: &ours,
            theirs: &theirs,
        },
        MarkerSize::DEFAULT,
    );
    assert_eq!(
        result.notices,
        vec![MergeNotice::TextualFallback(TextualFallback::Unparseable(
            Side::Ours
        ))]
    );
}

// ---------------------------------------------------------------------------
// Properties over generated edits
// ---------------------------------------------------------------------------

const TITLES: [&str; 4] = [
    "Shared title",
    "Fix the parser",
    "Ship the merge driver",
    "Rename storage",
];
const LABELS: [&str; 5] = ["alpha", "beta", "gamma", "delta", "human"];
const DEPENDENCY_IDS: [&str; 3] = ["proj-1", "proj-2", "proj-3"];
const DEPENDENCY_TYPES: [DependencyType; 3] = [
    DependencyType::Blocks,
    DependencyType::Related,
    DependencyType::ParentChild,
];
const STATUSES: [Status; 4] = [
    Status::Open,
    Status::InProgress,
    Status::Blocked,
    Status::Closed,
];
const ASSIGNEES: [&str; 3] = ["", "alice", "bob"];
const SENTENCES: [&str; 6] = [
    "Alpha bravo charlie.",
    "Delta echo foxtrot.",
    "Golf hotel india.",
    "Juliet kilo lima.",
    "Mike november oscar.",
    "Papa quebec romeo.",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditKind {
    Title,
    Status,
    Priority,
    Assignee,
    Labels,
    Dependencies,
    Description,
    Notes,
}

const EDIT_KINDS: [EditKind; 8] = [
    EditKind::Title,
    EditKind::Status,
    EditKind::Priority,
    EditKind::Assignee,
    EditKind::Labels,
    EditKind::Dependencies,
    EditKind::Description,
    EditKind::Notes,
];

fn pick<'a, T>(rng: &mut StdRng, items: &'a [T]) -> &'a T {
    items.choose(rng).unwrap()
}

fn set_status(issue: &mut Issue, status: Status, minute: Minute) {
    issue.status = status;
    issue.closed_at = (status == Status::Closed).then(|| at(minute));
}

fn random_base(rng: &mut StdRng) -> Issue {
    let mut issue = Issue::new(
        ISSUE_ID.into(),
        (*pick(rng, &TITLES)).into(),
        rng.gen_range(0..5),
        IssueType::Task,
    );
    issue.created_at = at(0);
    issue.updated_at = at(1);
    set_status(&mut issue, *pick(rng, &STATUSES), 1);
    issue.assignee = (*pick(rng, &ASSIGNEES)).into();
    issue.labels = LABELS
        .iter()
        .filter(|_| rng.gen_bool(0.4))
        .map(|label| (*label).to_owned())
        .collect();
    for id in DEPENDENCY_IDS {
        if rng.gen_bool(0.4) {
            issue
                .depends_on
                .insert(id.into(), *pick(rng, &DEPENDENCY_TYPES));
        }
    }
    let paragraphs = rng.gen_range(1..5);
    issue.description = (0..paragraphs)
        .map(|index| format!("P{index} {}", pick(rng, &SENTENCES)))
        .collect::<Vec<_>>()
        .join("\n\n");
    if rng.gen_bool(0.3) {
        issue.notes = (*pick(rng, &SENTENCES)).into();
    }
    issue
}

/// Apply one random edit of `kind`. `tag` makes some edits side-specific while
/// others are drawn from small vocabularies so both sides sometimes agree.
fn apply_edit(rng: &mut StdRng, issue: &mut Issue, kind: EditKind, tag: &str, minute: Minute) {
    match kind {
        EditKind::Title => {
            issue.title = if rng.gen_bool(0.5) {
                (*pick(rng, &TITLES)).into()
            } else {
                format!("Title by {tag}")
            };
        }
        EditKind::Status => set_status(issue, *pick(rng, &STATUSES), minute),
        EditKind::Priority => issue.priority = rng.gen_range(0..5),
        EditKind::Assignee => issue.assignee = (*pick(rng, &ASSIGNEES)).into(),
        EditKind::Labels => {
            let label = (*pick(rng, &LABELS)).to_owned();
            match issue.labels.iter().position(|existing| *existing == label) {
                Some(index) => {
                    issue.labels.remove(index);
                }
                None => issue.labels.push(label),
            }
        }
        EditKind::Dependencies => {
            let id = (*pick(rng, &DEPENDENCY_IDS)).to_owned();
            if issue.depends_on.contains_key(&id) && rng.gen_bool(0.5) {
                issue.depends_on.remove(&id);
            } else {
                issue.depends_on.insert(id, *pick(rng, &DEPENDENCY_TYPES));
            }
        }
        EditKind::Description => {
            let mut paragraphs: Vec<String> =
                issue.description.split("\n\n").map(str::to_owned).collect();
            let index = rng.gen_range(0..paragraphs.len());
            match rng.gen_range(0..3) {
                0 => paragraphs[index] = format!("{} ({tag})", pick(rng, &SENTENCES)),
                1 => paragraphs.insert(
                    index + 1,
                    format!("Added by {tag}: {}", pick(rng, &SENTENCES)),
                ),
                _ if paragraphs.len() > 1 => {
                    paragraphs.remove(index);
                }
                _ => paragraphs[index].push_str(" More."),
            }
            issue.description = paragraphs.join("\n\n");
        }
        EditKind::Notes => {
            issue.notes = if rng.gen_bool(0.3) {
                String::new()
            } else {
                (*pick(rng, &SENTENCES)).into()
            };
        }
    }
    issue.updated_at = at(minute);
}

fn random_descendant(
    rng: &mut StdRng,
    base: &Issue,
    kinds: &[EditKind],
    tag: &str,
    minute: Minute,
) -> Issue {
    let mut issue = base.clone();
    for kind in kinds {
        apply_edit(rng, &mut issue, *kind, tag, minute);
    }
    issue
}

fn random_kinds(rng: &mut StdRng) -> Vec<EditKind> {
    EDIT_KINDS
        .iter()
        .copied()
        .filter(|_| rng.gen_bool(0.3))
        .collect()
}

/// Generated (base, ours, theirs) triples with overlapping random edits.
fn generated_triples() -> impl Iterator<Item = (Seed, Issue, Issue, Issue)> {
    (0..PROPERTY_CASES).map(|seed| {
        let mut rng = StdRng::seed_from_u64(seed);
        let base = random_base(&mut rng);
        let ours_kinds = random_kinds(&mut rng);
        let theirs_kinds = random_kinds(&mut rng);
        let ours_minute = rng.gen_range(2..50);
        let theirs_minute = rng.gen_range(2..50);
        let ours = random_descendant(&mut rng, &base, &ours_kinds, "ours", ours_minute);
        let theirs = random_descendant(&mut rng, &base, &theirs_kinds, "theirs", theirs_minute);
        (seed, base, ours, theirs)
    })
}

#[test]
fn property_issue_merge_is_commutative() {
    let mut conflicted = 0;
    for (seed, base, ours, theirs) in generated_triples() {
        let forward = merge(&base, &ours, &theirs);
        let backward = merge(&base, &theirs, &ours);
        assert_eq!(forward.is_clean(), backward.is_clean(), "seed {seed}");
        assert_eq!(
            conflict_fields(&forward),
            conflict_fields(&backward),
            "seed {seed}"
        );
        if forward.is_clean() {
            assert_eq!(forward.text, backward.text, "seed {seed}");
        } else {
            conflicted += 1;
            // Swapping the roles swaps hunk contents and nothing else.
            assert_eq!(
                resolve(&forward.text, Side::Ours),
                resolve(&backward.text, Side::Theirs),
                "seed {seed}"
            );
            assert_eq!(
                resolve(&forward.text, Side::Base),
                resolve(&backward.text, Side::Base),
                "seed {seed}"
            );
        }
    }
    // The generator must exercise both outcomes to mean anything.
    assert!(conflicted > PROPERTY_CASES / 20, "{conflicted} conflicts");
    assert!(
        conflicted < PROPERTY_CASES * 19 / 20,
        "{conflicted} conflicts"
    );
}

#[test]
fn property_structured_merge_is_identity_when_one_side_is_unchanged() {
    for (seed, base, ours, theirs) in generated_triples() {
        assert_eq!(
            structured(&base, &ours, &base).text,
            render(&ours),
            "seed {seed}"
        );
        assert_eq!(
            structured(&base, &base, &theirs).text,
            render(&theirs),
            "seed {seed}"
        );
        assert_eq!(
            structured(&base, &ours, &ours).text,
            render(&ours),
            "seed {seed}"
        );
    }
}

/// Merging a clean result back against either input changes nothing.
///
/// Field merges satisfy this exactly. Prose, like any diff3, does not always:
/// with base `P0 P1`, a result `X` (P0 rewritten, P1 deleted) and a side `P0`
/// (P1 deleted), the two edits overlap at P1 and the re-merge conflicts. Such
/// re-merges must conflict only in prose sections, never produce a different
/// clean text, and stay rare.
#[test]
fn property_remerging_a_merged_side_is_idempotent() {
    let (mut checked, mut prose_overlaps) = (0, 0);
    for (seed, base, ours, theirs) in generated_triples() {
        let result = merge(&base, &ours, &theirs);
        if !result.is_clean() {
            continue;
        }
        checked += 1;
        let merged = parse(&result.text);
        assert_eq!(
            structured(&base, &merged, &merged).text,
            result.text,
            "seed {seed}"
        );
        for side in [&ours, &theirs] {
            let remerge = structured(&base, &merged, side);
            if remerge.is_clean() {
                assert_eq!(remerge.text, result.text, "seed {seed}");
                continue;
            }
            prose_overlaps += 1;
            for field in conflict_fields(&remerge) {
                assert!(
                    matches!(field, IssueField::Section(_)),
                    "seed {seed}: re-merge conflicted on {field}"
                );
            }
            assert_eq!(
                resolve(&remerge.text, Side::Ours),
                result.text,
                "seed {seed}: re-merge disturbed text outside its prose hunks"
            );
        }
    }
    assert!(checked > PROPERTY_CASES / 4, "{checked} clean merges");
    assert!(
        prose_overlaps * 20 < checked,
        "{prose_overlaps} of {checked}"
    );
}

#[test]
fn property_disjoint_edits_always_merge_with_both_edits() {
    for seed in 0..PROPERTY_CASES {
        let mut rng = StdRng::seed_from_u64(seed ^ 0xD15_7017);
        let base = random_base(&mut rng);
        let mut kinds = EDIT_KINDS;
        kinds.shuffle(&mut rng);
        let split = rng.gen_range(0..=kinds.len());
        let (ours_kinds, theirs_kinds) = kinds.split_at(split);
        let ours = random_descendant(&mut rng, &base, ours_kinds, "ours", 10);
        let theirs = random_descendant(&mut rng, &base, theirs_kinds, "theirs", 20);

        let result = merge(&base, &ours, &theirs);
        assert!(result.is_clean(), "seed {seed}: {:?}", result.conflicts);
        let merged = parse(&result.text);
        let expect = |side: &Issue, kinds: &[EditKind]| {
            for kind in kinds {
                match kind {
                    EditKind::Title => assert_eq!(merged.title, side.title, "seed {seed}"),
                    EditKind::Status => {
                        assert_eq!(merged.status, side.status, "seed {seed}");
                        assert_eq!(merged.closed_at, side.closed_at, "seed {seed}");
                    }
                    EditKind::Priority => assert_eq!(merged.priority, side.priority, "seed {seed}"),
                    EditKind::Assignee => assert_eq!(merged.assignee, side.assignee, "seed {seed}"),
                    EditKind::Labels => assert_eq!(merged.labels, side.labels, "seed {seed}"),
                    EditKind::Dependencies => {
                        assert_eq!(merged.depends_on, side.depends_on, "seed {seed}")
                    }
                    EditKind::Description => {
                        assert_eq!(merged.description, side.description, "seed {seed}")
                    }
                    EditKind::Notes => assert_eq!(merged.notes, side.notes, "seed {seed}"),
                }
            }
        };
        expect(&ours, ours_kinds);
        expect(&theirs, theirs_kinds);
    }
}

#[test]
fn property_conflict_output_is_machine_resolvable_and_precise() {
    for (seed, base, ours, theirs) in generated_triples() {
        let result = merge(&base, &ours, &theirs);
        if result.is_clean() {
            assert_eq!(hunk_count(&result.text), 0, "seed {seed}");
            continue;
        }
        assert!(hunk_count(&result.text) >= 1, "seed {seed}");
        let conflicts = conflict_fields(&result);
        let took_ours = parse(&resolve(&result.text, Side::Ours));
        let took_theirs = parse(&resolve(&result.text, Side::Theirs));
        for field in FRONTMATTER_FIELDS.iter().cloned().chain(
            DEPENDENCY_IDS
                .iter()
                .map(|id| IssueField::Dependency((*id).into())),
        ) {
            if conflicts.contains(&field) {
                assert_eq!(
                    field_value(&took_ours, &field),
                    field_value(&ours, &field),
                    "seed {seed} {field}"
                );
                assert_eq!(
                    field_value(&took_theirs, &field),
                    field_value(&theirs, &field),
                    "seed {seed} {field}"
                );
            } else {
                // Hunks never touch fields that merged.
                assert_eq!(
                    field_value(&took_ours, &field),
                    field_value(&took_theirs, &field),
                    "seed {seed} {field}"
                );
            }
        }
        for section in Section::ALL {
            if !conflicts.contains(&IssueField::Section(section)) {
                assert_eq!(
                    section.content(&took_ours),
                    section.content(&took_theirs),
                    "seed {seed}"
                );
            }
        }
        assert_eq!(took_ours.labels, took_theirs.labels, "seed {seed}");
        assert_eq!(
            took_ours.updated_at,
            ours.updated_at.max(theirs.updated_at),
            "seed {seed}"
        );
    }
}

const COMMENT_BODIES: [&str; 4] = ["Looks good.", "Needs work.", "Ping.", "Done."];

fn random_comment_base(rng: &mut StdRng) -> Vec<Comment> {
    (0..rng.gen_range(0..5))
        .map(|index| {
            let minute = Minute::from(index) * 10;
            if rng.gen_bool(0.5) {
                imported(index, pick(rng, &COMMENT_BODIES), minute, minute)
            } else {
                comment(
                    &format!("c-base-{index}"),
                    pick(rng, &COMMENT_BODIES),
                    minute,
                )
            }
        })
        .collect()
}

fn random_comment_descendant(rng: &mut StdRng, base: &[Comment], tag: &str) -> Vec<Comment> {
    let mut comments: Vec<Comment> = base
        .iter()
        .filter(|_| !rng.gen_bool(0.2))
        .cloned()
        .collect();
    for comment in &mut comments {
        if comment.source_id.is_some() && rng.gen_bool(0.2) {
            // A newer GitHub revision pulled by this side.
            comment.body = format!("{} (edited, seen by {tag})", comment.body);
            comment.updated_at += Duration::minutes(rng.gen_range(1..30));
        } else if comment.source_id.is_none() && rng.gen_bool(0.08) {
            comment.body = format!("{} (rewritten by {tag})", comment.body);
        }
    }
    for index in 0..rng.gen_range(0..3) {
        let minute = 100 + rng.gen_range(0..50);
        if rng.gen_bool(0.3) {
            // A GitHub comment both sides may import independently.
            let remote_id = 500 + rng.gen_range(0..3);
            comments.push(imported(
                remote_id,
                "Shared remote",
                Minute::from(remote_id),
                Minute::from(remote_id),
            ));
        } else {
            comments.push(comment(
                &format!("c-{tag}-{index}"),
                pick(rng, &COMMENT_BODIES),
                minute,
            ));
        }
    }
    let mut seen = BTreeSet::new();
    comments.retain(|comment| seen.insert(comment.id.clone()));
    comments
}

fn assert_no_lost_or_duplicated_comments(
    seed: Seed,
    merged: &[Comment],
    base: &[Comment],
    ours: &[Comment],
    theirs: &[Comment],
) {
    let merged_ids = ids(merged);
    let unique: BTreeSet<&str> = merged_ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        merged_ids.len(),
        "seed {seed}: duplicate IDs {merged_ids:?}"
    );
    for id in ids(ours).into_iter().chain(ids(theirs)) {
        assert!(unique.contains(id), "seed {seed}: lost comment {id}");
    }
    for id in &unique {
        assert!(
            ids(base).contains(id) || ids(ours).contains(id) || ids(theirs).contains(id),
            "seed {seed}: invented comment {id}"
        );
        assert!(
            ids(ours).contains(id) || ids(theirs).contains(id),
            "seed {seed}: resurrected comment {id} deleted on both sides"
        );
    }
    let mut sorted = merged.to_vec();
    sorted.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    assert_eq!(ids(&sorted), merged_ids, "seed {seed}: not in time order");
}

fn generated_comment_triples(
) -> impl Iterator<Item = (Seed, Vec<Comment>, Vec<Comment>, Vec<Comment>)> {
    (0..PROPERTY_CASES).map(|seed| {
        let mut rng = StdRng::seed_from_u64(seed ^ 0xC0_44E7);
        let base = random_comment_base(&mut rng);
        let ours = random_comment_descendant(&mut rng, &base, "ours");
        let theirs = random_comment_descendant(&mut rng, &base, "theirs");
        (seed, base, ours, theirs)
    })
}

#[test]
fn property_comment_merge_never_loses_or_duplicates_comments() {
    let mut conflicted = 0;
    for (seed, base, ours, theirs) in generated_comment_triples() {
        let result = merge_comment_lists(&base, &ours, &theirs);
        if result.is_clean() {
            let merged = parse_comments(&result.text);
            assert_no_lost_or_duplicated_comments(seed, &merged, &base, &ours, &theirs);
            for comment in &merged {
                let known = ours.iter().chain(&theirs).any(|side| side == comment);
                let merged_body = ours.iter().any(|side| side.id == comment.id)
                    && theirs.iter().any(|side| side.id == comment.id);
                assert!(known || merged_body, "seed {seed}: fabricated {comment:?}");
            }
        } else {
            conflicted += 1;
            for side in [Side::Ours, Side::Theirs] {
                let merged = parse_comments(&resolve(&result.text, side));
                assert_no_lost_or_duplicated_comments(seed, &merged, &base, &ours, &theirs);
            }
        }
    }
    assert!(
        conflicted > 0,
        "generator never produced a comment conflict"
    );
}

#[test]
fn property_comment_merge_is_commutative_and_idempotent() {
    for (seed, base, ours, theirs) in generated_comment_triples() {
        let forward = merge_comment_lists(&base, &ours, &theirs);
        let backward = merge_comment_lists(&base, &theirs, &ours);
        assert_eq!(forward.conflicts, backward.conflicts, "seed {seed}");
        if !forward.is_clean() {
            assert_eq!(
                resolve(&forward.text, Side::Ours),
                resolve(&backward.text, Side::Theirs),
                "seed {seed}"
            );
            continue;
        }
        assert_eq!(forward.text, backward.text, "seed {seed}");
        let merged = parse_comments(&forward.text);
        assert_eq!(
            structured_comments(&base, &merged, &ours).text,
            forward.text,
            "seed {seed}"
        );
        assert_eq!(
            structured_comments(&base, &merged, &theirs).text,
            forward.text,
            "seed {seed}"
        );
        assert_eq!(
            structured_comments(&base, &ours, &ours).text,
            render_comments(&ours),
            "seed {seed}"
        );
    }
}
