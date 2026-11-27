// A patch of one file cut in its hunks, to stage, unstage or discard a part of it.

// A hunk of a patch: where it starts on each side, and its lines with their sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchHunk {
    pub old_start: usize,
    pub new_start: usize,
    // What git says after the second `@@`, the scope it is in.
    pub scope: String,
    pub lines: Vec<String>,
}

impl PatchHunk {
    // The first line it changes, and on which side: one it adds by its number in the new file,
    // else one it takes out by its number in the old.
    pub fn first_change(&self) -> Option<(bool, usize)> {
        let (mut old, mut new) = (self.old_start, self.new_start);
        for line in &self.lines {
            match line.as_bytes().first() {
                Some(b'+') => return Some((true, new)),
                Some(b'-') => return Some((false, old)),
                Some(b'\\') => {}
                _ => {
                    old += 1;
                    new += 1;
                }
            }
        }
        None
    }

    // Whether it holds the line on that side, by its number.
    pub fn holds(&self, new_side: bool, line: usize) -> bool {
        let (mut old, mut new) = (self.old_start, self.new_start);
        for text in &self.lines {
            let here = match text.as_bytes().first() {
                Some(b'+') => {
                    new += 1;
                    new_side && new - 1 == line
                }
                Some(b'-') => {
                    old += 1;
                    !new_side && old - 1 == line
                }
                Some(b'\\') => false,
                _ => {
                    old += 1;
                    new += 1;
                    (if new_side { new } else { old }) - 1 == line
                }
            };
            if here {
                return true;
            }
        }
        false
    }
}

// A patch of one file: the lines before its first hunk (`diff --git`, `---`, `+++`), and its
// hunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub head: Vec<String>,
    pub hunks: Vec<PatchHunk>,
}

impl Patch {
    pub fn parse(text: &str) -> Self {
        let mut head = Vec::new();
        let mut hunks: Vec<PatchHunk> = Vec::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("@@ -") {
                let (ranges, scope) = rest.split_once(" @@").unwrap_or((rest, ""));
                let mut sides = ranges.split(" +");
                let start = |side: Option<&str>| {
                    side.and_then(|s| s.split(',').next())
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0)
                };
                let old_start = start(sides.next());
                let new_start = start(sides.next());
                hunks.push(PatchHunk {
                    old_start,
                    new_start,
                    scope: scope.trim().to_owned(),
                    lines: Vec::new(),
                });
            } else if let Some(hunk) = hunks.last_mut() {
                hunk.lines.push(line.to_owned());
            } else {
                head.push(line.to_owned());
            }
        }
        Self { head, hunks }
    }

    // The patch with only its hunk at `ix`.
    pub fn hunk(&self, ix: usize) -> Option<String> {
        let hunk = self.hunks.get(ix)?;
        Some(self.write(&[cut(hunk, |_, _| true, false)?]))
    }

    // The patch with only the lines picked, the others left as they are: a line taken out that
    // is not picked stays, a line added that is not picked is not added. `reverse` builds it to
    // be applied backwards, for taking picked lines out of the index or the work tree, where a
    // line added that is not picked stays and one taken out that is not picked is not there.
    // `picked` gets whether the line is on the new side, and its number there.
    pub fn lines(&self, picked: impl Fn(bool, usize) -> bool, reverse: bool) -> Option<String> {
        let hunks: Vec<String> = self
            .hunks
            .iter()
            .filter_map(|hunk| cut(hunk, &picked, reverse))
            .collect();
        (!hunks.is_empty()).then(|| self.write(&hunks))
    }

    fn write(&self, hunks: &[String]) -> String {
        let mut out = String::new();
        for line in &self.head {
            out.push_str(line);
            out.push('\n');
        }
        for hunk in hunks {
            out.push_str(hunk);
        }
        out
    }
}

// A hunk with only the changes picked, its counts made again; none when no change is left.
fn cut(hunk: &PatchHunk, picked: impl Fn(bool, usize) -> bool, reverse: bool) -> Option<String> {
    let (mut old, mut new) = (hunk.old_start, hunk.new_start);
    let mut body = Vec::new();
    let (mut old_len, mut new_len, mut changes) = (0, 0, 0);
    for line in &hunk.lines {
        match line.as_bytes().first() {
            Some(b'+') => {
                if picked(true, new) {
                    body.push(line.clone());
                    new_len += 1;
                    changes += 1;
                } else if reverse {
                    // Applied backwards, it must be found there: it stays, as context.
                    body.push(format!(" {}", &line[1..]));
                    old_len += 1;
                    new_len += 1;
                }
                new += 1;
            }
            Some(b'-') => {
                if picked(false, old) {
                    body.push(line.clone());
                    old_len += 1;
                    changes += 1;
                } else if !reverse {
                    body.push(format!(" {}", &line[1..]));
                    old_len += 1;
                    new_len += 1;
                }
                old += 1;
            }
            // `\ No newline at end of file` belongs to the line before it.
            Some(b'\\') => body.push(line.clone()),
            _ => {
                body.push(line.clone());
                old_len += 1;
                new_len += 1;
                old += 1;
                new += 1;
            }
        }
    }
    if changes == 0 {
        return None;
    }
    let mut out = format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, old_len, hunk.new_start, new_len
    );
    for line in body {
        out.push_str(&line);
        out.push('\n');
    }
    Some(out)
}
