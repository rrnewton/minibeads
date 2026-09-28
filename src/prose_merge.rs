use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

type ByteOffset = usize;
type ByteCount = usize;
type PieceIndex = usize;
type MatchCount = usize;
type CellCount = usize;

const MAX_INPUT_BYTES: ByteCount = 1 << 20;
const MAX_PIECES: PieceIndex = 4096;
const MAX_CELLS: CellCount = 1_000_000;
const MAX_COMPARISON_BYTES: ByteCount = 16 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProseConflict {
    OverlappingEdits,
    DeleteVsEdit,
    AmbiguousAlignment,
    AmbiguousContent,
    WorkLimitExceeded,
}

impl fmt::Display for ProseConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OverlappingEdits => "overlapping prose edits",
            Self::DeleteVsEdit => "prose deletion overlaps an edit",
            Self::AmbiguousAlignment => "prose has multiple optimal edit alignments",
            Self::AmbiguousContent => "content is not safely refinable as plain prose",
            Self::WorkLimitExceeded => "prose merge work limit exceeded",
        })
    }
}

impl std::error::Error for ProseConflict {}

/// Merges exact paragraph edits, refining overlaps into whitespace-separated words.
/// Unchanged bytes are copied verbatim; fast paths borrow their winning input.
/// Concurrent standalone paragraph insertions retain local order before remote
/// order, deduplicating identical paragraphs apart from framing CR/LF separators.
/// Repeated additions or shared paragraphs requiring reordering fail closed.
/// Only plain prose can be refined or have distinct same-gap additions combined.
/// Markup, code-like punctuation, tabs, and indentation disable those operations
/// throughout the body. Words and their attached punctuation are indivisible.
/// Repeated-text alignments must be unique, and overlapping changes fail closed.
/// Nontrivial merges allow at most 1 MiB per input, 4096 pieces per segmentation,
/// one million total LCS cells, and 16 MiB of charged text comparisons. Equality
/// scans and segmentation are linear; the limits do not apply to borrowed paths.
pub(crate) fn merge_prose<'a>(
    ancestor: &'a str,
    local: &'a str,
    remote: &'a str,
) -> Result<Cow<'a, str>, ProseConflict> {
    if local == remote || remote == ancestor {
        return Ok(Cow::Borrowed(local));
    }
    if local == ancestor {
        return Ok(Cow::Borrowed(remote));
    }
    if [ancestor.len(), local.len(), remote.len()]
        .into_iter()
        .any(|length| length > MAX_INPUT_BYTES)
    {
        return Err(ProseConflict::WorkLimitExceeded);
    }

    let mut budget = WorkBudget::new();
    let allow_refinement = [ancestor, local, remote].into_iter().all(is_plain_prose);
    let mut merged = String::with_capacity(local.len().max(remote.len()));
    merge_level(
        ancestor,
        local,
        remote,
        Level::Paragraphs,
        allow_refinement,
        &mut budget,
        &mut merged,
    )?;
    Ok(Cow::Owned(merged))
}

struct WorkBudget {
    cells: CellCount,
    comparison_bytes: ByteCount,
}

impl WorkBudget {
    fn new() -> Self {
        Self {
            cells: MAX_CELLS,
            comparison_bytes: MAX_COMPARISON_BYTES,
        }
    }

    fn reserve_cells(&mut self, cells: CellCount) -> Result<(), ProseConflict> {
        self.cells = self
            .cells
            .checked_sub(cells)
            .ok_or(ProseConflict::WorkLimitExceeded)?;
        Ok(())
    }

    fn equal(&mut self, left: &str, right: &str) -> Result<bool, ProseConflict> {
        if left.len() != right.len() {
            return Ok(false);
        }
        self.comparison_bytes = self
            .comparison_bytes
            .checked_sub(left.len())
            .ok_or(ProseConflict::WorkLimitExceeded)?;
        Ok(left == right)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Level {
    Paragraphs,
    Words,
}

struct Pieces<'a> {
    source: &'a str,
    boundaries: Vec<ByteOffset>,
}

impl<'a> Pieces<'a> {
    fn new(source: &'a str, level: Level) -> Result<Self, ProseConflict> {
        let mut pieces = Self {
            source,
            boundaries: vec![0],
        };
        match level {
            Level::Paragraphs => {
                let mut offset = 0;
                let mut previous_blank = false;
                for line in source.split_inclusive('\n') {
                    let blank = line.trim().is_empty();
                    if !blank && previous_blank {
                        pieces.push_boundary(offset)?;
                    }
                    offset += line.len();
                    previous_blank = blank;
                }
            }
            Level::Words => {
                let mut previous_whitespace = None;
                for (offset, character) in source.char_indices() {
                    let whitespace = character.is_whitespace();
                    if previous_whitespace.is_some_and(|previous| previous != whitespace) {
                        pieces.push_boundary(offset)?;
                    }
                    previous_whitespace = Some(whitespace);
                }
            }
        }
        pieces.push_boundary(source.len())?;
        Ok(pieces)
    }

    fn push_boundary(&mut self, offset: ByteOffset) -> Result<(), ProseConflict> {
        if self.boundaries.last() == Some(&offset) {
            return Ok(());
        }
        if self.boundaries.len() > MAX_PIECES {
            return Err(ProseConflict::WorkLimitExceeded);
        }
        self.boundaries.push(offset);
        Ok(())
    }

    fn len(&self) -> PieceIndex {
        self.boundaries.len() - 1
    }

    fn piece(&self, index: PieceIndex) -> &'a str {
        &self.source[self.boundaries[index]..self.boundaries[index + 1]]
    }

    fn span(&self, start: PieceIndex, end: PieceIndex) -> Range<ByteOffset> {
        self.boundaries[start]..self.boundaries[end]
    }
}

#[derive(Debug)]
struct Edit<'a> {
    ancestor: Range<ByteOffset>,
    replacement: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MatchedPieces {
    ancestor: PieceIndex,
    side: PieceIndex,
}

fn diff<'a>(
    ancestor: &Pieces<'_>,
    side: &Pieces<'a>,
    budget: &mut WorkBudget,
) -> Result<Vec<Edit<'a>>, ProseConflict> {
    let columns = side.len() + 1;
    let cells = (ancestor.len() + 1)
        .checked_mul(columns)
        .ok_or(ProseConflict::WorkLimitExceeded)?;
    budget.reserve_cells(cells)?;
    let mut lengths: Vec<MatchCount> = vec![0; cells];
    for ancestor_index in (0..ancestor.len()).rev() {
        for side_index in (0..side.len()).rev() {
            lengths[ancestor_index * columns + side_index] =
                if budget.equal(ancestor.piece(ancestor_index), side.piece(side_index))? {
                    lengths[(ancestor_index + 1) * columns + side_index + 1] + 1
                } else {
                    lengths[(ancestor_index + 1) * columns + side_index]
                        .max(lengths[ancestor_index * columns + side_index + 1])
                };
        }
    }

    let mut edits = Vec::new();
    let mut ancestor_start = 0;
    let mut side_start = 0;
    let mut remaining = lengths[0];
    while remaining > 0 {
        let mut next_match = None;
        for ancestor_index in ancestor_start..ancestor.len() {
            if lengths[ancestor_index * columns + side_start] < remaining {
                break;
            }
            for side_index in side_start..side.len() {
                if lengths[ancestor_index * columns + side_index] < remaining {
                    break;
                }
                if lengths[(ancestor_index + 1) * columns + side_index + 1] + 1 == remaining
                    && budget.equal(ancestor.piece(ancestor_index), side.piece(side_index))?
                {
                    if next_match.is_some() {
                        return Err(ProseConflict::AmbiguousAlignment);
                    }
                    next_match = Some(MatchedPieces {
                        ancestor: ancestor_index,
                        side: side_index,
                    });
                }
            }
        }
        let matched = next_match.ok_or(ProseConflict::AmbiguousAlignment)?;
        push_edit(
            &mut edits,
            ancestor,
            side,
            ancestor_start..matched.ancestor,
            side_start..matched.side,
        );
        ancestor_start = matched.ancestor + 1;
        side_start = matched.side + 1;
        remaining -= 1;
    }
    push_edit(
        &mut edits,
        ancestor,
        side,
        ancestor_start..ancestor.len(),
        side_start..side.len(),
    );
    Ok(edits)
}

fn push_edit<'a>(
    edits: &mut Vec<Edit<'a>>,
    ancestor: &Pieces<'_>,
    side: &Pieces<'a>,
    ancestor_range: Range<PieceIndex>,
    side_range: Range<PieceIndex>,
) {
    if !ancestor_range.is_empty() || !side_range.is_empty() {
        edits.push(Edit {
            ancestor: ancestor.span(ancestor_range.start, ancestor_range.end),
            replacement: &side.source[side.span(side_range.start, side_range.end)],
        });
    }
}

fn overlaps(left: &Range<ByteOffset>, right: &Range<ByteOffset>) -> bool {
    if left.is_empty() || right.is_empty() {
        left.start <= right.end && right.start <= left.end
    } else {
        left.start < right.end && right.start < left.end
    }
}

fn render_region<'a>(
    ancestor: &'a str,
    edits: &[Edit<'a>],
    region: &Range<ByteOffset>,
) -> Cow<'a, str> {
    if let [edit] = edits {
        if edit.ancestor == *region {
            return Cow::Borrowed(edit.replacement);
        }
    }
    let mut rendered = String::new();
    let mut cursor = region.start;
    for edit in edits {
        rendered.push_str(&ancestor[cursor..edit.ancestor.start]);
        rendered.push_str(edit.replacement);
        cursor = edit.ancestor.end;
    }
    rendered.push_str(&ancestor[cursor..region.end]);
    Cow::Owned(rendered)
}

fn merge_level(
    ancestor: &str,
    local: &str,
    remote: &str,
    level: Level,
    allow_refinement: bool,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), ProseConflict> {
    let ancestor_pieces = Pieces::new(ancestor, level)?;
    let local_edits = diff(&ancestor_pieces, &Pieces::new(local, level)?, budget)?;
    let remote_edits = diff(&ancestor_pieces, &Pieces::new(remote, level)?, budget)?;
    let mut local_index = 0;
    let mut remote_index = 0;
    let mut cursor = 0;

    while local_index < local_edits.len() || remote_index < remote_edits.len() {
        let local_next = local_edits.get(local_index);
        let remote_next = remote_edits.get(remote_index);
        if let (Some(local_edit), Some(remote_edit)) = (local_next, remote_next) {
            if overlaps(&local_edit.ancestor, &remote_edit.ancestor) {
                let mut region = local_edit.ancestor.start.min(remote_edit.ancestor.start)
                    ..local_edit.ancestor.end.max(remote_edit.ancestor.end);
                let local_start = local_index;
                let remote_start = remote_index;
                local_index += 1;
                remote_index += 1;
                loop {
                    let previous_indices = (local_index, remote_index);
                    while let Some(edit) = local_edits.get(local_index) {
                        if !overlaps(&region, &edit.ancestor) {
                            break;
                        }
                        region.end = region.end.max(edit.ancestor.end);
                        local_index += 1;
                    }
                    while let Some(edit) = remote_edits.get(remote_index) {
                        if !overlaps(&region, &edit.ancestor) {
                            break;
                        }
                        region.end = region.end.max(edit.ancestor.end);
                        remote_index += 1;
                    }
                    if previous_indices == (local_index, remote_index) {
                        break;
                    }
                }

                output.push_str(&ancestor[cursor..region.start]);
                let local_region =
                    render_region(ancestor, &local_edits[local_start..local_index], &region);
                let remote_region =
                    render_region(ancestor, &remote_edits[remote_start..remote_index], &region);
                if budget.equal(&local_region, &remote_region)? {
                    output.push_str(&local_region);
                } else if local_region.is_empty() || remote_region.is_empty() {
                    return Err(ProseConflict::DeleteVsEdit);
                } else if let Some(additions) =
                    paragraph_additions(ancestor, &region, &local_region, &remote_region)
                {
                    if !allow_refinement {
                        return Err(ProseConflict::AmbiguousContent);
                    }
                    output.push_str(additions.before);
                    merge_additions(additions.local, additions.remote, budget, output)?;
                    output.push_str(additions.after);
                } else if level == Level::Paragraphs {
                    if !allow_refinement {
                        return Err(ProseConflict::AmbiguousContent);
                    }
                    merge_level(
                        &ancestor[region.start..region.end],
                        &local_region,
                        &remote_region,
                        Level::Words,
                        allow_refinement,
                        budget,
                        output,
                    )?;
                } else {
                    return Err(ProseConflict::OverlappingEdits);
                }
                cursor = region.end;
                continue;
            }
        }

        let next_edit = match (local_next, remote_next) {
            (Some(local_edit), Some(remote_edit))
                if remote_edit.ancestor.start < local_edit.ancestor.start =>
            {
                remote_index += 1;
                remote_edit
            }
            (Some(local_edit), _) => {
                local_index += 1;
                local_edit
            }
            (None, Some(remote_edit)) => {
                remote_index += 1;
                remote_edit
            }
            (None, None) => break,
        };
        output.push_str(&ancestor[cursor..next_edit.ancestor.start]);
        output.push_str(next_edit.replacement);
        cursor = next_edit.ancestor.end;
    }
    output.push_str(&ancestor[cursor..]);
    Ok(())
}

fn trailing_whitespace(text: &str) -> &str {
    &text[text.trim_end().len()..]
}

fn leading_whitespace(text: &str) -> &str {
    &text[..text.len() - text.trim_start().len()]
}

fn has_paragraph_break(left: &str, right: &str) -> bool {
    left.bytes()
        .chain(right.bytes())
        .filter(|byte| *byte == b'\n')
        .take(2)
        .count()
        == 2
}

fn standalone_addition(ancestor: &str, position: ByteOffset, addition: &str) -> bool {
    if addition.trim().is_empty() {
        return false;
    }
    let before = &ancestor[..position];
    let after = &ancestor[position..];
    (before.trim().is_empty()
        || has_paragraph_break(trailing_whitespace(before), leading_whitespace(addition)))
        && (after.trim().is_empty()
            || has_paragraph_break(trailing_whitespace(addition), leading_whitespace(after)))
}

struct ParagraphAdditions<'a> {
    before: &'a str,
    local: &'a str,
    remote: &'a str,
    after: &'a str,
}

fn paragraph_additions<'a>(
    ancestor: &'a str,
    region: &Range<ByteOffset>,
    local: &'a str,
    remote: &'a str,
) -> Option<ParagraphAdditions<'a>> {
    let original = &ancestor[region.start..region.end];
    if let (Some(local), Some(remote)) =
        (local.strip_prefix(original), remote.strip_prefix(original))
    {
        if standalone_addition(ancestor, region.end, local)
            && standalone_addition(ancestor, region.end, remote)
        {
            return Some(ParagraphAdditions {
                before: original,
                local,
                remote,
                after: "",
            });
        }
    }
    if let (Some(local), Some(remote)) =
        (local.strip_suffix(original), remote.strip_suffix(original))
    {
        if standalone_addition(ancestor, region.start, local)
            && standalone_addition(ancestor, region.start, remote)
        {
            return Some(ParagraphAdditions {
                before: "",
                local,
                remote,
                after: original,
            });
        }
    }
    None
}

fn merge_additions(
    local: &str,
    remote: &str,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), ProseConflict> {
    let local_pieces = Pieces::new(local, Level::Paragraphs)?;
    let remote_pieces = Pieces::new(remote, Level::Paragraphs)?;
    require_unique_additions(&local_pieces, budget)?;
    require_unique_additions(&remote_pieces, budget)?;
    let newline = if local.contains("\r\n") || remote.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    output.push_str(local);
    let mut appended_remote = false;
    let mut last_shared: Option<PieceIndex> = None;
    for remote_index in 0..remote_pieces.len() {
        let remote_piece = remote_pieces.piece(remote_index);
        let remote_content = addition_content(remote_piece);
        let mut duplicate = false;
        for local_index in 0..local_pieces.len() {
            budget.reserve_cells(1)?;
            if budget.equal(
                addition_content(local_pieces.piece(local_index)),
                remote_content,
            )? {
                if !remote_content.is_empty() {
                    if last_shared.is_some_and(|previous| local_index <= previous) {
                        return Err(ProseConflict::AmbiguousAlignment);
                    }
                    last_shared = Some(local_index);
                }
                duplicate = true;
                break;
            }
        }
        if duplicate && appended_remote && !remote_content.is_empty() {
            return Err(ProseConflict::AmbiguousAlignment);
        }
        if !duplicate {
            appended_remote |= !remote_content.is_empty();
            let trailing = trailing_whitespace(output);
            let leading = leading_whitespace(remote_piece);
            if !has_paragraph_break(trailing, leading) {
                if !trailing.contains('\n') && !leading.contains('\n') {
                    output.push_str(newline);
                }
                output.push_str(newline);
            }
            output.push_str(remote_piece);
        }
    }
    Ok(())
}

fn addition_content(paragraph: &str) -> &str {
    paragraph.trim_matches(['\r', '\n'])
}

fn require_unique_additions(
    pieces: &Pieces<'_>,
    budget: &mut WorkBudget,
) -> Result<(), ProseConflict> {
    for index in 0..pieces.len() {
        let content = addition_content(pieces.piece(index));
        if content.is_empty() {
            continue;
        }
        for earlier in 0..index {
            budget.reserve_cells(1)?;
            if budget.equal(content, addition_content(pieces.piece(earlier)))? {
                return Err(ProseConflict::AmbiguousAlignment);
            }
        }
    }
    Ok(())
}

fn is_plain_prose(text: &str) -> bool {
    if text.contains('\t') || text.lines().any(|line| line.starts_with("    ")) {
        return false;
    }
    text.split_whitespace().all(|word| {
        let core = word.trim_end_matches(['.', ',', '!', '?', ';']);
        let mut characters = core.chars().peekable();
        let mut previous_letter = false;
        while let Some(character) = characters.next() {
            if character == '-' || character == '\'' || character == '’' {
                if !previous_letter || !characters.peek().is_some_and(|next| next.is_alphanumeric())
                {
                    return false;
                }
                previous_letter = false;
            } else if character.is_alphanumeric()
                || (!character.is_ascii() && !character.is_control())
            {
                previous_letter = true;
            } else {
                return false;
            }
        }
        !core.is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_merge(ancestor: &str, local: &str, remote: &str, expected: &str) {
        let merged = merge_prose(ancestor, local, remote).unwrap();
        assert_eq!(merged, expected);
        assert_eq!(merge_prose(ancestor, local, remote).unwrap(), merged);
        assert_eq!(merge_prose(ancestor, &merged, &merged).unwrap(), merged);
        assert_eq!(merge_prose(local, &merged, local).unwrap(), merged);
        assert_eq!(merge_prose(remote, remote, &merged).unwrap(), merged);
    }

    #[test]
    fn fast_paths_borrow_winning_inputs() {
        for (ancestor, local, remote, expected) in [
            ("same", "same", "same", "same"),
            ("old", "new", "new", "new"),
            ("old", "new", "old", "new"),
            ("old", "old", "new", "new"),
            ("old", "", "old", ""),
            ("old", "", "", ""),
        ] {
            assert!(matches!(
                merge_prose(ancestor, local, remote),
                Ok(Cow::Borrowed(actual)) if actual == expected
            ));
        }
    }

    #[test]
    fn independent_paragraph_additions() {
        assert_merge(
            "First.\n\nLast.",
            "First.\n\nLocal.\n\nLast.",
            "Remote.\n\nFirst.\n\nLast.",
            "Remote.\n\nFirst.\n\nLocal.\n\nLast.",
        );
    }

    #[test]
    fn same_gap_paragraph_additions_are_local_first() {
        assert_merge(
            "First.\n\nLast.",
            "First.\n\nLocal.\n\nLast.",
            "First.\n\nRemote.\n\nLast.",
            "First.\n\nLocal.\n\nRemote.\n\nLast.",
        );
    }

    #[test]
    fn same_gap_identical_paragraphs_are_deduplicated() {
        assert_merge(
            "End.",
            "Shared.\n\nLocal.\n\nEnd.",
            "Shared.\n\nRemote.\n\nEnd.",
            "Shared.\n\nLocal.\n\nRemote.\n\nEnd.",
        );
    }

    #[test]
    fn appending_paragraphs_to_unterminated_body() {
        assert_merge(
            "Base.",
            "Base.\n\nLocal.",
            "Base.\n\nRemote.",
            "Base.\n\nLocal.\n\nRemote.",
        );
    }

    #[test]
    fn partial_write_retry_does_not_duplicate_same_gap_paragraphs() {
        let ancestor = "Base.";
        let local = "Base.\n\nLocal.";
        let remote = "Base.\n\nRemote.";
        let merged = merge_prose(ancestor, local, remote).unwrap();
        assert_eq!(merge_prose(ancestor, local, &merged).unwrap(), merged);
        assert_eq!(merge_prose(ancestor, &merged, remote).unwrap(), merged);
    }

    #[test]
    fn repeated_added_paragraphs_are_not_silently_discarded() {
        assert_eq!(
            merge_prose("", "Shared.\n\nShared.", "Shared.\n\nRemote."),
            Err(ProseConflict::AmbiguousAlignment),
        );
    }

    #[test]
    fn shared_additions_are_not_silently_reordered() {
        assert_eq!(
            merge_prose("", "Shared.\n\nLocal.", "Remote.\n\nShared."),
            Err(ProseConflict::AmbiguousAlignment),
        );
        assert_eq!(
            merge_prose("", "First.\n\nLast.", "Last.\n\nFirst."),
            Err(ProseConflict::AmbiguousAlignment),
        );
    }

    #[test]
    fn paragraph_additions_preserve_existing_boundary_newlines() {
        assert_merge(
            "Base.\n",
            "Base.\n\nLocal.",
            "Base.\n\nRemote.",
            "Base.\n\nLocal.\n\nRemote.",
        );
        assert_merge(
            "Base.\r\n",
            "Base.\r\n\r\nLocal.",
            "Base.\r\n\r\nRemote.",
            "Base.\r\n\r\nLocal.\r\n\r\nRemote.",
        );
        assert_merge(
            "\nBase.",
            "Local.\n\n\nBase.",
            "Remote.\n\n\nBase.",
            "Local.\n\nRemote.\n\n\nBase.",
        );
    }

    #[test]
    fn disjoint_replacements_in_one_line() {
        assert_merge(
            "The red fox runs quickly.",
            "The blue fox runs quickly.",
            "The red fox runs slowly.",
            "The blue fox runs slowly.",
        );
    }

    #[test]
    fn disjoint_paragraph_replacements() {
        assert_merge(
            "First old.\n\nLast old.",
            "First new.\n\nLast old.",
            "First old.\n\nLast new.",
            "First new.\n\nLast new.",
        );
    }

    #[test]
    fn shared_edit_and_disjoint_edits() {
        assert_merge(
            "Red fox runs fast.",
            "Blue fox walks fast.",
            "Blue fox runs slowly.",
            "Blue fox walks slowly.",
        );
    }

    #[test]
    fn conflicting_word_replacements() {
        assert_eq!(
            merge_prose("foo", "bar", "baz"),
            Err(ProseConflict::OverlappingEdits)
        );
        assert_eq!(
            merge_prose("prefix", "prelude", "suffix"),
            Err(ProseConflict::OverlappingEdits)
        );
    }

    #[test]
    fn delete_versus_edit_conflicts() {
        for (ancestor, local, remote) in [
            ("foo", "", "bar"),
            (
                "Keep.\n\nOld.\n\nEnd.",
                "Keep.\n\nEnd.",
                "Keep.\n\nNew.\n\nEnd.",
            ),
            ("The red fox.", "The fox.", "The blue fox."),
        ] {
            assert!(merge_prose(ancestor, local, remote).is_err());
            assert!(merge_prose(ancestor, remote, local).is_err());
        }
        assert_eq!(
            merge_prose("foo", "", "bar"),
            Err(ProseConflict::DeleteVsEdit)
        );
    }

    #[test]
    fn reflow_and_independent_word_replacement() {
        assert_merge(
            "The red fox runs quickly.",
            "The red fox\nruns quickly.",
            "The blue fox runs quickly.",
            "The blue fox\nruns quickly.",
        );
    }

    #[test]
    fn unicode_words_and_combining_marks_are_lossless() {
        assert_merge(
            "Cafe\u{301} 猫 sleeps happily.",
            "Cafe\u{301} 犬 sleeps happily.",
            "Cafe\u{301} 猫 sleeps peacefully.",
            "Cafe\u{301} 犬 sleeps peacefully.",
        );
        assert_merge("", "你好。", "🌍", "你好。\n\n🌍");
    }

    #[test]
    fn empty_bodies() {
        assert_merge("", "", "", "");
        assert_merge("", "Local.", "", "Local.");
        assert_merge("", "Local.", "Remote.", "Local.\n\nRemote.");
        assert_merge("", "Same.", "Same.", "Same.");
        assert_merge("Text.", "", "", "");
    }

    #[test]
    fn unchanged_whitespace_and_line_endings_are_preserved() {
        assert_merge(
            "  First old. \r\n \r\nLast old.  \r\n",
            "  First new. \r\n \r\nLast old.  \r\n",
            "  First old. \r\n \r\nLast new.  \r\n",
            "  First new. \r\n \r\nLast new.  \r\n",
        );
        assert_merge(
            "One  red\u{a0}fox.\r\n",
            "One  blue\u{a0}fox.\r\n",
            "One  red\u{a0}cat.\r\n",
            "One  blue\u{a0}cat.\r\n",
        );
    }

    #[test]
    fn inline_and_code_tokens_are_not_combined() {
        for (ancestor, local, remote) in [
            ("value_name", "other_name", "value_kind"),
            (
                "Call foo(bar, baz)",
                "Call foo(new, baz)",
                "Call foo(bar, old)",
            ),
            (
                "Use `red fox` now.",
                "Use `blue fox` now.",
                "Use `red cat` now.",
            ),
            ("word", "preword", "wordsuffix"),
            ("foo-bar", "new-bar", "foo-baz"),
            ("", "```\nlocal\n```", "```\nremote\n```"),
            ("    red fox", "    blue fox", "    red cat"),
        ] {
            assert!(
                merge_prose(ancestor, local, remote).is_err(),
                "{ancestor:?}"
            );
        }
    }

    #[test]
    fn same_gap_inline_additions_conflict() {
        assert!(merge_prose("The fox.", "The red fox.", "The blue fox.").is_err());
        assert!(merge_prose("Base.", "Base.\nLocal.", "Base.\nRemote.").is_err());
    }

    #[test]
    fn repeated_text_alignment_fails_closed() {
        assert_eq!(
            merge_prose("foo foo", "foo", "foo bar"),
            Err(ProseConflict::AmbiguousAlignment)
        );
        assert_eq!(
            merge_prose("Same.\n\nSame.\n\n", "Same.\n\n", "New.\n\nSame.\n\n"),
            Err(ProseConflict::AmbiguousAlignment)
        );
    }

    #[test]
    fn segmentation_is_byte_exact() {
        for source in [
            "",
            "\n",
            "\n\n",
            "One",
            "One\n\nTwo",
            "\r\n\r\nOne\r\n \r\nTwo  ",
            "Cafe\u{301}\t猫\u{a0}🌍\r\n\nEnd",
        ] {
            for level in [Level::Paragraphs, Level::Words] {
                let pieces = Pieces::new(source, level).unwrap();
                let mut reconstructed = String::new();
                for index in 0..pieces.len() {
                    reconstructed.push_str(pieces.piece(index));
                }
                assert_eq!(reconstructed.as_bytes(), source.as_bytes());
            }
        }
    }

    #[test]
    fn limits_do_not_affect_borrowed_fast_paths() {
        let huge = "word ".repeat(MAX_INPUT_BYTES / 5 + 1);
        assert!(matches!(merge_prose("", &huge, ""), Ok(Cow::Borrowed(_))));
        assert!(matches!(
            merge_prose("", &huge, &huge),
            Ok(Cow::Borrowed(_))
        ));
        assert_eq!(
            merge_prose("", &huge, "Other."),
            Err(ProseConflict::WorkLimitExceeded)
        );
    }

    #[test]
    fn expensive_alignment_has_a_typed_limit() {
        let ancestor = "word ".repeat(600);
        let local = format!("Local {ancestor}");
        let remote = format!("Remote {ancestor}");
        assert_eq!(
            merge_prose(&ancestor, &local, &remote),
            Err(ProseConflict::WorkLimitExceeded)
        );
    }
}
