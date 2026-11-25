use std::ops::Range;

// Pieces of a text by line: the bytes from the start of one line to the end of another, and the
// lines where a name comes, with the name marked.

pub fn span(text: &str, first: u32, last: u32) -> Option<Range<usize>> {
    if last < first {
        return None;
    }
    let starts = starts(text);
    let start = *starts.get(first as usize)?;
    if last as usize >= starts.len() {
        return None;
    }
    Some(start..line_end(text, &starts, last as usize))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    line: u32,
    text: String,
    marks: Vec<Range<usize>>,
}

impl Mention {
    pub fn line(&self) -> u32 {
        self.line
    }

    // The line without its indent.
    pub fn text(&self) -> &str {
        &self.text
    }

    // Byte ranges of the name in the text.
    pub fn marks(&self) -> &[Range<usize>] {
        &self.marks
    }
}

pub fn mentions(text: &str, ranges: &[Range<usize>]) -> Vec<Mention> {
    let starts = starts(text);
    let mut ranges = ranges.to_vec();
    ranges.sort_by_key(|range| range.start);
    let mut out: Vec<Mention> = Vec::new();
    for range in ranges {
        let line = starts.partition_point(|&start| start <= range.start) - 1;
        let start = starts[line];
        let end = line_end(text, &starts, line);
        let full = &text[start..end];
        let indent = full.len() - full.trim_start().len();
        let shift = start + indent;
        let mark = range.start.saturating_sub(shift)..range.end.min(end).saturating_sub(shift);
        match out.last_mut() {
            Some(last) if last.line == line as u32 => last.marks.push(mark),
            _ => out.push(Mention {
                line: line as u32,
                text: full[indent..].to_owned(),
                marks: vec![mark],
            }),
        }
    }
    out
}

fn starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(ix, _)| ix + 1))
        .collect()
}

fn line_end(text: &str, starts: &[usize], line: usize) -> usize {
    let end = starts.get(line + 1).map_or(text.len(), |next| next - 1);
    if text[..end].ends_with('\r') {
        end - 1
    } else {
        end
    }
}
