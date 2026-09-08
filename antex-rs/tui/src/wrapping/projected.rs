use std::ops::Range;

use textwrap::Options;
use textwrap::core::Word;
use textwrap::word_splitters::split_words;
use unicode_segmentation::UnicodeSegmentation;

use crate::width::display_width;

/// Projected text keeps source-offset lookup separate from legal grapheme split points.
pub(super) struct ProjectedText {
    text: String,
    source_boundaries: Vec<(usize, usize)>,
    grapheme_boundaries: Vec<usize>,
}

/// Replaces halfwidth sound-mark graphemes with equally wide, textwrap-safe placeholders.
///
/// Source boundaries recover original byte offsets, while grapheme boundaries keep placeholders
/// indivisible and preserve leading whitespace as a wrapping opportunity.
pub(super) fn project_halfwidth_sound_marks(text: &str) -> Option<ProjectedText> {
    if !text.contains(['\u{FF9E}', '\u{FF9F}']) {
        return None;
    }

    let mut projected = String::with_capacity(text.len());
    let mut source_boundaries = vec![(0, 0)];
    let mut grapheme_boundaries = vec![0];
    for (source_start, grapheme) in text.grapheme_indices(/*is_extended*/ true) {
        if grapheme.contains(['\u{FF9E}', '\u{FF9F}']) {
            let source_end = source_start + grapheme.len();
            let content_start = grapheme
                .find(|ch: char| !ch.is_whitespace())
                .unwrap_or(grapheme.len());
            let (whitespace, content) = grapheme.split_at(content_start);
            for (offset, ch) in whitespace.char_indices() {
                projected.push(ch);
                source_boundaries.push((projected.len(), source_start + offset + ch.len_utf8()));
                grapheme_boundaries.push(projected.len());
            }

            let width = display_width(content);
            let projected_start = projected.len();
            for _ in 0..width / 2 {
                if projected.len() > projected_start {
                    projected.push('\u{2060}');
                }
                projected.push('界');
                source_boundaries.push((projected.len(), source_end));
            }
            if width % 2 == 1 {
                if projected.len() > projected_start {
                    projected.push('\u{2060}');
                }
                projected.push('a');
                source_boundaries.push((projected.len(), source_end));
            }
        } else {
            for (offset, ch) in grapheme.char_indices() {
                projected.push(ch);
                source_boundaries.push((projected.len(), source_start + offset + ch.len_utf8()));
            }
        }
        grapheme_boundaries.push(projected.len());
    }

    Some(ProjectedText {
        text: projected,
        source_boundaries,
        grapheme_boundaries,
    })
}

/// Maps a projected byte offset back to the corresponding original-text boundary.
fn source_offset(boundaries: &[(usize, usize)], projected_offset: usize) -> usize {
    boundaries
        .binary_search_by_key(&projected_offset, |(offset, _)| *offset)
        .map(|index| boundaries[index].1)
        .unwrap_or(projected_offset)
}

/// Splits oversized projected words without separating placeholders for one source grapheme.
fn break_projected_words<'a>(
    words: impl Iterator<Item = Word<'a>>,
    projected: &'a ProjectedText,
    line_width: usize,
) -> Vec<Word<'a>> {
    let projected_start = projected.text.as_ptr() as usize;
    let mut pieces = Vec::new();

    for word in words {
        if display_width(word.word) <= line_width {
            pieces.push(word);
            continue;
        }

        let word_start = word.word.as_ptr() as usize - projected_start;
        let word_end = word_start + word.word.len();
        let mut piece_start = word_start;
        let mut piece_width = 0;
        let mut atom_start = word_start;
        let boundary_start = projected
            .grapheme_boundaries
            .partition_point(|atom_end| *atom_end <= word_start);

        for atom_end in projected
            .grapheme_boundaries
            .iter()
            .copied()
            .skip(boundary_start)
        {
            if atom_end > word_end {
                break;
            }

            let atom_width = display_width(&projected.text[atom_start..atom_end]);
            if piece_width > 0 && piece_width + atom_width > line_width {
                pieces.push(Word::from(&projected.text[piece_start..atom_start]));
                piece_start = atom_start;
                piece_width = 0;
            }
            piece_width += atom_width;
            atom_start = atom_end;
        }

        let mut last = Word::from(&projected.text[piece_start..word_end]);
        last.whitespace = word.whitespace;
        last.penalty = word.penalty;
        pieces.push(last);
    }

    pieces
}

/// Wraps projected text and translates the resulting ranges back to source byte offsets.
pub(super) fn wrap_projected_ranges(
    projected: &ProjectedText,
    opts: &Options<'_>,
    include_trailing_spaces: bool,
) -> Vec<Range<usize>> {
    let line_widths = [
        opts.width
            .saturating_sub(display_width(opts.initial_indent)),
        opts.width
            .saturating_sub(display_width(opts.subsequent_indent)),
    ];
    let line_ending = opts.line_ending.as_str();
    let mut ranges = Vec::new();
    let mut line_start = 0;

    for line in projected.text.split(line_ending) {
        let words = opts.word_separator.find_words(line);
        let split_words = split_words(words, &opts.word_splitter);
        let mut broken_words = if opts.break_words {
            break_projected_words(split_words, projected, line_widths[1])
        } else {
            split_words.collect()
        };
        if opts.break_words && !opts.initial_indent.is_empty() {
            broken_words.insert(0, Word::from(""));
        }

        let wrapped_words = opts.wrap_algorithm.wrap(&broken_words, &line_widths);
        let mut cursor = line_start;
        for words in wrapped_words {
            let Some(last_word) = words.last() else {
                let source = source_offset(&projected.source_boundaries, cursor);
                ranges.push(source..source + usize::from(include_trailing_spaces));
                continue;
            };
            let len = words
                .iter()
                .map(|word| word.word.len() + word.whitespace.len())
                .sum::<usize>()
                - last_word.whitespace.len();
            let end = cursor + len;
            let trailing_spaces = if include_trailing_spaces {
                projected.text[end..]
                    .chars()
                    .take_while(|ch| *ch == ' ')
                    .count()
            } else {
                0
            };
            let source_start = source_offset(&projected.source_boundaries, cursor);
            let source_end = source_offset(&projected.source_boundaries, end + trailing_spaces);
            ranges.push(source_start..source_end + usize::from(include_trailing_spaces));
            cursor = end + last_word.whitespace.len();
        }
        line_start += line.len() + line_ending.len();
    }

    ranges
}
