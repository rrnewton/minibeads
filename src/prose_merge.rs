use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;

type ByteOffset = usize;
type ByteCount = usize;
type PieceIndex = usize;
type MatchCount = usize;
type WorkCount = usize;
type OccurrenceCount = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputByteLimit(ByteCount);

impl InputByteLimit {
    pub(crate) const fn new(bytes: ByteCount) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PieceLimit(PieceIndex);

impl PieceLimit {
    pub(crate) const fn new(pieces: PieceIndex) -> Self {
        Self(pieces)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AlignmentWorkLimit(WorkCount);

impl AlignmentWorkLimit {
    pub(crate) const fn new(steps: WorkCount) -> Self {
        Self(steps)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComparisonByteLimit(ByteCount);

impl ComparisonByteLimit {
    pub(crate) const fn new(bytes: ByteCount) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProseMergeLimits {
    max_input_bytes: InputByteLimit,
    max_pieces_per_input: PieceLimit,
    max_alignment_work: AlignmentWorkLimit,
    max_comparison_bytes: ComparisonByteLimit,
}

impl ProseMergeLimits {
    pub(crate) const fn new(
        max_input_bytes: InputByteLimit,
        max_pieces_per_input: PieceLimit,
        max_alignment_work: AlignmentWorkLimit,
        max_comparison_bytes: ComparisonByteLimit,
    ) -> Self {
        Self {
            max_input_bytes,
            max_pieces_per_input,
            max_alignment_work,
            max_comparison_bytes,
        }
    }
}

impl Default for ProseMergeLimits {
    fn default() -> Self {
        Self::new(
            InputByteLimit::new(1 << 20),
            PieceLimit::new(4096),
            AlignmentWorkLimit::new(2_000_000),
            ComparisonByteLimit::new(16 << 20),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProseMergeInputs<'a> {
    ancestor: &'a str,
    local: &'a str,
    remote: &'a str,
}

impl<'a> ProseMergeInputs<'a> {
    pub(crate) const fn new(ancestor: &'a str, local: &'a str, remote: &'a str) -> Self {
        Self {
            ancestor,
            local,
            remote,
        }
    }

    #[cfg(test)]
    pub(crate) const fn ancestor(self) -> &'a str {
        self.ancestor
    }

    #[cfg(test)]
    pub(crate) const fn local(self) -> &'a str {
        self.local
    }

    #[cfg(test)]
    pub(crate) const fn remote(self) -> &'a str {
        self.remote
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MergedProse<'a>(Cow<'a, str>);

impl<'a> MergedProse<'a> {
    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_text(self) -> Cow<'a, str> {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompetingEditReason {
    OverlappingEdits,
    DeleteVsEdit,
    AmbiguousRepeatedAlignment,
    AmbiguousInsertionGroup,
    /// A word-level merge would repeat or drop a word beyond what either side
    /// has, which happens when the two sides align against repeated text
    /// differently.
    InconsistentWordCounts,
    /// The merge does not lie on a shortest token edit path from the
    /// ancestor through each side, so it does not carry out some side's
    /// change as written: repeated text let the change land in another spot.
    MissesASideEdit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StructuredOverlapReason {
    MarkdownSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkExhaustionReason {
    InputBytes,
    Pieces,
    AlignmentWork,
    ComparisonBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MergeFailure<'a, Reason> {
    inputs: ProseMergeInputs<'a>,
    reason: Reason,
}

impl<'a, Reason: Copy> MergeFailure<'a, Reason> {
    #[cfg(test)]
    pub(crate) const fn inputs(self) -> ProseMergeInputs<'a> {
        self.inputs
    }

    pub(crate) const fn reason(self) -> Reason {
        self.reason
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ProseMergeResult<'a> {
    Merged(MergedProse<'a>),
    CompetingEdit(MergeFailure<'a, CompetingEditReason>),
    UnsupportedStructuredOverlap(MergeFailure<'a, StructuredOverlapReason>),
    WorkExhausted(MergeFailure<'a, WorkExhaustionReason>),
}

pub(crate) fn merge_prose<'a>(
    ancestor: &'a str,
    local: &'a str,
    remote: &'a str,
    limits: ProseMergeLimits,
) -> ProseMergeResult<'a> {
    let inputs = ProseMergeInputs::new(ancestor, local, remote);
    if inputs.local == inputs.remote || inputs.remote == inputs.ancestor {
        return ProseMergeResult::Merged(MergedProse(Cow::Borrowed(inputs.local)));
    }
    if inputs.local == inputs.ancestor {
        return ProseMergeResult::Merged(MergedProse(Cow::Borrowed(inputs.remote)));
    }

    match merge_changed(inputs, limits) {
        Ok(merged) => ProseMergeResult::Merged(MergedProse(merged)),
        Err(MergeError::Competing(reason)) => {
            ProseMergeResult::CompetingEdit(MergeFailure { inputs, reason })
        }
        Err(MergeError::Structured(reason)) => {
            ProseMergeResult::UnsupportedStructuredOverlap(MergeFailure { inputs, reason })
        }
        Err(MergeError::Exhausted(reason)) => {
            ProseMergeResult::WorkExhausted(MergeFailure { inputs, reason })
        }
    }
}

fn merge_changed<'a>(
    inputs: ProseMergeInputs<'a>,
    limits: ProseMergeLimits,
) -> Result<Cow<'a, str>, MergeError> {
    if [
        inputs.ancestor.len(),
        inputs.local.len(),
        inputs.remote.len(),
    ]
    .into_iter()
    .any(|length| length > limits.max_input_bytes.0)
    {
        return Err(MergeError::Exhausted(WorkExhaustionReason::InputBytes));
    }

    let [ancestor, local, remote] = [inputs.ancestor, inputs.local, inputs.remote];
    let piecewise = merge_piecewise(ancestor, local, remote, limits);
    match superseding_side(ancestor, local, remote, limits) {
        // A token edit script cannot tell deleting text from rewriting it, so
        // a side that deleted what the other rewrote looks contained in it;
        // the piecewise merge still sees the delete-versus-edit conflict.
        Some(_) if piecewise == Err(MergeError::Competing(CompetingEditReason::DeleteVsEdit)) => {
            Err(MergeError::Competing(CompetingEditReason::DeleteVsEdit))
        }
        Some(superset) => Ok(Cow::Borrowed(superset)),
        None => {
            let merged = piecewise?;
            require_both_sides_between(ancestor, local, remote, &merged, limits)?;
            Ok(Cow::Owned(merged))
        }
    }
}

/// Fail unless each side lies on a shortest token edit path from the ancestor
/// to `merged`, the containment test of [`superseding_side`] applied to the
/// result: some minimal edit script of `merged` then makes every edit of each
/// side, and edits both sides made alike are counted once. Alignment can still
/// carry a side's change over to identical text elsewhere, which this catches
/// when the move costs edits. Fails closed when the distances exceed the work
/// limits.
fn require_both_sides_between(
    ancestor: &str,
    local: &str,
    remote: &str,
    merged: &str,
    limits: ProseMergeLimits,
) -> Result<(), MergeError> {
    let mut budget = WorkBudget::new(limits);
    let to_merged = token_distance(ancestor, merged, &mut budget)?;
    for side in [local, remote] {
        let through_side = token_distance(ancestor, side, &mut budget)?
            + token_distance(side, merged, &mut budget)?;
        if through_side != to_merged {
            return Err(MergeError::Competing(CompetingEditReason::MissesASideEdit));
        }
    }
    Ok(())
}

/// Merge each side's edits of the ancestor, piece by piece.
fn merge_piecewise(
    ancestor: &str,
    local: &str,
    remote: &str,
    limits: ProseMergeLimits,
) -> Result<String, MergeError> {
    let (first, second) = match WorkBudget::new(limits).compare(local, remote)? {
        Ordering::Less => (local, remote),
        Ordering::Equal => unreachable!("equal replicas use the borrowed fast path"),
        Ordering::Greater => (remote, local),
    };

    // Each side is diffed against the ancestor on its own, so when text
    // repeats, the two sides can align it differently and a merge can repeat
    // or drop text that neither side did. Every longest common subsequence
    // lies between the earliest and the latest one, and the merge is accepted
    // only when it comes out the same with both sides aligned earliest and
    // with both aligned latest.
    let mut agreed: Option<String> = None;
    for alignment in Alignment::BOTH {
        let merged = merge_aligned(ancestor, first, second, alignment, limits)?;
        match &agreed {
            None => agreed = Some(merged),
            Some(previous) if *previous == merged => {}
            Some(_) => {
                return Err(MergeError::Competing(
                    CompetingEditReason::AmbiguousRepeatedAlignment,
                ))
            }
        }
    }
    Ok(agreed.unwrap_or_default())
}

/// When one side lies on a shortest token edit path from the ancestor to the
/// other, some minimal edit script of the other side makes every edit this
/// side made (a replayed or cherry-picked change, or one side building on the
/// other), so the merge is the other side as it stands. Deciding this needs no
/// alignment, so it holds however text repeats; the piecewise merge could
/// otherwise read the shared edits differently on each side and apply them
/// twice. `None` when neither side contains the other, or when checking would
/// exceed the work limits.
fn superseding_side<'a>(
    ancestor: &str,
    local: &'a str,
    remote: &'a str,
    limits: ProseMergeLimits,
) -> Option<&'a str> {
    let mut budget = WorkBudget::new(limits);
    let mut distance = |from, to| token_distance(from, to, &mut budget).ok();
    let to_local = distance(ancestor, local)?;
    let to_remote = distance(ancestor, remote)?;
    let between = distance(local, remote)?;
    if to_local + between == to_remote {
        Some(remote)
    } else if to_remote + between == to_local {
        Some(local)
    } else {
        None
    }
}

/// Insertions plus deletions in a shortest token edit script from `from` to
/// `to`, by Myers' greedy algorithm: the work grows with the length times the
/// distance, not with the product of the lengths, so a long text with a few
/// edits stays cheap.
fn token_distance(from: &str, to: &str, budget: &mut WorkBudget) -> Result<PieceIndex, MergeError> {
    let from = Pieces::new(from, Level::Tokens, budget.piece_limit)?;
    let to = Pieces::new(to, Level::Tokens, budget.piece_limit)?;
    let longest = from.len() + to.len();
    // `furthest[longest + k]`: the furthest `from` index that a script with
    // the current number of edits reaches on diagonal k (the `from` index
    // minus the `to` index). Points past either end can be recorded, but
    // clamping them to the ends gives a script no longer, so the first
    // distance to reach both ends is still the shortest.
    let mut furthest: Vec<PieceIndex> = vec![0; 2 * longest + 2];
    for distance in 0..=longest {
        budget.reserve_alignment(distance + 1)?;
        for diagonal in (longest - distance..=longest + distance).step_by(2) {
            let after_insertion = diagonal == longest - distance
                || (diagonal != longest + distance
                    && furthest[diagonal - 1] < furthest[diagonal + 1]);
            let mut from_index = if after_insertion {
                furthest[diagonal + 1]
            } else {
                furthest[diagonal - 1] + 1
            };
            let mut to_index = from_index + longest - diagonal;
            while from_index < from.len()
                && to_index < to.len()
                && budget.equal(from.piece(from_index), to.piece(to_index))?
            {
                from_index += 1;
                to_index += 1;
            }
            furthest[diagonal] = from_index;
            if from_index >= from.len() && to_index >= to.len() {
                return Ok(distance);
            }
        }
    }
    unreachable!("deleting every piece and inserting every other reaches both ends")
}

fn merge_aligned(
    ancestor: &str,
    first: &str,
    second: &str,
    alignment: Alignment,
    limits: ProseMergeLimits,
) -> Result<String, MergeError> {
    let mut budget = WorkBudget::new(limits);
    let output_capacity = first
        .len()
        .checked_add(second.len())
        .ok_or(MergeError::Exhausted(WorkExhaustionReason::InputBytes))?;
    let mut output = String::with_capacity(output_capacity);
    merge_level(
        ancestor,
        first,
        second,
        Level::Paragraphs,
        alignment,
        &mut budget,
        &mut output,
    )?;
    Ok(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MergeError {
    Competing(CompetingEditReason),
    Structured(StructuredOverlapReason),
    Exhausted(WorkExhaustionReason),
}

struct WorkBudget {
    alignment_work: WorkCount,
    comparison_bytes: ByteCount,
    piece_limit: PieceLimit,
}

impl WorkBudget {
    fn new(limits: ProseMergeLimits) -> Self {
        Self {
            alignment_work: limits.max_alignment_work.0,
            comparison_bytes: limits.max_comparison_bytes.0,
            piece_limit: limits.max_pieces_per_input,
        }
    }

    fn reserve_alignment(&mut self, work: WorkCount) -> Result<(), MergeError> {
        self.alignment_work = self
            .alignment_work
            .checked_sub(work)
            .ok_or(MergeError::Exhausted(WorkExhaustionReason::AlignmentWork))?;
        Ok(())
    }

    fn reserve_comparison(&mut self, bytes: ByteCount) -> Result<(), MergeError> {
        self.comparison_bytes = self
            .comparison_bytes
            .checked_sub(bytes)
            .ok_or(MergeError::Exhausted(WorkExhaustionReason::ComparisonBytes))?;
        Ok(())
    }

    fn equal(&mut self, left: &str, right: &str) -> Result<bool, MergeError> {
        if left.len() != right.len() {
            return Ok(false);
        }
        self.reserve_comparison(left.len())?;
        Ok(left == right)
    }

    fn compare(&mut self, left: &str, right: &str) -> Result<Ordering, MergeError> {
        self.reserve_comparison(left.len().min(right.len()))?;
        Ok(left.as_bytes().cmp(right.as_bytes()))
    }

    fn strip_prefix<'a>(
        &mut self,
        source: &'a str,
        prefix: &str,
    ) -> Result<Option<&'a str>, MergeError> {
        if prefix.len() > source.len() {
            return Ok(None);
        }
        self.reserve_comparison(prefix.len())?;
        Ok(source.strip_prefix(prefix))
    }

    fn strip_suffix<'a>(
        &mut self,
        source: &'a str,
        suffix: &str,
    ) -> Result<Option<&'a str>, MergeError> {
        if suffix.len() > source.len() {
            return Ok(None);
        }
        self.reserve_comparison(suffix.len())?;
        Ok(source.strip_suffix(suffix))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Level {
    Paragraphs,
    Tokens,
}

struct Pieces<'a> {
    source: &'a str,
    level: Level,
    boundaries: Vec<ByteOffset>,
}

impl<'a> Pieces<'a> {
    fn new(source: &'a str, level: Level, piece_limit: PieceLimit) -> Result<Self, MergeError> {
        let mut pieces = Self {
            source,
            level,
            boundaries: vec![0],
        };
        match level {
            Level::Paragraphs => pieces.find_paragraph_boundaries(piece_limit)?,
            Level::Tokens => pieces.find_token_boundaries(piece_limit)?,
        }
        pieces.push_boundary(source.len(), piece_limit)?;
        Ok(pieces)
    }

    fn find_paragraph_boundaries(&mut self, piece_limit: PieceLimit) -> Result<(), MergeError> {
        let mut offset = 0;
        let mut previous_blank = false;
        let mut fence = None;
        for line in self.source.split_inclusive('\n') {
            let blank = fence.is_none() && line.trim().is_empty();
            if !blank && previous_blank {
                self.push_boundary(offset, piece_limit)?;
            }
            update_fence(line, &mut fence);
            offset += line.len();
            previous_blank = blank;
        }
        Ok(())
    }

    fn find_token_boundaries(&mut self, piece_limit: PieceLimit) -> Result<(), MergeError> {
        let mut previous = None;
        for (offset, character) in self.source.char_indices() {
            let class = TokenClass::of(self.source, offset, character);
            if previous.is_some_and(|prior| prior != class) || class == TokenClass::Punctuation {
                self.push_boundary(offset, piece_limit)?;
            }
            previous = Some(class);
        }
        Ok(())
    }

    fn push_boundary(
        &mut self,
        offset: ByteOffset,
        piece_limit: PieceLimit,
    ) -> Result<(), MergeError> {
        if self.boundaries.last() == Some(&offset) {
            return Ok(());
        }
        if self.boundaries.len() > piece_limit.0 {
            return Err(MergeError::Exhausted(WorkExhaustionReason::Pieces));
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

    fn matching_piece(&self, index: PieceIndex) -> &'a str {
        match self.level {
            Level::Paragraphs => paragraph_content(self.piece(index)),
            Level::Tokens => self.piece(index),
        }
    }

    /// The byte range of a piece within `source`.
    fn span(&self, index: PieceIndex) -> Range<ByteOffset> {
        self.boundaries[index]..self.boundaries[index + 1]
    }
}

/// The bytes of two matched pieces that the match covers, as ranges of their
/// sources. Tokens match whole. Matched paragraphs share their content, and
/// the match extends over the blank lines around it only as far as the two
/// pieces agree, so that a change to those blank lines alone, such as the
/// separator a final paragraph gains when text is appended after it, remains
/// an edit of just those bytes rather than of the whole paragraph.
fn matched_spans(
    ancestor: &Pieces<'_>,
    side: &Pieces<'_>,
    matched: MatchedPieces,
) -> (Range<ByteOffset>, Range<ByteOffset>) {
    let ancestor_span = ancestor.span(matched.ancestor);
    let side_span = side.span(matched.side);
    if ancestor.level == Level::Tokens {
        return (ancestor_span, side_span);
    }
    let ancestor_piece = ancestor.piece(matched.ancestor);
    let side_piece = side.piece(matched.side);
    let ancestor_content = paragraph_content_span(ancestor_piece);
    let side_content = paragraph_content_span(side_piece);
    let shared_lead = common_suffix_length(
        &ancestor_piece[..ancestor_content.start],
        &side_piece[..side_content.start],
    );
    let shared_trail = common_prefix_length(
        &ancestor_piece[ancestor_content.end..],
        &side_piece[side_content.end..],
    );
    let covered = |span_start: ByteOffset, content: Range<ByteOffset>| {
        span_start + content.start - shared_lead..span_start + content.end + shared_trail
    };
    (
        covered(ancestor_span.start, ancestor_content),
        covered(side_span.start, side_content),
    )
}

fn common_prefix_length(first: &str, second: &str) -> ByteOffset {
    first
        .char_indices()
        .zip(second.chars())
        .find(|((_, left), right)| left != right)
        .map_or(first.len().min(second.len()), |((offset, _), _)| offset)
}

fn common_suffix_length(first: &str, second: &str) -> ByteOffset {
    first
        .chars()
        .rev()
        .zip(second.chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum()
}

fn paragraph_content(piece: &str) -> &str {
    &piece[paragraph_content_span(piece)]
}

/// The bytes of `piece` from its first to its last non-blank character,
/// without the blank lines and line endings around them.
fn paragraph_content_span(piece: &str) -> Range<ByteOffset> {
    let mut first_content = None;
    let mut content_end = 0;
    let mut offset = 0;
    for line in piece.split_inclusive('\n') {
        let line_without_ending = line.trim_end_matches(['\r', '\n']);
        if !line_without_ending.trim().is_empty() {
            first_content.get_or_insert(offset);
            content_end = offset + line_without_ending.len();
        }
        offset += line.len();
    }
    first_content.unwrap_or(content_end)..content_end
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenClass {
    Word,
    Whitespace,
    Punctuation,
}

impl TokenClass {
    fn of(source: &str, offset: ByteOffset, character: char) -> Self {
        if character.is_whitespace() {
            return Self::Whitespace;
        }
        if character.is_alphanumeric()
            || (!character.is_ascii() && !character.is_control())
            || is_internal_word_mark(source, offset, character)
        {
            Self::Word
        } else {
            Self::Punctuation
        }
    }
}

fn is_internal_word_mark(source: &str, offset: ByteOffset, character: char) -> bool {
    if !matches!(character, '-' | '\'' | '’') {
        return false;
    }
    let previous = source[..offset].chars().next_back();
    let next = source[offset + character.len_utf8()..].chars().next();
    previous.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric)
}

#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    length: usize,
}

fn update_fence(line: &str, fence: &mut Option<Fence>) {
    let Some((marker, length, remainder)) = fence_marker(line) else {
        return;
    };
    match *fence {
        Some(open)
            if marker == open.marker && length >= open.length && remainder.trim().is_empty() =>
        {
            *fence = None;
        }
        None => *fence = Some(Fence { marker, length }),
        Some(_) => {}
    }
}

fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return None;
    }
    let body = &line[indentation..];
    let marker = body.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = body
        .chars()
        .take_while(|candidate| *candidate == marker)
        .count();
    if length < 3 {
        return None;
    }
    Some((marker, length, &body[length..]))
}

#[derive(Debug)]
struct Edit<'a> {
    ancestor: Range<ByteOffset>,
    replacement: &'a str,
}

#[derive(Clone, Copy)]
struct MatchedPieces {
    ancestor: PieceIndex,
    side: PieceIndex,
}

/// Which longest common subsequence `diff` reports when several exist: the one
/// matching every piece as early as possible, or as late as possible. Every
/// other LCS lies between the two, so a merge that comes out the same under
/// both does not depend on how repeated text happens to be aligned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Alignment {
    Earliest,
    Latest,
}

impl Alignment {
    const BOTH: [Self; 2] = [Self::Earliest, Self::Latest];

    /// Map an index in scan order to a piece index: `Latest` scans backwards.
    fn piece_index(self, scan_index: PieceIndex, length: PieceIndex) -> PieceIndex {
        match self {
            Self::Earliest => scan_index,
            Self::Latest => length - 1 - scan_index,
        }
    }
}

fn diff<'a>(
    ancestor: &Pieces<'_>,
    side: &Pieces<'a>,
    alignment: Alignment,
    budget: &mut WorkBudget,
) -> Result<Vec<Edit<'a>>, MergeError> {
    let matches = longest_common_pieces(ancestor, side, alignment, budget)?;
    let mut edits = Vec::new();
    let mut ancestor_start = 0;
    let mut side_start = 0;
    for matched in matches {
        let (ancestor_match, side_match) = matched_spans(ancestor, side, matched);
        push_edit(
            &mut edits,
            ancestor.source,
            side.source,
            ancestor_start..ancestor_match.start,
            side_start..side_match.start,
            budget,
        )?;
        ancestor_start = ancestor_match.end;
        side_start = side_match.end;
    }
    push_edit(
        &mut edits,
        ancestor.source,
        side.source,
        ancestor_start..ancestor.source.len(),
        side_start..side.source.len(),
        budget,
    )?;
    Ok(edits)
}

/// The matched piece pairs of one longest common subsequence, in increasing
/// order. `alignment` picks which LCS when there are several; a choice between
/// repeated content pieces at the same step is refused outright.
fn longest_common_pieces(
    ancestor: &Pieces<'_>,
    side: &Pieces<'_>,
    alignment: Alignment,
    budget: &mut WorkBudget,
) -> Result<Vec<MatchedPieces>, MergeError> {
    let (ancestor_length, side_length) = (ancestor.len(), side.len());
    let ancestor_piece =
        |scan_index| ancestor.matching_piece(alignment.piece_index(scan_index, ancestor_length));
    let side_piece =
        |scan_index| side.matching_piece(alignment.piece_index(scan_index, side_length));
    let columns = side_length + 1;
    let cells = (ancestor_length + 1)
        .checked_mul(columns)
        .ok_or(MergeError::Exhausted(WorkExhaustionReason::AlignmentWork))?;
    budget.reserve_alignment(cells)?;
    let mut lengths: Vec<MatchCount> = vec![0; cells];
    for ancestor_index in (0..ancestor_length).rev() {
        for side_index in (0..side_length).rev() {
            lengths[ancestor_index * columns + side_index] =
                if budget.equal(ancestor_piece(ancestor_index), side_piece(side_index))? {
                    lengths[(ancestor_index + 1) * columns + side_index + 1] + 1
                } else {
                    lengths[(ancestor_index + 1) * columns + side_index]
                        .max(lengths[ancestor_index * columns + side_index + 1])
                };
        }
    }

    let mut matches = Vec::with_capacity(lengths[0]);
    let mut ancestor_start = 0;
    let mut side_start = 0;
    let mut remaining = lengths[0];
    while remaining > 0 {
        let mut next_match: Option<MatchedPieces> = None;
        for ancestor_index in ancestor_start..ancestor_length {
            if lengths[ancestor_index * columns + side_start] < remaining {
                break;
            }
            for side_index in side_start..side_length {
                budget.reserve_alignment(1)?;
                if lengths[ancestor_index * columns + side_index] < remaining {
                    break;
                }
                if lengths[(ancestor_index + 1) * columns + side_index + 1] + 1 == remaining
                    && budget.equal(ancestor_piece(ancestor_index), side_piece(side_index))?
                {
                    if let Some(prior_match) = next_match {
                        let content_anchor = is_content_anchor(ancestor_piece(ancestor_index));
                        let repeated_ancestor = budget.equal(
                            ancestor_piece(prior_match.ancestor),
                            ancestor_piece(ancestor_index),
                        )?;
                        let repeated_side =
                            budget.equal(side_piece(prior_match.side), side_piece(side_index))?;
                        if (repeated_ancestor || repeated_side) && content_anchor {
                            return Err(MergeError::Competing(
                                CompetingEditReason::AmbiguousRepeatedAlignment,
                            ));
                        }
                        if content_anchor {
                            return Err(MergeError::Competing(
                                CompetingEditReason::OverlappingEdits,
                            ));
                        }
                    }
                    if next_match.is_none() {
                        next_match = Some(MatchedPieces {
                            ancestor: ancestor_index,
                            side: side_index,
                        });
                    }
                }
            }
        }
        let matched = next_match.ok_or(MergeError::Competing(
            CompetingEditReason::AmbiguousRepeatedAlignment,
        ))?;
        matches.push(MatchedPieces {
            ancestor: alignment.piece_index(matched.ancestor, ancestor_length),
            side: alignment.piece_index(matched.side, side_length),
        });
        ancestor_start = matched.ancestor + 1;
        side_start = matched.side + 1;
        remaining -= 1;
    }
    if alignment == Alignment::Latest {
        matches.reverse();
    }
    Ok(matches)
}

fn is_content_anchor(piece: &str) -> bool {
    piece.chars().any(|character| {
        character.is_alphanumeric() || (!character.is_ascii() && !character.is_whitespace())
    })
}

/// Record the replacement of `ancestor[ancestor_range]` by
/// `side[side_range]`, unless the two are the same text.
fn push_edit<'a>(
    edits: &mut Vec<Edit<'a>>,
    ancestor: &str,
    side: &'a str,
    ancestor_range: Range<ByteOffset>,
    side_range: Range<ByteOffset>,
    budget: &mut WorkBudget,
) -> Result<(), MergeError> {
    let replacement = &side[side_range];
    if !budget.equal(&ancestor[ancestor_range.clone()], replacement)? {
        edits.push(Edit {
            ancestor: ancestor_range,
            replacement,
        });
    }
    Ok(())
}

fn edits_overlap(left: &Range<ByteOffset>, right: &Range<ByteOffset>) -> bool {
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
    alignment: Alignment,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), MergeError> {
    let ancestor_pieces = Pieces::new(ancestor, level, budget.piece_limit)?;
    let local_pieces = Pieces::new(local, level, budget.piece_limit)?;
    let remote_pieces = Pieces::new(remote, level, budget.piece_limit)?;
    let local_edits = diff(&ancestor_pieces, &local_pieces, alignment, budget)?;
    let remote_edits = diff(&ancestor_pieces, &remote_pieces, alignment, budget)?;
    let mut local_index = 0;
    let mut remote_index = 0;
    let mut cursor = 0;

    while local_index < local_edits.len() || remote_index < remote_edits.len() {
        let local_next = local_edits.get(local_index);
        let remote_next = remote_edits.get(remote_index);
        if let (Some(local_edit), Some(remote_edit)) = (local_next, remote_next) {
            if edits_overlap(&local_edit.ancestor, &remote_edit.ancestor) {
                let mut region = local_edit.ancestor.start.min(remote_edit.ancestor.start)
                    ..local_edit.ancestor.end.max(remote_edit.ancestor.end);
                let local_start = local_index;
                let remote_start = remote_index;
                local_index += 1;
                remote_index += 1;
                loop {
                    let prior = (local_index, remote_index);
                    while let Some(edit) = local_edits.get(local_index) {
                        if !edits_overlap(&region, &edit.ancestor) {
                            break;
                        }
                        region.end = region.end.max(edit.ancestor.end);
                        local_index += 1;
                    }
                    while let Some(edit) = remote_edits.get(remote_index) {
                        if !edits_overlap(&region, &edit.ancestor) {
                            break;
                        }
                        region.end = region.end.max(edit.ancestor.end);
                        remote_index += 1;
                    }
                    if prior == (local_index, remote_index) {
                        break;
                    }
                }

                output.push_str(&ancestor[cursor..region.start]);
                let local_region =
                    render_region(ancestor, &local_edits[local_start..local_index], &region);
                let remote_region =
                    render_region(ancestor, &remote_edits[remote_start..remote_index], &region);
                merge_overlap(
                    ancestor,
                    &region,
                    &local_region,
                    &remote_region,
                    level,
                    alignment,
                    budget,
                    output,
                )?;
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

#[allow(clippy::too_many_arguments)]
fn merge_overlap(
    ancestor: &str,
    region: &Range<ByteOffset>,
    local: &str,
    remote: &str,
    level: Level,
    alignment: Alignment,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), MergeError> {
    if budget.equal(local, remote)? {
        output.push_str(local);
        return Ok(());
    }
    let original = &ancestor[region.start..region.end];
    if local.is_empty() || remote.is_empty() {
        return Err(MergeError::Competing(CompetingEditReason::DeleteVsEdit));
    }

    if level == Level::Paragraphs {
        if let Some(additions) = paragraph_additions(ancestor, region, local, remote, budget)? {
            if contains_structured_markdown(additions.local)
                || contains_structured_markdown(additions.remote)
            {
                return Err(MergeError::Structured(
                    StructuredOverlapReason::MarkdownSource,
                ));
            }
            output.push_str(additions.before);
            merge_addition_groups(additions.local, additions.remote, budget, output)?;
            output.push_str(additions.after);
            return Ok(());
        }

        if contains_structured_markdown(original)
            || contains_structured_markdown(local)
            || contains_structured_markdown(remote)
        {
            return Err(MergeError::Structured(
                StructuredOverlapReason::MarkdownSource,
            ));
        }
        let start = output.len();
        merge_level(
            original,
            local,
            remote,
            Level::Tokens,
            alignment,
            budget,
            output,
        )?;
        return require_word_counts_within_sides(local, remote, &output[start..], budget);
    }

    Err(MergeError::Competing(CompetingEditReason::OverlappingEdits))
}

/// Each side's token diff against the ancestor is computed independently, so
/// when text repeats the two alignments can disagree: both sides' copies of a
/// shared insertion land at different, non-overlapping positions and are both
/// emitted, or both sides' deletions of one repeated word remove different
/// copies. A sound merge of `local` and `remote` never holds a word more often
/// than both sides or less often than both, so anything else is refused. The
/// bound is necessary, not sufficient: it cannot see a word moved or a word
/// that both sides repeat equally often, which the agreement of the earliest
/// and latest alignments covers. Only content words are counted; whitespace
/// and punctuation are not.
fn require_word_counts_within_sides(
    local: &str,
    remote: &str,
    merged: &str,
    budget: &WorkBudget,
) -> Result<(), MergeError> {
    // Occurrences of each word in `[local, remote, merged]`.
    let mut counts: HashMap<&str, [OccurrenceCount; 3]> = HashMap::new();
    for (slot, text) in [local, remote, merged].into_iter().enumerate() {
        let pieces = Pieces::new(text, Level::Tokens, budget.piece_limit)?;
        for index in 0..pieces.len() {
            let word = pieces.piece(index);
            if is_content_anchor(word) {
                counts.entry(word).or_default()[slot] += 1;
            }
        }
    }
    if counts
        .values()
        .all(|[local, remote, merged]| local.min(remote) <= merged && merged <= local.max(remote))
    {
        Ok(())
    } else {
        Err(MergeError::Competing(
            CompetingEditReason::InconsistentWordCounts,
        ))
    }
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
    budget: &mut WorkBudget,
) -> Result<Option<ParagraphAdditions<'a>>, MergeError> {
    let original = &ancestor[region.start..region.end];
    if let (Some(local_addition), Some(remote_addition)) = (
        budget.strip_prefix(local, original)?,
        budget.strip_prefix(remote, original)?,
    ) {
        if standalone_addition(ancestor, region.end, local_addition)
            && standalone_addition(ancestor, region.end, remote_addition)
        {
            return Ok(Some(ParagraphAdditions {
                before: original,
                local: local_addition,
                remote: remote_addition,
                after: "",
            }));
        }
    }
    if let (Some(local_addition), Some(remote_addition)) = (
        budget.strip_suffix(local, original)?,
        budget.strip_suffix(remote, original)?,
    ) {
        if standalone_addition(ancestor, region.start, local_addition)
            && standalone_addition(ancestor, region.start, remote_addition)
        {
            return Ok(Some(ParagraphAdditions {
                before: "",
                local: local_addition,
                remote: remote_addition,
                after: original,
            }));
        }
    }
    Ok(None)
}

fn merge_addition_groups(
    local: &str,
    remote: &str,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), MergeError> {
    if budget.equal(local, remote)? {
        output.push_str(local);
        return Ok(());
    }
    let local_pieces = Pieces::new(local, Level::Paragraphs, budget.piece_limit)?;
    let remote_pieces = Pieces::new(remote, Level::Paragraphs, budget.piece_limit)?;
    require_unique_group(&local_pieces, budget)?;
    require_unique_group(&remote_pieces, budget)?;
    if insertion_group_contains(remote, local, budget)? {
        output.push_str(remote);
        return Ok(());
    }
    if insertion_group_contains(local, remote, budget)? {
        output.push_str(local);
        return Ok(());
    }
    require_disjoint_groups(&local_pieces, &remote_pieces, budget)?;
    let (first, second) = match budget.compare(local, remote)? {
        Ordering::Less => (local, remote),
        Ordering::Equal => unreachable!(),
        Ordering::Greater => (remote, local),
    };
    let line_ending = preferred_line_ending(first, second);
    output.push_str(first);
    if !has_paragraph_break(trailing_whitespace(output), leading_whitespace(second)) {
        if !trailing_whitespace(output).contains('\n') && !leading_whitespace(second).contains('\n')
        {
            output.push_str(line_ending);
        }
        output.push_str(line_ending);
    }
    output.push_str(second);
    Ok(())
}

fn insertion_group_contains(
    candidate: &str,
    subset: &str,
    budget: &mut WorkBudget,
) -> Result<bool, MergeError> {
    if let Some(remainder) = budget.strip_prefix(candidate, subset)? {
        if !remainder.is_empty()
            && has_paragraph_break(trailing_whitespace(subset), leading_whitespace(remainder))
        {
            return Ok(true);
        }
    }
    if let Some(remainder) = budget.strip_suffix(candidate, subset)? {
        return Ok(!remainder.is_empty()
            && has_paragraph_break(trailing_whitespace(remainder), leading_whitespace(subset)));
    }
    Ok(false)
}

fn require_disjoint_groups(
    local_pieces: &Pieces<'_>,
    remote_pieces: &Pieces<'_>,
    budget: &mut WorkBudget,
) -> Result<(), MergeError> {
    for local_index in 0..local_pieces.len() {
        let local_content = local_pieces.matching_piece(local_index);
        if local_content.is_empty() {
            continue;
        }
        for remote_index in 0..remote_pieces.len() {
            budget.reserve_alignment(1)?;
            let remote_content = remote_pieces.matching_piece(remote_index);
            if budget.equal(local_content, remote_content)? {
                return Err(MergeError::Competing(
                    CompetingEditReason::AmbiguousInsertionGroup,
                ));
            }
        }
    }
    Ok(())
}

fn require_unique_group(pieces: &Pieces<'_>, budget: &mut WorkBudget) -> Result<(), MergeError> {
    for index in 0..pieces.len() {
        let content = pieces.matching_piece(index);
        if content.is_empty() {
            continue;
        }
        for earlier in 0..index {
            budget.reserve_alignment(1)?;
            if budget.equal(content, pieces.matching_piece(earlier))? {
                return Err(MergeError::Competing(
                    CompetingEditReason::AmbiguousInsertionGroup,
                ));
            }
        }
    }
    Ok(())
}

fn preferred_line_ending(first: &str, second: &str) -> &'static str {
    for source in [first, second] {
        if let Some(index) = source.find('\n') {
            return if index > 0 && source.as_bytes()[index - 1] == b'\r' {
                "\r\n"
            } else {
                "\n"
            };
        }
    }
    "\n"
}

fn contains_structured_markdown(source: &str) -> bool {
    let mut fence = None;
    for line in source.lines() {
        if fence_marker(line).is_some() {
            return true;
        }
        if fence.is_some() {
            return true;
        }
        update_fence(line, &mut fence);
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if has_indented_code_prefix(line)
            || line.contains('|')
            || line.contains('`')
            || line.contains('*')
            || line.contains('_')
            || line.contains("~~")
            || line.contains("](")
            || (line.contains('[') && line.contains(']'))
            || (line.contains('<') && line.contains('>'))
            || line.ends_with("  ")
            || is_setext_underline(trimmed)
            || starts_structured_line(trimmed)
        {
            return true;
        }
    }
    false
}

fn has_indented_code_prefix(line: &str) -> bool {
    let mut columns = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - (columns % 4),
            _ => break,
        }
        if columns >= 4 {
            return true;
        }
    }
    false
}

fn is_setext_underline(line: &str) -> bool {
    let marker = line.as_bytes().first().copied();
    matches!(marker, Some(b'=' | b'-'))
        && line
            .bytes()
            .all(|byte| Some(byte) == marker || matches!(byte, b' ' | b'\t'))
}

fn starts_structured_line(line: &str) -> bool {
    if line.starts_with('#')
        || line.starts_with('>')
        || line.starts_with('<')
        || line.starts_with("![")
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || line == "---"
        || line == "***"
        || line == "___"
    {
        return true;
    }
    if line.starts_with('[') && line.contains("]:") {
        return true;
    }
    let digit_count = line.bytes().take_while(u8::is_ascii_digit).count();
    digit_count > 0
        && line
            .get(digit_count..)
            .is_some_and(|remainder| remainder.starts_with(". ") || remainder.starts_with(") "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(ancestor: &'a str, local: &'a str, remote: &'a str) -> ProseMergeInputs<'a> {
        ProseMergeInputs::new(ancestor, local, remote)
    }

    fn merge<'a>(ancestor: &'a str, local: &'a str, remote: &'a str) -> ProseMergeResult<'a> {
        merge_prose(ancestor, local, remote, ProseMergeLimits::default())
    }

    fn merged_text<'a>(result: &'a ProseMergeResult<'_>) -> &'a str {
        match result {
            ProseMergeResult::Merged(text) => text.as_str(),
            unexpected => panic!("expected merged text, got {unexpected:?}"),
        }
    }

    fn assert_merge(ancestor: &str, local: &str, remote: &str, expected: &str) {
        let result = merge(ancestor, local, remote);
        assert_eq!(merged_text(&result), expected);
        let swapped = merge(ancestor, remote, local);
        assert_eq!(merged_text(&swapped), expected);
        let settled = merge(expected, expected, expected);
        assert_eq!(merged_text(&settled), expected);
    }

    fn competing_reason(result: ProseMergeResult<'_>) -> CompetingEditReason {
        match result {
            ProseMergeResult::CompetingEdit(failure) => failure.reason(),
            unexpected => panic!("expected competing edit, got {unexpected:?}"),
        }
    }

    fn assert_competing_in_both_roles(
        ancestor: &str,
        local: &str,
        remote: &str,
        expected: CompetingEditReason,
    ) {
        assert_eq!(competing_reason(merge(ancestor, local, remote)), expected);
        assert_eq!(competing_reason(merge(ancestor, remote, local)), expected);
    }

    fn structured_reason(result: ProseMergeResult<'_>) -> StructuredOverlapReason {
        match result {
            ProseMergeResult::UnsupportedStructuredOverlap(failure) => failure.reason(),
            unexpected => panic!("expected structured overlap, got {unexpected:?}"),
        }
    }

    fn exhaustion_reason(result: ProseMergeResult<'_>) -> WorkExhaustionReason {
        match result {
            ProseMergeResult::WorkExhausted(failure) => failure.reason(),
            unexpected => panic!("expected work exhaustion, got {unexpected:?}"),
        }
    }

    #[test]
    fn word_merges_never_duplicate_a_shared_insertion() {
        // Each case: the local side already carries the remote's insertion,
        // whose words repeat nearby text, plus one edit of its own. The two
        // independent token alignments placed the insertion at different
        // positions and emitted it twice. The first case spans paragraphs and
        // was found by the issue-merge re-merge property; no shortest token
        // edit path runs through the remote, and its alignment is ambiguous.
        assert_competing_in_both_roles(
            "P0 Golf hotel india.\n\nP1 Mike.\n",
            "Golf hotel india. (ours)\n\nAdded: Golf hotel india.\n\nP1 Mike.\n",
            "P0 Golf hotel india.\n\nAdded: Golf hotel india.\n\nP1 Mike.\n",
            CompetingEditReason::AmbiguousRepeatedAlignment,
        );
        // These came from adversarial review. The remote lies on a shortest
        // token edit path to the local side, so the merge is the local side
        // as it stands, with the insertion once.
        for (ancestor, local, remote) in [
            (
                "bug fix a the and should fix the",
                "really fix test bug ship a the and should fix the",
                "bug fix test bug ship a the and should fix the",
            ),
            (
                "We should fix the bug in the parser and then ship the release.",
                "We should fix the really in bug fix the parser and then ship the release.",
                "We should fix the bug in bug fix the parser and then ship the release.",
            ),
        ] {
            assert_merge(ancestor, local, remote, local);
        }
    }

    #[test]
    fn word_merges_never_drop_both_copies_of_one_deleted_word() {
        // Both sides delete one "fix" but align on different copies, so a
        // naive merge deletes both.
        assert_competing_in_both_roles(
            "fix one fix two",
            "one fix two more",
            "fix one two",
            CompetingEditReason::InconsistentWordCounts,
        );
    }

    #[test]
    fn each_alignment_alone_corrupts_some_merge() {
        // Found by the generated properties with the merge run under one
        // alignment only: earliest alone merges the first three cases and
        // latest alone the last two, each dropping a word of one side. The
        // two alignments disagree, or one of them fails, on every case.
        for (ancestor, local, remote, reason) in [
            (
                "bug a bug bug bug bug\n\nbug ship fix the the the\n\na test bug\n\nfix bug bug a fix ship\n",
                "bug a bug\n\na test bug bug bug ship a bug ship fix the test the\n\na test bug\n\nship test bug bug a ship\n",
                "bug a bug bug bug bug\n\nbug ship fix the the the\n\na bug\n\ntest bug\n\nfix test bug bug a fix ship\n",
                CompetingEditReason::OverlappingEdits,
            ),
            (
                "a bug the a ship ship\n\na bug the a ship ship\n\nthe bug\n\nship test fix bug a fix the\n",
                "a bug the a the fix ship\n\na bug a ship ship\n\nthe bug\n\nship test fix bug a fix the\n",
                "a bug test\n\nthe a ship fix ship a fix\n\nbug the a ship ship\n\nthe bug\n\nship ship fix bug a fix the\n",
                CompetingEditReason::OverlappingEdits,
            ),
            // From adversarial review: earliest alone gives "... ship ship a".
            (
                "the test the bug fix a ship the the the a",
                "the test the bug fix a the the the the ship a",
                "the test the bug fix a ship the the ship a",
                CompetingEditReason::OverlappingEdits,
            ),
            (
                "ship test test the",
                "ship a fix\n\ntest bug the",
                "ship test test bug bug",
                CompetingEditReason::InconsistentWordCounts,
            ),
            (
                "bug a bug\n\nthe a test test\n\nfix bug the fix",
                "bug a bug\n\nbug\n\nthe a the test\n\nfix bug the fix",
                "bug a bug\n\nthe fix the test the\n\nfix bug the fix",
                CompetingEditReason::OverlappingEdits,
            ),
        ] {
            assert_competing_in_both_roles(ancestor, local, remote, reason);
        }
    }

    #[test]
    fn merges_that_carry_an_edit_to_repeated_text_elsewhere_are_refused() {
        // The paragraph alignment matches the local's moved copy of "bug ship
        // fix", so the remote's deletion of "ship" would land in the copy
        // and the remote's own paragraph would keep it. Found by the
        // generated disjoint property.
        assert_competing_in_both_roles(
            "bug ship fix\n\na ship fix\n\nbug ship fix",
            "ship fix\n\nbug ship fix\n\nbug ship fix",
            "bug fix\n\na ship fix\n\nbug ship bug",
            CompetingEditReason::MissesASideEdit,
        );
        // From adversarial review, where the remote's insertion was repeated.
        for (ancestor, local, remote) in [
            (
                "Alpha the.\n\nThe.",
                "Test fix.\n\nAlpha the.",
                "Alpha test fix.\n\nAlpha the.",
            ),
            ("a the\n\nthe", "test fix\n\na the", "a test fix\n\na the"),
        ] {
            assert_competing_in_both_roles(
                ancestor,
                local,
                remote,
                CompetingEditReason::MissesASideEdit,
            );
        }
        // Insertions both sides made at one point may land in either order:
        // the result is one side's change away from the other side.
        assert_merge(
            "the test the a test bug\n\ntest bug bug bug a fix bug",
            "the test the a test ship bug bug\n\ntest bug bug bug a fix bug",
            "the test the a test bug\n\ntest bug\n\ntest bug bug bug a bug",
            "the test the a test ship bug bug\n\ntest bug\n\ntest bug bug bug a bug",
        );
    }

    #[test]
    fn a_superseding_side_keeps_every_paragraph() {
        // From adversarial review, which saw the remote's kept paragraph
        // dropped. The remote lies on a shortest token edit path to the
        // local side, so the merge is the local side.
        for (ancestor, local, remote) in [
            (
                "Ship it today.\n\nShip it.",
                "Ship it.\n\nThe end.",
                "Ship it today.\n\nThe end.",
            ),
            ("ship fix\n\nship", "ship\n\nthe", "ship fix\n\nthe"),
        ] {
            assert_merge(ancestor, local, remote, local);
        }
    }

    /// Generated three-way cases. Text is a list of tokens drawn from a
    /// six-word vocabulary, so words repeat, and paragraphs are often copies
    /// or near copies of earlier ones. Changes may be adjacent. A case is
    /// kept only when every side's changes form a minimal edit script, so no
    /// two changes cancel out or merge into a smaller one and the generated
    /// changes are what the merger is shown.
    mod generated {
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};

        type Token = &'static str;
        type TokenIndex = usize;
        type EditCost = usize;

        const WORDS: [Token; 6] = ["bug", "fix", "the", "ship", "test", "a"];
        /// Renders as the blank line between two paragraphs.
        const PARAGRAPH: Token = "\n\n";

        #[derive(Debug)]
        enum Change {
            InsertBefore(Vec<Token>),
            Replace(Vec<Token>),
            Delete,
        }

        /// A change to the ancestor token at `at` (or an insertion at the end
        /// when `at` is the token count). At most one change per index.
        #[derive(Debug)]
        struct Placed {
            at: TokenIndex,
            change: Change,
        }

        impl Placed {
            /// Tokens inserted plus tokens deleted.
            fn cost(&self) -> EditCost {
                match &self.change {
                    Change::InsertBefore(tokens) => tokens.len(),
                    Change::Replace(tokens) => tokens.len() + 1,
                    Change::Delete => 1,
                }
            }
        }

        pub(super) struct Case {
            pub(super) ancestor: String,
            pub(super) local: String,
            pub(super) remote: String,
            /// The only acceptable clean merge.
            pub(super) expected: String,
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(super) enum Shape {
            /// The remote makes some of the local side's changes.
            Subsumed,
            /// The two sides make disjoint sets of changes.
            Disjoint,
        }

        fn apply<'c>(
            ancestor: &[Token],
            changes: impl Iterator<Item = &'c Placed> + Clone,
        ) -> Vec<Token> {
            let mut tokens = Vec::with_capacity(ancestor.len() + 4);
            for index in 0..=ancestor.len() {
                let mut keep = true;
                for placed in changes.clone().filter(|placed| placed.at == index) {
                    match &placed.change {
                        Change::InsertBefore(inserted) => tokens.extend_from_slice(inserted),
                        Change::Replace(inserted) => {
                            tokens.extend_from_slice(inserted);
                            keep = false;
                        }
                        Change::Delete => keep = false,
                    }
                }
                if keep {
                    tokens.extend(ancestor.get(index));
                }
            }
            tokens
        }

        fn cost<'c>(changes: impl Iterator<Item = &'c Placed>) -> EditCost {
            changes.map(Placed::cost).sum()
        }

        /// Insertions plus deletions of a shortest edit script.
        fn distance(from: &[Token], to: &[Token]) -> EditCost {
            let columns = to.len() + 1;
            let mut lengths = vec![0; (from.len() + 1) * columns];
            for (row, from_token) in from.iter().enumerate() {
                for (column, to_token) in to.iter().enumerate() {
                    lengths[(row + 1) * columns + column + 1] = if from_token == to_token {
                        lengths[row * columns + column] + 1
                    } else {
                        lengths[row * columns + column + 1]
                            .max(lengths[(row + 1) * columns + column])
                    };
                }
            }
            from.len() + to.len() - 2 * lengths[lengths.len() - 1]
        }

        /// `None` for token lists that do not render as plain paragraphs.
        fn render(tokens: &[Token], final_newline: bool) -> Option<String> {
            let empty_paragraph = tokens.first() == Some(&PARAGRAPH)
                || tokens.last() == Some(&PARAGRAPH)
                || tokens
                    .windows(2)
                    .any(|pair| pair[0] == PARAGRAPH && pair[1] == PARAGRAPH);
            if tokens.is_empty() || empty_paragraph {
                return None;
            }
            let mut text = String::new();
            for (index, token) in tokens.iter().enumerate() {
                if index > 0 && *token != PARAGRAPH && tokens[index - 1] != PARAGRAPH {
                    text.push(' ');
                }
                text.push_str(token);
            }
            if final_newline {
                text.push('\n');
            }
            Some(text)
        }

        fn words(rng: &mut StdRng, count: usize) -> Vec<Token> {
            (0..count)
                .map(|_| WORDS[rng.gen_range(0..WORDS.len())])
                .collect()
        }

        fn ancestor(rng: &mut StdRng) -> Vec<Token> {
            let mut paragraphs: Vec<Vec<Token>> = Vec::new();
            for _ in 0..rng.gen_range(1..=4) {
                let mut paragraph = if !paragraphs.is_empty() && rng.gen_bool(0.4) {
                    paragraphs[rng.gen_range(0..paragraphs.len())].clone()
                } else {
                    let count = rng.gen_range(2..=7);
                    words(rng, count)
                };
                if rng.gen_bool(0.3) {
                    let index = rng.gen_range(0..paragraph.len());
                    paragraph[index] = WORDS[rng.gen_range(0..WORDS.len())];
                }
                paragraphs.push(paragraph);
            }
            paragraphs.join(&PARAGRAPH)
        }

        fn change(rng: &mut StdRng) -> Change {
            let count = rng.gen_range(1..=2);
            let mut inserted = words(rng, count);
            match rng.gen_range(0..8) {
                0 => {
                    inserted.insert(0, PARAGRAPH);
                    Change::InsertBefore(inserted)
                }
                1 => {
                    inserted.push(PARAGRAPH);
                    Change::InsertBefore(inserted)
                }
                2 | 3 => Change::InsertBefore(inserted),
                4 | 5 => Change::Replace(inserted),
                _ => Change::Delete,
            }
        }

        pub(super) fn case(seed: u64, shape: Shape) -> Option<Case> {
            let mut rng = StdRng::seed_from_u64(seed);
            let ancestor = ancestor(&mut rng);
            let mut changes = Vec::new();
            for at in 0..=ancestor.len() {
                if rng.gen_bool(0.3) {
                    let change = change(&mut rng);
                    let at_end = at == ancestor.len();
                    if !at_end || matches!(change, Change::InsertBefore(_)) {
                        changes.push(Placed { at, change });
                    }
                }
            }
            if changes.len() < 2 {
                return None;
            }
            // Which changes the local side makes and which the remote makes.
            let sides: Vec<(bool, bool)> = match shape {
                Shape::Subsumed => {
                    let mut sides: Vec<(bool, bool)> =
                        changes.iter().map(|_| (true, rng.gen_bool(0.5))).collect();
                    let skipped = rng.gen_range(0..sides.len());
                    sides[skipped].1 = false;
                    sides
                }
                Shape::Disjoint => changes
                    .iter()
                    .map(|_| {
                        let local = rng.gen_bool(0.5);
                        (local, !local)
                    })
                    .collect(),
            };
            let local_changes = || {
                changes
                    .iter()
                    .zip(&sides)
                    .filter(|(_, side)| side.0)
                    .map(|(placed, _)| placed)
            };
            let remote_changes = || {
                changes
                    .iter()
                    .zip(&sides)
                    .filter(|(_, side)| side.1)
                    .map(|(placed, _)| placed)
            };
            let local = apply(&ancestor, local_changes());
            let remote = apply(&ancestor, remote_changes());
            let expected = apply(&ancestor, changes.iter());
            let local_cost = cost(local_changes());
            let remote_cost = cost(remote_changes());
            let minimal = remote_cost > 0
                && local != remote
                && distance(&ancestor, &local) == local_cost
                && distance(&ancestor, &remote) == remote_cost
                && distance(&ancestor, &expected) == cost(changes.iter())
                && distance(&local, &expected) == cost(changes.iter()) - local_cost
                && distance(&remote, &expected) == cost(changes.iter()) - remote_cost;
            if !minimal {
                return None;
            }
            let final_newline = rng.gen_bool(0.3);
            let case = Case {
                ancestor: render(&ancestor, final_newline)?,
                local: render(&local, final_newline)?,
                remote: render(&remote, final_newline)?,
                expected: render(&expected, final_newline)?,
            };
            // The merger's tokens also count the spaces around words, so a
            // script that is minimal here need not be minimal there; keep
            // only cases whose shape holds in the merger's own token space.
            let to_local = merger_distance(&case.ancestor, &case.local);
            let to_remote = merger_distance(&case.ancestor, &case.remote);
            let between = merger_distance(&case.local, &case.remote);
            let local_contains_remote = to_remote + between == to_local;
            let holds = match shape {
                Shape::Subsumed => local_contains_remote,
                Shape::Disjoint => {
                    let to_expected = merger_distance(&case.ancestor, &case.expected);
                    !local_contains_remote
                        && to_local + between != to_remote
                        && to_expected == to_local + to_remote
                        && merger_distance(&case.local, &case.expected) == to_remote
                        && merger_distance(&case.remote, &case.expected) == to_local
                }
            };
            holds.then_some(case)
        }

        impl Case {
            /// Whether `merged` is the expected merge with insertions that both
            /// sides made at one point in the other order. The generator's
            /// script puts each insertion in one gap, but where text repeats
            /// an equally short script of that side puts it in another, and
            /// the merger orders concurrent insertions canonically. `merged`
            /// must hold exactly the expected words and paragraphs and lie
            /// exactly one side's change from the other side, so no text can
            /// be dropped, repeated or invented.
            pub(super) fn reorders_expected_insertions(&self, merged: &str) -> bool {
                fn words(text: &str) -> (Vec<&str>, usize) {
                    let mut words: Vec<&str> = text.split_whitespace().collect();
                    words.sort_unstable();
                    (words, text.trim_end().matches("\n\n").count())
                }
                let to_local = merger_distance(&self.ancestor, &self.local);
                let to_remote = merger_distance(&self.ancestor, &self.remote);
                words(merged) == words(&self.expected)
                    && merger_distance(&self.local, merged) == to_remote
                    && merger_distance(&self.remote, merged) == to_local
                    && merger_distance(&self.ancestor, merged) == to_local + to_remote
            }
        }

        fn merger_distance(from: &str, to: &str) -> usize {
            let mut budget = super::WorkBudget::new(super::ProseMergeLimits::default());
            super::token_distance(from, to, &mut budget).expect("small inputs")
        }
    }

    /// Seeds per generated property; about a quarter yield a usable case.
    const GENERATED_SEEDS: u64 = 20_000;

    /// Runs `seeds` generated cases of one shape in both roles; every clean
    /// merge must be the expected text, up to the order of insertions both
    /// sides made at one point. Returns (clean, refused) counts.
    fn check_generated(shape: generated::Shape, seeds: std::ops::Range<u64>) -> (usize, usize) {
        let (mut clean, mut refused) = (0, 0);
        for seed in seeds {
            let Some(case) = generated::case(seed, shape) else {
                continue;
            };
            for (first, second) in [(&case.local, &case.remote), (&case.remote, &case.local)] {
                match merge(&case.ancestor, first, second) {
                    ProseMergeResult::Merged(text) => {
                        clean += 1;
                        assert!(
                            text.as_str() == case.expected
                                || case.reorders_expected_insertions(text.as_str()),
                            "{shape:?} seed {seed}: {:?} {first:?} {second:?} merged to {:?}, \
                             expected {:?}",
                            case.ancestor,
                            text.as_str(),
                            case.expected
                        );
                    }
                    _ => refused += 1,
                }
            }
        }
        (clean, refused)
    }

    /// One side makes some of the other side's changes, so the only correct
    /// clean merge is the other side as it stands. Generated in the merger's
    /// own token space, so the betweenness rule should take nearly all of
    /// these; the rest are delete-versus-edit refusals.
    #[test]
    fn property_merging_a_subsumed_side_returns_the_superset() {
        let (clean, refused) = check_generated(generated::Shape::Subsumed, 0..GENERATED_SEEDS);
        assert!(clean > 20 * refused, "{clean} clean, {refused} refused");
    }

    /// The two sides make disjoint sets of changes, so the only correct clean
    /// merge applies both. Changes are often adjacent and text repeats, so
    /// many of these are refused as ambiguous; a clean merge must be exact up
    /// to the order of insertions both sides made at one point.
    #[test]
    fn property_merging_disjoint_changes_applies_both() {
        let (clean, refused) = check_generated(generated::Shape::Disjoint, 0..GENERATED_SEEDS);
        assert!(5 * clean > refused, "{clean} clean, {refused} refused");
    }

    #[test]
    fn overlapping_paragraph_deletions_merge() {
        // Both sides delete P1; the local side also deletes P3. Refusing
        // every multi-paragraph overlap made this conflict.
        assert_merge(
            "P0 Papa quebec romeo.\n\nP1 Golf hotel india.\n\nP2 Mike november oscar.\n\nP3 Alpha bravo charlie.",
            "P0 Papa quebec romeo.\n\nP2 Mike november oscar.",
            "P0 Papa quebec romeo.\n\nP2 Mike november oscar.\n\nP3 Alpha bravo charlie.",
            "P0 Papa quebec romeo.\n\nP2 Mike november oscar.",
        );
    }

    #[test]
    fn a_final_paragraph_that_stops_being_final_still_aligns() {
        // Both sides delete P1 and local also appends after the last
        // paragraph, so its copy of P2 gains a separator. Paragraph diff3
        // merges this cleanly (found by the re-merge property).
        assert_merge(
            "P0 Juliet kilo lima.\n\nP1 Golf hotel india.\n\nP2 Alpha bravo charlie.",
            "P0 Juliet kilo lima.\n\nP2 Alpha bravo charlie.\n\nAdded: Mike november.",
            "P0 Juliet kilo lima.\n\nP2 Alpha bravo charlie.",
            "P0 Juliet kilo lima.\n\nP2 Alpha bravo charlie.\n\nAdded: Mike november.",
        );
        // Text that ends in a line break is left as it is.
        assert_merge(
            "P0 Juliet kilo lima.\n\nP1 Golf hotel india.\n",
            "P0 Juliet kilo lima.\n\nP1 Golf hotel india.\n\nAdded: Mike november.\n",
            "P1 Golf hotel india.\n",
            "P1 Golf hotel india.\n\nAdded: Mike november.\n",
        );
    }

    #[test]
    fn fast_paths_borrow_the_selected_input() {
        for (ancestor, local, remote, expected) in [
            ("same", "same", "same", "same"),
            ("old", "new", "new", "new"),
            ("old", "new", "old", "new"),
            ("old", "old", "new", "new"),
            ("old", "", "old", ""),
        ] {
            match merge(ancestor, local, remote) {
                ProseMergeResult::Merged(MergedProse(Cow::Borrowed(actual))) => {
                    assert_eq!(actual, expected);
                }
                unexpected => panic!("expected borrowed merge, got {unexpected:?}"),
            }
        }
    }

    #[test]
    fn owned_text_can_be_extracted_from_the_result() {
        let result = merge("red fox", "blue fox", "red cat");
        let ProseMergeResult::Merged(text) = result else {
            panic!("expected merged text");
        };
        assert_eq!(text.into_text(), "blue cat");
    }

    #[test]
    fn failures_retain_every_input() {
        let original = inputs("foo", "bar", "baz");
        let ProseMergeResult::CompetingEdit(failure) =
            merge_prose("foo", "bar", "baz", ProseMergeLimits::default())
        else {
            panic!("expected conflict");
        };
        assert_eq!(failure.inputs(), original);
        assert_eq!(failure.inputs().ancestor(), "foo");
        assert_eq!(failure.inputs().local(), "bar");
        assert_eq!(failure.inputs().remote(), "baz");
    }

    #[test]
    fn independent_paragraph_additions_keep_ancestor_order() {
        assert_merge(
            "First.\n\nMiddle.\n\nLast.",
            "First.\n\nLocal.\n\nMiddle.\n\nLast.",
            "First.\n\nMiddle.\n\nRemote.\n\nLast.",
            "First.\n\nLocal.\n\nMiddle.\n\nRemote.\n\nLast.",
        );
    }

    #[test]
    fn same_gap_groups_use_content_derived_order() {
        assert_merge(
            "First.\n\nLast.",
            "First.\n\nZulu.\n\nLast.",
            "First.\n\nAlpha.\n\nLast.",
            "First.\n\nAlpha.\n\nZulu.\n\nLast.",
        );
    }

    #[test]
    fn insertion_group_internal_order_is_preserved() {
        assert_merge(
            "End.",
            "Zulu one.\n\nZulu two.\n\nEnd.",
            "Alpha one.\n\nAlpha two.\n\nEnd.",
            "Alpha one.\n\nAlpha two.\n\nZulu one.\n\nZulu two.\n\nEnd.",
        );
    }

    #[test]
    fn identical_same_gap_groups_coalesce_exactly() {
        assert_merge(
            "Start.\n\nEnd.",
            "Start.\n\nShared.\n\nEnd.",
            "Start.\n\nShared.\n\nEnd.",
            "Start.\n\nShared.\n\nEnd.",
        );
    }

    #[test]
    fn partial_duplicate_groups_are_ambiguous() {
        assert_eq!(
            competing_reason(merge(
                "End.",
                "Shared.\n\nLocal.\n\nEnd.",
                "Shared.\n\nRemote.\n\nEnd.",
            )),
            CompetingEditReason::AmbiguousInsertionGroup,
        );
    }

    #[test]
    fn repeated_paragraph_within_an_insertion_group_is_ambiguous() {
        assert_eq!(
            competing_reason(merge(
                "End.",
                "Repeat.\n\nRepeat.\n\nEnd.",
                "Other.\n\nEnd.",
            )),
            CompetingEditReason::AmbiguousInsertionGroup,
        );
    }

    #[test]
    fn repeated_superset_is_taken_whole() {
        // Both sides insert the repeated paragraphs and the remote also adds
        // one before them. Aligning the two insertions piece by piece is
        // ambiguous, but the remote contains the local side's whole change,
        // so it is the merge as it stands.
        let remote = "Other.\n\nRepeat.\n\nRepeat.\n\nEnd.";
        assert_merge("End.", "Repeat.\n\nRepeat.\n\nEnd.", remote, remote);
    }

    #[test]
    fn stale_history_replay_reuses_the_already_merged_group() {
        let ancestor = "Base.";
        let local = "Base.\n\nZulu.";
        let previously_merged = "Base.\n\nAlpha.\n\nZulu.";
        assert_merge(ancestor, local, previously_merged, previously_merged);
    }

    #[test]
    fn stale_history_replay_accepts_merged_edge_deletions() {
        for (ancestor, local, remote, merged) in [
            ("A\n\nB", "A\n\nC", "B", "C"),
            ("A\n\nB", "C\n\nB", "A", "C"),
        ] {
            assert_merge(ancestor, local, remote, merged);
            assert_merge(ancestor, local, merged, merged);
        }
    }

    #[test]
    fn textual_prefix_without_a_paragraph_boundary_keeps_both_additions() {
        assert_merge(
            "End.",
            "Foo\n\nEnd.",
            "Foobar\n\nEnd.",
            "Foo\n\nFoobar\n\nEnd.",
        );
    }

    #[test]
    fn empty_ancestor_additions_receive_a_structural_separator() {
        assert_merge("", "Zulu.", "Alpha.", "Alpha.\n\nZulu.");
    }

    #[test]
    fn disjoint_paragraph_edits_merge() {
        assert_merge(
            "First old.\n\nLast old.",
            "First new.\n\nLast old.",
            "First old.\n\nLast new.",
            "First new.\n\nLast new.",
        );
    }

    #[test]
    fn disjoint_word_edits_in_one_paragraph_merge() {
        assert_merge(
            "The red fox runs quickly.",
            "The blue fox runs quickly.",
            "The red fox runs slowly.",
            "The blue fox runs slowly.",
        );
    }

    #[test]
    fn disjoint_whitespace_and_word_edits_merge_losslessly() {
        assert_merge(
            "The red fox runs quickly.",
            "The red fox\nruns quickly.",
            "The blue fox runs quickly.",
            "The blue fox\nruns quickly.",
        );
    }

    #[test]
    fn disjoint_punctuation_and_word_edits_merge() {
        assert_merge(
            "Wait, red fox!",
            "Wait; red fox!",
            "Wait, blue fox!",
            "Wait; blue fox!",
        );
    }

    #[test]
    fn equal_overlapping_edits_coalesce() {
        assert_merge(
            "The red fox runs.",
            "The blue fox walks.",
            "The blue fox runs.",
            "The blue fox walks.",
        );
    }

    #[test]
    fn competing_word_replacements_conflict() {
        assert_eq!(
            competing_reason(merge("foo", "bar", "baz")),
            CompetingEditReason::OverlappingEdits,
        );
    }

    #[test]
    fn delete_versus_edit_conflicts() {
        for (ancestor, local, remote) in [
            ("foo", "", "bar"),
            ("The red fox.", "The fox.", "The blue fox."),
            (
                "Keep.\n\nOld.\n\nEnd.",
                "Keep.\n\nEnd.",
                "Keep.\n\nNew.\n\nEnd.",
            ),
        ] {
            assert_eq!(
                competing_reason(merge(ancestor, local, remote)),
                CompetingEditReason::DeleteVsEdit,
            );
        }
    }

    #[test]
    fn delete_versus_unchanged_accepts_the_deletion() {
        assert_merge("Old.", "", "Old.", "");
        assert_merge("Old.", "", "", "");
    }

    #[test]
    fn same_boundary_inline_insertions_conflict() {
        assert_eq!(
            competing_reason(merge("The fox.", "The red fox.", "The blue fox.")),
            CompetingEditReason::OverlappingEdits,
        );
    }

    #[test]
    fn repeated_alignment_conflicts() {
        assert_eq!(
            competing_reason(merge("foo foo", "foo", "foo bar")),
            CompetingEditReason::AmbiguousRepeatedAlignment,
        );
        assert_eq!(
            competing_reason(merge(
                "Same.\n\nSame.\n\n",
                "Same.\n\n",
                "New.\n\nSame.\n\n",
            )),
            CompetingEditReason::AmbiguousRepeatedAlignment,
        );
    }

    #[test]
    fn repeated_paragraphs_ignore_separator_position_when_detecting_ambiguity() {
        assert_competing_in_both_roles(
            "A\n\nA",
            "A",
            "A\n\nB",
            CompetingEditReason::AmbiguousRepeatedAlignment,
        );
    }

    #[test]
    fn paragraph_reordering_conflicts() {
        assert_eq!(
            competing_reason(merge(
                "First.\n\nSecond.",
                "Second.\n\nFirst.",
                "First updated.\n\nSecond.",
            )),
            CompetingEditReason::OverlappingEdits,
        );
    }

    #[test]
    fn insertion_into_a_concurrently_deleted_range_conflicts() {
        assert_eq!(
            competing_reason(merge(
                "First.\n\nMiddle.\n\nLast.",
                "First.\n\nLast.",
                "First.\n\nAdded.\n\nMiddle.\n\nLast.",
            )),
            CompetingEditReason::DeleteVsEdit,
        );
    }

    #[test]
    fn structured_block_elsewhere_does_not_block_plain_refinement() {
        assert_merge(
            "The red fox runs quickly.\n\n```text\nfixed\n```",
            "The blue fox runs quickly.\n\n```text\nfixed\n```",
            "The red fox runs slowly.\n\n```text\nfixed\n```",
            "The blue fox runs slowly.\n\n```text\nfixed\n```",
        );
    }

    #[test]
    fn disjoint_prose_edits_around_fence_merge() {
        assert_merge(
            "First old.\n\n```\nold\n\ncode\n```\n\nLast old.",
            "First new.\n\n```\nold\n\ncode\n```\n\nLast old.",
            "First old.\n\n```\nold\n\ncode\n```\n\nLast new.",
            "First new.\n\n```\nold\n\ncode\n```\n\nLast new.",
        );
    }

    #[test]
    fn one_sided_fence_edit_merges_with_disjoint_prose_edit() {
        assert_merge(
            "```\nold\n```\n\nText old.",
            "```\nnew\n```\n\nText old.",
            "```\nold\n```\n\nText new.",
            "```\nnew\n```\n\nText new.",
        );
    }

    #[test]
    fn overlapping_fence_edits_are_typed_as_unsupported() {
        let result = merge("```\nold\n```", "```\nlocal\n```", "```\nremote\n```");
        let ProseMergeResult::UnsupportedStructuredOverlap(failure) = result else {
            panic!("expected structured overlap");
        };
        assert_eq!(failure.reason(), StructuredOverlapReason::MarkdownSource);
        assert_eq!(failure.inputs().ancestor(), "```\nold\n```");
    }

    #[test]
    fn overlapping_table_edits_are_typed_as_unsupported() {
        let result = merge(
            "| Name | Value |\n| --- | --- |\n| old | 1 |",
            "| Name | Value |\n| --- | --- |\n| local | 1 |",
            "| Name | Value |\n| --- | --- |\n| remote | 1 |",
        );
        assert!(matches!(
            result,
            ProseMergeResult::UnsupportedStructuredOverlap(_)
        ));
    }

    #[test]
    fn overlapping_commonmark_structures_are_never_token_merged() {
        for (ancestor, local, remote) in [
            ("\tred fox", "\tblue fox", "\tred cat"),
            ("Old red\n===", "New red\n===", "Old blue\n==="),
            (
                "See <https://old.example/path>",
                "See <https://local.example/path>",
                "See <https://old.example/remote>",
            ),
            ("**red fox**", "**blue fox**", "**red cat**"),
        ] {
            assert_eq!(
                structured_reason(merge(ancestor, local, remote)),
                StructuredOverlapReason::MarkdownSource,
            );
            assert_eq!(
                structured_reason(merge(ancestor, remote, local)),
                StructuredOverlapReason::MarkdownSource,
            );
        }
    }

    #[test]
    fn competing_structured_same_gap_additions_are_unsupported() {
        let result = merge(
            "End.",
            "```\nlocal\n```\n\nEnd.",
            "```\nremote\n```\n\nEnd.",
        );
        assert!(matches!(
            result,
            ProseMergeResult::UnsupportedStructuredOverlap(_)
        ));
    }

    #[test]
    fn unicode_is_preserved_at_utf8_boundaries() {
        assert_merge(
            "Cafe\u{301} 猫 sleeps happily.",
            "Cafe\u{301} 犬 sleeps happily.",
            "Cafe\u{301} 猫 sleeps peacefully.",
            "Cafe\u{301} 犬 sleeps peacefully.",
        );
        assert_merge("", "🌍", "你好。", "你好。\n\n🌍");
    }

    #[test]
    fn crlf_and_existing_spacing_are_preserved() {
        assert_merge(
            "  First old. \r\n \r\nLast old.  \r\n",
            "  First new. \r\n \r\nLast old.  \r\n",
            "  First old. \r\n \r\nLast new.  \r\n",
            "  First new. \r\n \r\nLast new.  \r\n",
        );
        assert_merge(
            "Base.\r\n",
            "Base.\r\n\r\nZulu.",
            "Base.\r\n\r\nAlpha.",
            "Base.\r\n\r\nAlpha.\r\n\r\nZulu.",
        );
        assert_merge(
            "First.\n\nLast old.",
            "First.\r\n\r\nLast old.",
            "First.\n\nLast new.",
            "First.\r\n\r\nLast new.",
        );
    }

    #[test]
    fn segmentation_reconstructs_exact_source_bytes() {
        for source in [
            "",
            "\n",
            "One\n\nTwo",
            "```\nline\n\nline\n```\n\nAfter",
            "Cafe\u{301}\t猫\u{a0}🌍\r\n\nEnd",
        ] {
            for level in [Level::Paragraphs, Level::Tokens] {
                let pieces = Pieces::new(source, level, PieceLimit::new(128)).unwrap();
                let mut reconstructed = String::new();
                for index in 0..pieces.len() {
                    reconstructed.push_str(pieces.piece(index));
                }
                assert_eq!(reconstructed.as_bytes(), source.as_bytes());
            }
        }
    }

    #[test]
    fn oversized_changed_input_reports_input_exhaustion() {
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(3),
            PieceLimit::new(20),
            AlignmentWorkLimit::new(100),
            ComparisonByteLimit::new(100),
        );
        assert_eq!(
            exhaustion_reason(merge_prose("old", "local", "remote", limits)),
            WorkExhaustionReason::InputBytes,
        );
    }

    #[test]
    fn fast_paths_do_not_allocate_or_consume_work_limits() {
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(0),
            PieceLimit::new(0),
            AlignmentWorkLimit::new(0),
            ComparisonByteLimit::new(0),
        );
        let huge = "text ".repeat(100);
        assert_eq!(merged_text(&merge_prose("", &huge, "", limits)), huge,);
    }

    #[test]
    fn piece_limit_has_a_distinct_exhaustion_result() {
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(100),
            PieceLimit::new(1),
            AlignmentWorkLimit::new(100),
            ComparisonByteLimit::new(100),
        );
        assert_eq!(
            exhaustion_reason(merge_prose("a b", "a c", "d b", limits)),
            WorkExhaustionReason::Pieces,
        );
    }

    #[test]
    fn alignment_limit_has_a_distinct_exhaustion_result() {
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(100),
            PieceLimit::new(20),
            AlignmentWorkLimit::new(1),
            ComparisonByteLimit::new(100),
        );
        assert_eq!(
            exhaustion_reason(merge_prose("one", "two", "three", limits)),
            WorkExhaustionReason::AlignmentWork,
        );
    }

    #[test]
    fn comparison_limit_has_a_distinct_exhaustion_result() {
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(100),
            PieceLimit::new(20),
            AlignmentWorkLimit::new(100),
            ComparisonByteLimit::new(0),
        );
        assert_eq!(
            exhaustion_reason(merge_prose("one", "two", "four", limits)),
            WorkExhaustionReason::ComparisonBytes,
        );
    }

    #[test]
    fn same_gap_ordering_consumes_the_comparison_budget() {
        let local = format!("{}x", "a".repeat(100));
        let remote = format!("{}yy", "a".repeat(100));
        let limits = ProseMergeLimits::new(
            InputByteLimit::new(1_000),
            PieceLimit::new(20),
            AlignmentWorkLimit::new(1_000),
            ComparisonByteLimit::new(0),
        );
        for (first, second) in [(&local, &remote), (&remote, &local)] {
            assert_eq!(
                exhaustion_reason(merge_prose("", first, second, limits)),
                WorkExhaustionReason::ComparisonBytes,
            );
        }
    }

    #[test]
    fn default_work_limits_have_role_symmetric_outcomes() {
        let repeated = "Repeat.\n\nRepeat.";
        let mut unique = String::new();
        for index in 0..2_001 {
            if index > 0 {
                unique.push_str("\n\n");
            }
            unique.push_str("Unique ");
            unique.push_str(&index.to_string());
            unique.push('.');
        }
        for (first, second) in [(repeated, unique.as_str()), (unique.as_str(), repeated)] {
            assert_eq!(
                competing_reason(merge("", first, second)),
                CompetingEditReason::AmbiguousInsertionGroup,
            );
        }
    }

    /// The quadratic longest-common-subsequence table that [`token_distance`]
    /// replaced, kept as its oracle.
    fn token_distance_by_table(
        from: &str,
        to: &str,
        budget: &mut WorkBudget,
    ) -> Result<PieceIndex, MergeError> {
        let from = Pieces::new(from, Level::Tokens, budget.piece_limit)?;
        let to = Pieces::new(to, Level::Tokens, budget.piece_limit)?;
        // Pieces shared at both ends are in every longest common subsequence.
        let mut prefix = 0;
        while prefix < from.len().min(to.len())
            && budget.equal(from.piece(prefix), to.piece(prefix))?
        {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < (from.len() - prefix).min(to.len() - prefix)
            && budget.equal(
                from.piece(from.len() - 1 - suffix),
                to.piece(to.len() - 1 - suffix),
            )?
        {
            suffix += 1;
        }
        let from_middle = prefix..from.len() - suffix;
        let to_middle = prefix..to.len() - suffix;
        let cells = from_middle
            .len()
            .checked_mul(to_middle.len())
            .ok_or(MergeError::Exhausted(WorkExhaustionReason::AlignmentWork))?;
        budget.reserve_alignment(cells)?;
        // `row[column]`: longest common subsequence of the `from` pieces seen so
        // far and the first `column` pieces of the `to` middle.
        let mut row: Vec<MatchCount> = vec![0; to_middle.len() + 1];
        for from_index in from_middle {
            let mut diagonal = 0;
            for (column, to_index) in to_middle.clone().enumerate() {
                let above = row[column + 1];
                row[column + 1] = if budget.equal(from.piece(from_index), to.piece(to_index))? {
                    diagonal + 1
                } else {
                    above.max(row[column])
                };
                diagonal = above;
            }
        }
        let common = prefix + suffix + row[to_middle.len()];
        Ok(from.len() + to.len() - 2 * common)
    }

    #[test]
    fn token_distance_matches_the_quadratic_table() {
        let mut checked = 0;
        for seed in 0..2000 {
            for shape in [generated::Shape::Subsumed, generated::Shape::Disjoint] {
                let Some(case) = generated::case(seed, shape) else {
                    continue;
                };
                let texts = [&case.ancestor, &case.local, &case.remote, &case.expected];
                for from in texts {
                    for to in texts {
                        let limits = ProseMergeLimits::default();
                        let fast = token_distance(from, to, &mut WorkBudget::new(limits));
                        let table = token_distance_by_table(from, to, &mut WorkBudget::new(limits));
                        assert_eq!(fast, table, "{from:?} {to:?}");
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 1000, "{checked} pairs checked");
        assert_eq!(
            token_distance("", "", &mut WorkBudget::new(ProseMergeLimits::default())),
            Ok(0)
        );
    }

    #[test]
    fn long_sections_with_distant_small_edits_merge() {
        // The merge containment check measures distances over whole texts;
        // they must stay within the default work limits when the edits are
        // small, however long the section. Paragraphs of 100 words keep the
        // piecewise word alignment, which is quadratic in one paragraph's
        // length, within the limits too.
        for paragraphs in [6, 12, 18] {
            let ancestor = (0..paragraphs)
                .map(|paragraph| {
                    (0..100)
                        .map(|word| format!("w{paragraph}x{word}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            let local = format!("start {ancestor}");
            let remote = format!("{ancestor} end").replacen(" w1x50 ", " w1x50 changed ", 1);
            let merged = format!("start {remote}");
            assert_merge(&ancestor, &local, &remote, &merged);
        }
    }
}
