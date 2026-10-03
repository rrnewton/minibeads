use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Range;

type ByteOffset = usize;
type ByteCount = usize;
type PieceIndex = usize;
type MatchCount = usize;
type WorkCount = usize;

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
        Ok(merged) => ProseMergeResult::Merged(MergedProse(Cow::Owned(merged))),
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

fn merge_changed(
    inputs: ProseMergeInputs<'_>,
    limits: ProseMergeLimits,
) -> Result<String, MergeError> {
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

    let mut budget = WorkBudget::new(limits);
    let (first, second) = match budget.compare(inputs.local, inputs.remote)? {
        Ordering::Less => (inputs.local, inputs.remote),
        Ordering::Equal => unreachable!("equal replicas use the borrowed fast path"),
        Ordering::Greater => (inputs.remote, inputs.local),
    };
    let output_capacity = inputs
        .local
        .len()
        .checked_add(inputs.remote.len())
        .ok_or(MergeError::Exhausted(WorkExhaustionReason::InputBytes))?;
    let mut output = String::with_capacity(output_capacity);
    merge_level(
        inputs.ancestor,
        first,
        second,
        Level::Paragraphs,
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

    fn span(&self, start: PieceIndex, end: PieceIndex) -> Range<ByteOffset> {
        self.boundaries[start]..self.boundaries[end]
    }
}

fn paragraph_content(piece: &str) -> &str {
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
    let start = first_content.unwrap_or(content_end);
    &piece[start..content_end]
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

fn diff<'a>(
    ancestor: &Pieces<'_>,
    side: &Pieces<'a>,
    budget: &mut WorkBudget,
) -> Result<Vec<Edit<'a>>, MergeError> {
    let columns = side.len() + 1;
    let cells = (ancestor.len() + 1)
        .checked_mul(columns)
        .ok_or(MergeError::Exhausted(WorkExhaustionReason::AlignmentWork))?;
    budget.reserve_alignment(cells)?;
    let mut lengths: Vec<MatchCount> = vec![0; cells];
    for ancestor_index in (0..ancestor.len()).rev() {
        for side_index in (0..side.len()).rev() {
            lengths[ancestor_index * columns + side_index] = if budget.equal(
                ancestor.matching_piece(ancestor_index),
                side.matching_piece(side_index),
            )? {
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
        let mut next_match: Option<MatchedPieces> = None;
        for ancestor_index in ancestor_start..ancestor.len() {
            if lengths[ancestor_index * columns + side_start] < remaining {
                break;
            }
            for side_index in side_start..side.len() {
                budget.reserve_alignment(1)?;
                if lengths[ancestor_index * columns + side_index] < remaining {
                    break;
                }
                if lengths[(ancestor_index + 1) * columns + side_index + 1] + 1 == remaining
                    && budget.equal(
                        ancestor.matching_piece(ancestor_index),
                        side.matching_piece(side_index),
                    )?
                {
                    if let Some(prior_match) = next_match {
                        let content_anchor =
                            is_content_anchor(ancestor.matching_piece(ancestor_index));
                        let repeated_ancestor = budget.equal(
                            ancestor.matching_piece(prior_match.ancestor),
                            ancestor.matching_piece(ancestor_index),
                        )?;
                        let repeated_side = budget.equal(
                            side.matching_piece(prior_match.side),
                            side.matching_piece(side_index),
                        )?;
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
        let raw_match = ancestor.level == Level::Tokens
            || budget.equal(ancestor.piece(matched.ancestor), side.piece(matched.side))?;
        let matched_width = usize::from(!raw_match);
        push_edit(
            &mut edits,
            ancestor,
            side,
            ancestor_start..matched.ancestor + matched_width,
            side_start..matched.side + matched_width,
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

fn is_content_anchor(piece: &str) -> bool {
    piece.chars().any(|character| {
        character.is_alphanumeric() || (!character.is_ascii() && !character.is_whitespace())
    })
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
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), MergeError> {
    let ancestor_pieces = Pieces::new(ancestor, level, budget.piece_limit)?;
    let local_pieces = Pieces::new(local, level, budget.piece_limit)?;
    let remote_pieces = Pieces::new(remote, level, budget.piece_limit)?;
    let local_edits = diff(&ancestor_pieces, &local_pieces, budget)?;
    let remote_edits = diff(&ancestor_pieces, &remote_pieces, budget)?;
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

fn merge_overlap(
    ancestor: &str,
    region: &Range<ByteOffset>,
    local: &str,
    remote: &str,
    level: Level,
    budget: &mut WorkBudget,
    output: &mut String,
) -> Result<(), MergeError> {
    if budget.equal(local, remote)? {
        output.push_str(local);
        return Ok(());
    }
    let original = &ancestor[region.start..region.end];
    if level == Level::Paragraphs {
        if replay_includes_edge_deletion(original, local, remote, budget)? {
            output.push_str(remote);
            return Ok(());
        }
        if replay_includes_edge_deletion(original, remote, local, budget)? {
            output.push_str(local);
            return Ok(());
        }
    }
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
        // Word-level merging is only sound inside one paragraph. Across
        // paragraph breaks the token alignment can anchor on text repeated in
        // an inserted paragraph and emit that paragraph twice.
        for text in [original, local, remote] {
            if Pieces::new(text, Level::Paragraphs, budget.piece_limit)?.len() > 1 {
                return Err(MergeError::Competing(CompetingEditReason::OverlappingEdits));
            }
        }
        return merge_level(original, local, remote, Level::Tokens, budget, output);
    }

    Err(MergeError::Competing(CompetingEditReason::OverlappingEdits))
}

fn replay_includes_edge_deletion(
    ancestor: &str,
    prior_side: &str,
    candidate: &str,
    budget: &mut WorkBudget,
) -> Result<bool, MergeError> {
    let ancestor_pieces = Pieces::new(ancestor, Level::Paragraphs, budget.piece_limit)?;
    let prior_pieces = Pieces::new(prior_side, Level::Paragraphs, budget.piece_limit)?;
    let candidate_pieces = Pieces::new(candidate, Level::Paragraphs, budget.piece_limit)?;
    if candidate_pieces.len() >= prior_pieces.len() {
        return Ok(false);
    }

    let mut unchanged_prefix = 0;
    while unchanged_prefix < ancestor_pieces.len()
        && unchanged_prefix < prior_pieces.len()
        && budget.equal(
            ancestor_pieces.piece(unchanged_prefix),
            prior_pieces.piece(unchanged_prefix),
        )?
    {
        unchanged_prefix += 1;
    }

    let mut unchanged_suffix = 0;
    while unchanged_suffix < ancestor_pieces.len().saturating_sub(unchanged_prefix)
        && unchanged_suffix < prior_pieces.len().saturating_sub(unchanged_prefix)
    {
        let ancestor_index = ancestor_pieces.len() - unchanged_suffix - 1;
        let prior_index = prior_pieces.len() - unchanged_suffix - 1;
        if !budget.equal(
            ancestor_pieces.piece(ancestor_index),
            prior_pieces.piece(prior_index),
        )? {
            break;
        }
        unchanged_suffix += 1;
    }

    let removed = prior_pieces.len() - candidate_pieces.len();
    for removed_prefix in 0..=unchanged_prefix.min(removed) {
        budget.reserve_alignment(1)?;
        let removed_suffix = removed - removed_prefix;
        if removed_suffix > unchanged_suffix {
            continue;
        }
        let prior_start = removed_prefix;
        let prior_end = prior_pieces.len() - removed_suffix;
        let mut matches = true;
        for (candidate_index, prior_index) in (prior_start..prior_end).enumerate() {
            budget.reserve_alignment(1)?;
            if !budget.equal(
                prior_pieces.matching_piece(prior_index),
                candidate_pieces.matching_piece(candidate_index),
            )? {
                matches = false;
                break;
            }
        }
        if matches {
            return Ok(true);
        }
    }
    Ok(false)
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
    fn overlap_spanning_paragraphs_never_duplicates_an_insertion() {
        // Found by the issue-merge re-merge property: the local side rewrote
        // P0 and carries the remote's inserted paragraph, whose words repeat
        // P0's. A token merge across the break emitted the insertion twice.
        assert_competing_in_both_roles(
            "P0 Golf hotel india.\n\nP1 Mike.\n",
            "Golf hotel india. (ours)\n\nAdded: Golf hotel india.\n\nP1 Mike.\n",
            "P0 Golf hotel india.\n\nAdded: Golf hotel india.\n\nP1 Mike.\n",
            CompetingEditReason::OverlappingEdits,
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
    fn repeated_superset_cannot_bypass_insertion_ambiguity() {
        assert_competing_in_both_roles(
            "End.",
            "Repeat.\n\nRepeat.\n\nEnd.",
            "Other.\n\nRepeat.\n\nRepeat.\n\nEnd.",
            CompetingEditReason::AmbiguousInsertionGroup,
        );
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
}
