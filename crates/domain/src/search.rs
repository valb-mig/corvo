use std::ops::Range;
use std::path::PathBuf;

use regex::{Regex, RegexBuilder};

// How much of a line a hit's preview keeps before it, in characters, and how long it gets at
// most, in bytes.
const BEFORE: usize = 40;
const LONGEST: usize = 240;

// What is looked for in the files: the text, and how it is matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    // Upper and lower case kept apart.
    pub case: bool,
    // Only where it stands as a whole word.
    pub word: bool,
    // The text read as a regular expression.
    pub regex: bool,
}

impl Query {
    // How the query finds: none while it is empty, and an error the dev can read when the
    // expression does not hold, or would match empty text everywhere.
    pub fn finder(&self) -> Result<Option<Finder>, String> {
        if self.text.is_empty() {
            return Ok(None);
        }
        let mut pattern = if self.regex {
            self.text.clone()
        } else {
            regex::escape(&self.text)
        };
        if self.word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(!self.case)
            .multi_line(true)
            .build()
            .map_err(|err| reason(&err))?;
        if re.is_match("") {
            return Err("The expression matches empty text".to_owned());
        }
        Ok(Some(Finder {
            re,
            literal: !self.regex,
        }))
    }
}

#[derive(Clone, Debug)]
pub struct Finder {
    re: Regex,
    // The replacement goes in as it is, `$` and all.
    literal: bool,
}

// A line where a query was found in a text, with each place on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    // The line, 0-based.
    pub line: u32,
    // Where the first place on the line is in the whole text, in bytes.
    pub range: Range<usize>,
    // How many places there are on the line.
    pub count: usize,
    // The line, cut around the first place when long, and where the places are in that; those
    // cut out are left unmarked.
    pub preview: String,
    pub marks: Vec<Range<usize>>,
}

// The hits of a query in the files of a project, file by file in the order they were walked.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub files: Vec<(PathBuf, Vec<Hit>)>,
    // How many places, all lines together.
    pub hits: usize,
    // It stopped at the most it looks for; there may be more.
    pub capped: bool,
}

impl Found {
    // Takes the lines of one more file, as many as fit under `most` places; tells whether there
    // is room for more.
    pub fn add(&mut self, path: PathBuf, mut hits: Vec<Hit>, most: usize) -> bool {
        let mut taken = 0;
        let fit = hits
            .iter()
            .take_while(|hit| {
                let fits = self.hits + taken + hit.count <= most;
                if fits {
                    taken += hit.count;
                }
                fits
            })
            .count();
        if fit < hits.len() {
            hits.truncate(fit);
            self.capped = true;
        }
        if !hits.is_empty() {
            self.hits += taken;
            self.files.push((path, hits));
        }
        !self.capped && self.hits < most
    }
}

impl Finder {
    // The lines the query is on in a text, with at most `most` places in all.
    pub fn find(&self, text: &str, most: usize) -> Vec<Hit> {
        let mut hits: Vec<Hit> = Vec::new();
        // The line being read: its number, where it starts, and where its preview is cut.
        let (mut line, mut start, mut scanned) = (0u32, 0usize, 0usize);
        let mut cut = Cut::default();
        for found in self.matches(text).take(most) {
            let mut moved = false;
            for (ix, _) in text[scanned..found.start()].match_indices('\n') {
                line += 1;
                start = scanned + ix + 1;
                moved = true;
            }
            scanned = found.start();
            let end = text[found.start()..]
                .find('\n')
                .map_or(text.len(), |ix| found.start() + ix);
            let mark = found.start() - start..found.end().min(end) - start;
            match hits.last_mut() {
                Some(hit) if hit.line == line && !moved => {
                    hit.count += 1;
                    hit.marks.extend(cut.place(&mark));
                }
                _ => {
                    let (preview, made) = preview(&text[start..end], mark.clone());
                    cut = made;
                    hits.push(Hit {
                        line,
                        range: found.range(),
                        count: 1,
                        preview,
                        marks: cut.place(&mark).into_iter().collect(),
                    });
                }
            }
        }
        hits
    }

    // Whether the text holds the query at all.
    pub fn is_in(&self, text: &str) -> bool {
        self.matches(text).next().is_some()
    }

    // The text with every hit replaced, and how many there were. Into a regular expression's
    // replacement its groups go as `$1` or `${name}`; plain text goes in as it is.
    pub fn replace(&self, text: &str, with: &str) -> (String, usize) {
        let mut replaced = String::with_capacity(text.len());
        let (mut last, mut count) = (0, 0);
        for caps in self.re.captures_iter(text) {
            let Some(found) = caps.get(0).filter(|found| !found.is_empty()) else {
                continue;
            };
            replaced.push_str(&text[last..found.start()]);
            if self.literal {
                replaced.push_str(with);
            } else {
                caps.expand(with, &mut replaced);
            }
            last = found.end();
            count += 1;
        }
        replaced.push_str(&text[last..]);
        (replaced, count)
    }

    // What the expression matches, but where it matches nothing between two characters, as a
    // word's edge alone does.
    fn matches<'t>(&self, text: &'t str) -> impl Iterator<Item = regex::Match<'t>> + use<'_, 't> {
        self.re.find_iter(text).filter(|found| !found.is_empty())
    }
}

// Where a line was cut for its preview, and how far the ellipsis put in front moved it.
#[derive(Clone, Copy, Debug, Default)]
struct Cut {
    start: usize,
    end: usize,
    shift: usize,
}

impl Cut {
    // Where a place on the line is in the preview, unless it was cut out.
    fn place(&self, mark: &Range<usize>) -> Option<Range<usize>> {
        (mark.start >= self.start && mark.end <= self.end)
            .then(|| mark.start - self.start + self.shift..mark.end - self.start + self.shift)
    }
}

// The line of a hit as it is shown: without its indent, and cut to some characters before the
// first place and a length that fits, an ellipsis where it was cut in front.
fn preview(line: &str, mark: Range<usize>) -> (String, Cut) {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mark = mark.start.min(line.len())..mark.end.min(line.len());
    let indent = line.len() - line.trim_start().len();
    let mut start = indent.min(mark.start);
    if line[start..mark.start].chars().count() > BEFORE {
        start = line[..mark.start]
            .char_indices()
            .rev()
            .nth(BEFORE - 1)
            .map_or(start, |(ix, _)| ix);
    }
    let mut end = line.len();
    if end - start > LONGEST {
        end = (start + LONGEST).max(mark.end);
        while !line.is_char_boundary(end) {
            end += 1;
        }
    }
    let lead = if start > indent { "…" } else { "" };
    let cut = Cut {
        start,
        end,
        shift: lead.len(),
    };
    (format!("{lead}{}", &line[start..end]), cut)
}

// The last line of what the regex crate says, which names the fault.
fn reason(err: &regex::Error) -> String {
    let text = err.to_string();
    let last = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("The expression does not hold");
    let last = last.trim().strip_prefix("error: ").unwrap_or(last.trim());
    let mut chars = last.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
