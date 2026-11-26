use std::ops::Range;

use corvo_domain::{Hunk, HunkMark};
use gpui_kit::component::input::{
    GutterDecoration, GutterDecorationCollection, OverviewRulerDecoration,
    OverviewRulerDecorationCollection, OverviewRulerLane, RangeDecoration,
    RangeDecorationCollection, RangeDecorationStyle,
};
use gpui_kit::{App, Context};

use super::State;
use crate::git::mark_tint;

// The changes of a file against its last commit: a bar in the gutter, a light fill behind the
// lines and a tick on the scrollbar. Lines taken out leave a mark between the lines around
// them.
pub struct Marks {
    gutter: GutterDecorationCollection,
    fill: RangeDecorationCollection,
    ruler: OverviewRulerDecorationCollection,
}

impl Marks {
    pub fn new(state: &mut State, cx: &mut Context<State>) -> Self {
        Self {
            gutter: state.create_gutter_decorations_collection(Vec::new(), cx),
            fill: state.create_range_decorations_collection(Vec::new(), cx),
            ruler: state.create_overview_ruler_decorations_collection(Vec::new(), cx),
        }
    }

    // The hunks of the text as the editor has it; with no fill, only the bars and ticks.
    pub fn set(&self, text: &str, hunks: &[Hunk], fill: bool, cx: &mut App) {
        let starts = starts(text);
        let mut gutter = Vec::new();
        let mut fills = Vec::new();
        let mut ruler = Vec::new();
        for hunk in hunks {
            let color = mark_tint(hunk.mark, cx);
            let range = span(&starts, text.len(), &hunk.lines);
            gutter.push(GutterDecoration::new(range.clone()).with_color(color));
            ruler.push(
                OverviewRulerDecoration::new(range.clone())
                    .with_lane(OverviewRulerLane::Left)
                    .with_color(color),
            );
            if fill && hunk.mark != HunkMark::Deleted {
                fills.push(
                    RangeDecoration::new(range)
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(color.opacity(0.08)),
                );
            }
        }
        self.gutter.set(gutter, cx);
        self.fill.set(fills, cx);
        self.ruler.set(ruler, cx);
    }

    pub fn clear(&self, cx: &mut App) {
        self.gutter.clear(cx);
        self.fill.clear(cx);
        self.ruler.clear(cx);
    }
}

// Where each line starts.
fn starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
    starts
}

// The bytes of a run of lines, up to the start of the line after it; none, at the start of the
// line where lines were taken out.
fn span(starts: &[usize], len: usize, lines: &Range<usize>) -> Range<usize> {
    let at = |line: usize| starts.get(line).copied().unwrap_or(len).min(len);
    at(lines.start)..at(lines.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_lines_goes_up_to_the_next_line() {
        let text = "a\nbb\nc";
        let starts = starts(text);
        assert_eq!(span(&starts, text.len(), &(1..2)), 2..5);
        assert_eq!(span(&starts, text.len(), &(2..3)), 5..6);
    }

    #[test]
    fn lines_taken_out_are_a_point_at_a_line_start() {
        let text = "a\nb\n";
        let starts = starts(text);
        assert_eq!(span(&starts, text.len(), &(1..1)), 2..2);
        assert_eq!(span(&starts, text.len(), &(5..5)), 4..4);
    }
}
