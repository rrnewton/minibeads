//! Line-based three-way merge (diff3) with standard conflict hunks.
//!
//! This is the textual layer under `mb merge-driver`: structured issue and
//! comment merges render each conflicting field through it so that conflict
//! hunks cover only the lines that actually disagree, and files that cannot be
//! merged structurally fall back to it wholesale.

use similar::{capture_diff_slices, Algorithm, DiffOp};

type LineIndex = usize;

/// Length of the `<<<<<<<`-style marker runs (git's `%L`, default 7).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MarkerSize(usize);

impl MarkerSize {
    pub(crate) const DEFAULT: Self = Self(7);

    /// Git never asks for fewer than 7; shorter markers would be ambiguous with
    /// ordinary Markdown such as setext headings.
    pub(crate) fn new(size: usize) -> Self {
        Self(size.max(Self::DEFAULT.0))
    }
}

/// Number of conflict hunks written into a merge result.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct HunkCount(usize);

impl HunkCount {
    pub(crate) const fn get(self) -> usize {
        self.0
    }

    pub(crate) const fn is_clean(self) -> bool {
        self.0 == 0
    }

    fn increment(&mut self) {
        self.0 += 1;
    }
}

impl std::ops::AddAssign for HunkCount {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// Fixed labels keep conflict hunks machine-recognisable: a resolver can look
/// for exactly `<<<<<<< ours`, `||||||| base`, `=======`, `>>>>>>> theirs`.
pub(crate) const OURS_LABEL: &str = "ours";
pub(crate) const BASE_LABEL: &str = "base";
pub(crate) const THEIRS_LABEL: &str = "theirs";

/// Text produced by a line merge plus the number of conflict hunks in it.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct LineMerge {
    pub(crate) text: String,
    pub(crate) hunks: HunkCount,
}

/// The three versions of one region of text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreeWay<'a> {
    pub(crate) base: &'a str,
    pub(crate) ours: &'a str,
    pub(crate) theirs: &'a str,
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// For every base line, the index of the line it is matched to on `side`.
fn base_matches(base: &[&str], side: &[&str]) -> Vec<Option<LineIndex>> {
    let mut matches = vec![None; base.len()];
    for op in capture_diff_slices(Algorithm::Myers, base, side) {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            for offset in 0..len {
                matches[old_index + offset] = Some(new_index + offset);
            }
        }
    }
    matches
}

fn push_lines(output: &mut String, lines: &[&str]) {
    for line in lines {
        output.push_str(line);
    }
}

fn push_marker_line(output: &mut String, marker: char, size: MarkerSize, label: Option<&str>) {
    output.extend(std::iter::repeat_n(marker, size.0));
    if let Some(label) = label {
        output.push(' ');
        output.push_str(label);
    }
    output.push('\n');
}

fn push_side(output: &mut String, text: &str) {
    output.push_str(text);
    if !text.is_empty() && !text.ends_with('\n') {
        output.push('\n');
    }
}

/// Append one diff3-style conflict hunk holding the three given texts.
pub(crate) fn push_conflict(output: &mut String, region: ThreeWay<'_>, size: MarkerSize) {
    push_marker_line(output, '<', size, Some(OURS_LABEL));
    push_side(output, region.ours);
    push_marker_line(output, '|', size, Some(BASE_LABEL));
    push_side(output, region.base);
    push_marker_line(output, '=', size, None);
    push_side(output, region.theirs);
    push_marker_line(output, '>', size, Some(THEIRS_LABEL));
}

fn merge_chunk(
    output: &mut String,
    base: &[&str],
    ours: &[&str],
    theirs: &[&str],
    size: MarkerSize,
    hunks: &mut HunkCount,
) {
    if ours == theirs || theirs == base {
        push_lines(output, ours);
    } else if ours == base {
        push_lines(output, theirs);
    } else {
        let base_text = base.concat();
        let ours_text = ours.concat();
        let theirs_text = theirs.concat();
        push_conflict(
            output,
            ThreeWay {
                base: &base_text,
                ours: &ours_text,
                theirs: &theirs_text,
            },
            size,
        );
        hunks.increment();
    }
}

/// Merge three texts line by line.
///
/// Base lines matched on both sides are synchronisation points; each region
/// between two of them is taken from whichever side changed it, or becomes a
/// conflict hunk when both sides changed it differently. The algorithm is
/// symmetric: swapping `ours` and `theirs` swaps only the hunk contents.
pub(crate) fn merge_lines(region: ThreeWay<'_>, size: MarkerSize) -> LineMerge {
    let base = split_lines(region.base);
    let ours = split_lines(region.ours);
    let theirs = split_lines(region.theirs);
    let ours_matches = base_matches(&base, &ours);
    let theirs_matches = base_matches(&base, &theirs);

    let mut output = String::with_capacity(region.ours.len().max(region.theirs.len()));
    let mut hunks = HunkCount::default();
    let (mut base_start, mut ours_start, mut theirs_start) = (0, 0, 0);
    for (base_index, line) in base.iter().enumerate() {
        let (Some(ours_index), Some(theirs_index)) =
            (ours_matches[base_index], theirs_matches[base_index])
        else {
            continue;
        };
        merge_chunk(
            &mut output,
            &base[base_start..base_index],
            &ours[ours_start..ours_index],
            &theirs[theirs_start..theirs_index],
            size,
            &mut hunks,
        );
        output.push_str(line);
        base_start = base_index + 1;
        ours_start = ours_index + 1;
        theirs_start = theirs_index + 1;
    }
    merge_chunk(
        &mut output,
        &base[base_start..],
        &ours[ours_start..],
        &theirs[theirs_start..],
        size,
        &mut hunks,
    );
    LineMerge {
        text: output,
        hunks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(base: &str, ours: &str, theirs: &str) -> LineMerge {
        merge_lines(ThreeWay { base, ours, theirs }, MarkerSize::DEFAULT)
    }

    #[test]
    fn disjoint_line_edits_merge_cleanly() {
        let result = merge("a\nb\nc\nd\ne\n", "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n");
        assert_eq!(result.text, "A\nb\nc\nd\nE\n");
        assert!(result.hunks.is_clean());
    }

    #[test]
    fn identical_edits_merge_cleanly() {
        let result = merge("a\nb\n", "a\nB\n", "a\nB\n");
        assert_eq!(result.text, "a\nB\n");
        assert!(result.hunks.is_clean());
    }

    #[test]
    fn overlapping_edits_produce_one_precise_hunk() {
        let result = merge("a\nb\nc\n", "a\nours\nc\n", "a\ntheirs\nc\n");
        assert_eq!(
            result.text,
            "a\n<<<<<<< ours\nours\n||||||| base\nb\n=======\ntheirs\n>>>>>>> theirs\nc\n"
        );
        assert_eq!(result.hunks.get(), 1);
    }

    #[test]
    fn missing_final_newline_is_terminated_inside_a_hunk() {
        let result = merge("x", "y", "z");
        assert_eq!(
            result.text,
            "<<<<<<< ours\ny\n||||||| base\nx\n=======\nz\n>>>>>>> theirs\n"
        );
    }

    #[test]
    fn marker_size_is_respected_and_never_below_seven() {
        let result = merge_lines(
            ThreeWay {
                base: "b\n",
                ours: "o\n",
                theirs: "t\n",
            },
            MarkerSize::new(9),
        );
        assert!(result.text.starts_with("<<<<<<<<< ours\n"));
        assert_eq!(MarkerSize::new(3), MarkerSize::DEFAULT);
    }

    #[test]
    fn deletion_versus_unchanged_takes_the_deletion() {
        let result = merge("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        assert_eq!(result.text, "a\nc\n");
        assert!(result.hunks.is_clean());
    }
}
