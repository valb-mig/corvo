// Parsers of what git prints, kept apart from running it so they can be tested on its words.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use corvo_domain::{Branch, Change, Commit, Doing, FileStat, RefName, Stash, Tag, Worktree};

// The fields of a commit in `git log --format`, NUL between them and RS after each.
pub const COMMIT: &str = "%H%x00%P%x00%an%x00%ae%x00%at%x00%D%x00%s";
pub const END: char = '\u{1e}';

// The commits of `git log --decorate=full --format=COMMIT%x1e`, with a reflog note as a last
// field when one was asked for.
pub fn commits(out: &str, head: Option<&str>) -> Vec<Commit> {
    out.split(END)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split('\0');
            let hash = fields.next()?.to_owned();
            let parents = fields
                .next()?
                .split(' ')
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect();
            let author = fields.next()?.to_owned();
            let email = fields.next()?.to_owned();
            let when = fields.next()?.parse().unwrap_or(0);
            let refs = decorations(fields.next()?, head);
            let subject = fields.next()?.to_owned();
            let note = fields.next().map(str::to_owned).filter(|n| !n.is_empty());
            Some(Commit {
                hash,
                parents,
                author,
                email,
                when,
                subject,
                refs,
                note,
            })
        })
        .collect()
}

// The names of `%D` under `--decorate=full`: `HEAD -> refs/heads/main, refs/remotes/origin/main,
// tag: refs/tags/v1`. The branch checked out comes first; a detached HEAD has none.
fn decorations(field: &str, head: Option<&str>) -> Vec<RefName> {
    let mut refs: Vec<RefName> = Vec::new();
    for part in field.split(", ").filter(|p| !p.is_empty()) {
        if let Some(branch) = part.strip_prefix("HEAD -> ") {
            if let Some(RefName::Local(name)) = RefName::from_full(branch) {
                refs.insert(0, RefName::Head(name));
            }
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            refs.extend(RefName::from_full(tag));
        } else if part != "HEAD" {
            match RefName::from_full(part) {
                // Another worktree's branch, or this one's when git leaves out the arrow.
                Some(RefName::Local(name)) if head == Some(name.as_str()) => {
                    refs.insert(0, RefName::Head(name))
                }
                Some(name) => refs.push(name),
                None => {}
            }
        }
    }
    refs
}

// The files of `--numstat -z` and `--name-status -z` over the same range, together.
pub fn files(numstat: &str, status: &str) -> Vec<FileStat> {
    let mut changes: HashMap<String, (Change, Option<String>)> = HashMap::new();
    let mut fields = status.split('\0').filter(|f| !f.is_empty());
    while let Some(letter) = fields.next() {
        let change = match letter.chars().next() {
            Some('A') => Change::Added,
            Some('D') => Change::Deleted,
            Some('R' | 'C') => Change::Renamed,
            Some('U') => Change::Conflicted,
            _ => Change::Modified,
        };
        if change == Change::Renamed {
            let (Some(from), Some(to)) = (fields.next(), fields.next()) else {
                break;
            };
            changes.insert(to.to_owned(), (change, Some(from.to_owned())));
        } else if let Some(path) = fields.next() {
            changes.insert(path.to_owned(), (change, None));
        }
    }
    let mut files = Vec::new();
    let mut fields = numstat.split('\0');
    while let Some(field) = fields.next() {
        let field = field.trim_start_matches('\n');
        if field.is_empty() {
            continue;
        }
        let mut parts = field.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // A rename leaves the path empty, and its two paths follow as fields of their own.
        let path = if path.is_empty() {
            let _from = fields.next();
            match fields.next() {
                Some(to) => to.to_owned(),
                None => break,
            }
        } else {
            path.to_owned()
        };
        let lines = match (added.parse(), removed.parse()) {
            (Ok(a), Ok(r)) => Some((a, r)),
            _ => None,
        };
        let (change, from) = changes.remove(&path).unwrap_or((Change::Modified, None));
        files.push(FileStat {
            path,
            from,
            change,
            lines,
        });
    }
    files
}

// The branches and tags of `git for-each-ref --format=REFS`, one per line.
pub const REFS: &str = "%(refname)%00%(objectname)%00%(*objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(worktreepath)%00%(creatordate:unix)%00%(contents:subject)";

pub fn refs(out: &str) -> (Vec<Branch>, Vec<Branch>, Vec<Tag>) {
    let (mut local, mut remote, mut tags) = (Vec::new(), Vec::new(), Vec::new());
    for line in out.lines() {
        let f: Vec<&str> = line.split('\0').collect();
        if f.len() < 9 {
            continue;
        }
        let when = f[7].parse().unwrap_or(0);
        match RefName::from_full(f[0]) {
            Some(RefName::Tag(name)) => tags.push(Tag {
                name,
                // An annotated tag points at itself; the commit it names comes peeled.
                hash: if f[2].is_empty() { f[1] } else { f[2] }.to_owned(),
                when,
            }),
            Some(name @ (RefName::Local(_) | RefName::Remote(_))) => {
                let is_remote = matches!(name, RefName::Remote(_));
                let (ahead, behind, gone) = track(f[5]);
                let branch = Branch {
                    name: name.name().to_owned(),
                    remote: is_remote,
                    hash: f[1].to_owned(),
                    head: f[3] == "*",
                    upstream: (!f[4].is_empty()).then(|| f[4].to_owned()),
                    gone,
                    ahead,
                    behind,
                    base: None,
                    worktree: (!f[6].is_empty()).then(|| PathBuf::from(f[6])),
                    merged: false,
                    when,
                    subject: f[8].to_owned(),
                };
                if is_remote {
                    remote.push(branch);
                } else {
                    local.push(branch);
                }
            }
            _ => {}
        }
    }
    (local, remote, tags)
}

// `ahead 2, behind 1`, `gone`, or nothing.
fn track(field: &str) -> (usize, usize, bool) {
    let (mut ahead, mut behind) = (0, 0);
    for part in field.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind, field == "gone")
}

// The stashes of `git stash list --format=%gd%x00%H%x00%ct%x00%gs`.
pub fn stashes(out: &str) -> Vec<Stash> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.split('\0');
            let name = f.next()?;
            let index = name
                .strip_prefix("stash@{")?
                .strip_suffix('}')?
                .parse()
                .ok()?;
            let hash = f.next()?.to_owned();
            let when = f.next()?.parse().unwrap_or(0);
            let said = f.next()?;
            // `WIP on main: 2e49ded subject` when no message was given, `On main: message`
            // when one was.
            let (branch, message) = if let Some(rest) = said.strip_prefix("WIP on ") {
                let branch = rest.split(':').next().unwrap_or_default();
                (branch, said)
            } else if let Some(rest) = said.strip_prefix("On ") {
                let (branch, message) = rest.split_once(": ").unwrap_or((rest, ""));
                (branch, message)
            } else {
                ("", said)
            };
            Some(Stash {
                index,
                hash,
                message: message.to_owned(),
                branch: branch.to_owned(),
                when,
            })
        })
        .collect()
}

// The worktrees of `git worktree list --porcelain -z`: fields, with an empty one after each.
pub fn worktrees(out: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in out.split('\0') {
        if field.is_empty() {
            list.extend(current.take());
            continue;
        }
        if let Some(path) = field.strip_prefix("worktree ") {
            list.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                main: list.is_empty(),
                locked: false,
                prunable: false,
            });
            continue;
        }
        let Some(tree) = current.as_mut() else {
            continue;
        };
        if let Some(head) = field.strip_prefix("HEAD ") {
            tree.head = Some(head.to_owned()).filter(|h| h.chars().any(|c| c != '0'));
        } else if let Some(branch) = field.strip_prefix("branch ") {
            tree.branch = Some(branch.trim_start_matches("refs/heads/").to_owned());
        } else if field == "locked" || field.starts_with("locked ") {
            tree.locked = true;
        } else if field == "prunable" || field.starts_with("prunable ") {
            tree.prunable = true;
        }
    }
    list.extend(current);
    list
}

// What git left half done in a repository, read from its folder (`--absolute-git-dir`).
pub fn doing(git_dir: &Path) -> Option<Doing> {
    let read = |name: &str| {
        fs::read_to_string(git_dir.join(name))
            .ok()
            .map(|text| text.trim().to_owned())
    };
    let short = |hash: String| hash[..hash.len().min(7)].to_owned();
    for dir in ["rebase-merge", "rebase-apply"] {
        if git_dir.join(dir).is_dir() {
            let (step, steps) = if dir == "rebase-merge" {
                ("msgnum", "end")
            } else {
                ("next", "last")
            };
            let number = |name: &str| {
                read(&format!("{dir}/{name}"))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0)
            };
            let branch = read(&format!("{dir}/head-name"))
                .map(|name| name.trim_start_matches("refs/heads/").to_owned())
                .unwrap_or_default();
            return Some(Doing::Rebase {
                branch,
                onto: read(&format!("{dir}/onto")).map(short).unwrap_or_default(),
                step: number(step),
                steps: number(steps),
            });
        }
    }
    if let Some(hash) = read("MERGE_HEAD") {
        // `Merge branch 'feat/x'`, `Merge remote-tracking branch 'origin/main'`.
        let named = read("MERGE_MSG").and_then(|msg| {
            let first = msg.lines().next()?.to_owned();
            let start = first.find('\'')? + 1;
            let end = start + first[start..].find('\'')?;
            Some(first[start..end].to_owned())
        });
        let first = hash.lines().next().unwrap_or_default().to_owned();
        return Some(Doing::Merge {
            theirs: named.unwrap_or_else(|| short(first)),
        });
    }
    if let Some(hash) = read("CHERRY_PICK_HEAD") {
        return Some(Doing::CherryPick {
            theirs: short(hash),
        });
    }
    read("REVERT_HEAD").map(|hash| Doing::Revert {
        theirs: short(hash),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_commits_with_their_parents_and_names() {
        let out = format!(
            "aaa\0bbb ccc\0Dev\0dev@x\x001700000000\0HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1\0merge it{END}\n\
             bbb\0\0Dev\0dev@x\x001690000000\0\0first{END}\n"
        );
        let commits = commits(&out, Some("main"));
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, ["bbb", "ccc"]);
        assert_eq!(
            commits[0].refs,
            [
                RefName::Head("main".into()),
                RefName::Remote("origin/main".into()),
                RefName::Tag("v1".into())
            ]
        );
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].subject, "first");
        assert_eq!(commits[1].note, None);
    }

    #[test]
    fn reads_the_files_a_range_touched() {
        let numstat = "3\t1\ta.rs\0-\t-\timg.png\x000\t0\t\0old.rs\0new.rs\0";
        let status = "M\0a.rs\0A\0img.png\0R100\0old.rs\0new.rs\0";
        let files = files(numstat, status);
        assert_eq!(files.len(), 3);
        assert_eq!(
            (files[0].change, files[0].lines),
            (Change::Modified, Some((3, 1)))
        );
        assert_eq!((files[1].change, files[1].lines), (Change::Added, None));
        assert_eq!(files[2].path, "new.rs");
        assert_eq!(files[2].from.as_deref(), Some("old.rs"));
        assert_eq!(files[2].change, Change::Renamed);
    }

    #[test]
    fn reads_branches_tags_and_how_they_track() {
        let out = [
            "refs/heads/main\0aaa\0\0*\0origin/main\0ahead 7, behind 2\0/w/corvo\x001700\0last one",
            "refs/heads/old\0bbb\0\0 \0origin/old\0gone\0\x001600\0gone one",
            "refs/remotes/origin/main\0ccc\0\0 \0\0\0\x001650\0remote one",
            "refs/remotes/origin/HEAD\0ccc\0\0 \0\0\0\x001650\0remote one",
            "refs/tags/v1\0tagobj\0ddd\0 \0\0\0\x001500\0release",
        ]
        .join("\n");
        let (local, remote, tags) = refs(&out);
        assert_eq!(local.len(), 2);
        assert!(local[0].head);
        assert_eq!((local[0].ahead, local[0].behind), (7, 2));
        assert_eq!(local[0].worktree.as_deref(), Some(Path::new("/w/corvo")));
        assert!(local[1].gone);
        assert_eq!(remote.len(), 1);
        assert_eq!(remote[0].name, "origin/main");
        assert_eq!(tags[0].hash, "ddd");
    }

    #[test]
    fn reads_stashes_with_their_branch_and_message() {
        let out = "stash@{0}\0aaa\x001700\0On main: half of it\n\
                   stash@{1}\0bbb\x001600\0WIP on fix/x: 2e49ded subject\n";
        let stashes = stashes(out);
        assert_eq!(stashes[0].index, 0);
        assert_eq!(
            (stashes[0].branch.as_str(), stashes[0].message.as_str()),
            ("main", "half of it")
        );
        assert_eq!(stashes[1].branch, "fix/x");
        assert_eq!(stashes[1].message, "WIP on fix/x: 2e49ded subject");
    }

    #[test]
    fn reads_worktrees() {
        let out = "worktree /w/corvo\0HEAD aaa\0branch refs/heads/main\0\0\
                   worktree /w/corvo/.claude/worktrees/x\0HEAD bbb\0detached\0locked\0\0\
                   worktree /gone\0HEAD ccc\0branch refs/heads/old\0prunable gitdir file points to non-existent location\0\0";
        let trees = worktrees(out);
        assert_eq!(trees.len(), 3);
        assert!(trees[0].main && !trees[1].main);
        assert_eq!(trees[0].branch.as_deref(), Some("main"));
        assert_eq!(trees[1].branch, None);
        assert!(trees[1].locked);
        assert!(trees[2].prunable);
    }

    #[test]
    fn reads_what_git_left_half_done() {
        let dir = std::env::temp_dir().join(format!("corvo-doing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(doing(&dir), None);
        fs::write(dir.join("MERGE_HEAD"), "1234567890\n").unwrap();
        fs::write(
            dir.join("MERGE_MSG"),
            "Merge branch 'feat/x'\n\n# Conflicts:\n",
        )
        .unwrap();
        assert_eq!(
            doing(&dir),
            Some(Doing::Merge {
                theirs: "feat/x".into()
            })
        );
        fs::create_dir_all(dir.join("rebase-merge")).unwrap();
        fs::write(dir.join("rebase-merge/head-name"), "refs/heads/feat/x\n").unwrap();
        fs::write(dir.join("rebase-merge/onto"), "abcdef1234\n").unwrap();
        fs::write(dir.join("rebase-merge/msgnum"), "3\n").unwrap();
        fs::write(dir.join("rebase-merge/end"), "5\n").unwrap();
        assert_eq!(
            doing(&dir),
            Some(Doing::Rebase {
                branch: "feat/x".into(),
                onto: "abcdef1".into(),
                step: 3,
                steps: 5
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
