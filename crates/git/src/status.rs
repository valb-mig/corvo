use std::path::Path;

use corvo_domain::{Change, Changed, Repo, Stage, Sync};

// The repository from `git status --porcelain=v2 -z --branch --show-stash`.
pub fn parse(root: &Path, out: &[u8]) -> Repo {
    let mut branch = None;
    let mut head = None;
    let mut sync = Sync::default();
    let mut stashes = 0;
    let mut changes = Vec::new();
    let mut fields = out
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned());
    while let Some(line) = fields.next() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            head = (rest != "(initial)").then(|| rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = (rest != "(detached)").then(|| rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            sync.upstream = Some(rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            let mut counts = rest
                .split(' ')
                .map(|n| n.trim_start_matches(['+', '-']).parse().unwrap_or(0));
            sync.ahead = counts.next().unwrap_or(0);
            sync.behind = counts.next().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("# stash ") {
            stashes = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("1 ") {
            let (xy, path) = (rest.get(..2), rest.splitn(8, ' ').nth(7));
            if let (Some(xy), Some(path)) = (xy, path)
                && let Some(change) = change(xy)
            {
                changes.push(Changed {
                    path: root.join(path),
                    change,
                    from: None,
                    stage: stage(xy),
                });
            }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            // The path it came from follows, as a field of its own.
            let from = fields.next();
            if let Some(path) = rest.splitn(9, ' ').nth(8) {
                changes.push(Changed {
                    path: root.join(path),
                    change: Change::Renamed,
                    from: from.map(|from| root.join(from)),
                    stage: stage(rest.get(..2).unwrap_or("..")),
                });
            }
        } else if let Some(rest) = line.strip_prefix("u ") {
            if let Some(path) = rest.splitn(10, ' ').nth(9) {
                changes.push(Changed {
                    path: root.join(path),
                    change: Change::Conflicted,
                    from: None,
                    stage: Stage::default(),
                });
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            changes.push(Changed {
                path: root.join(path),
                change: Change::Untracked,
                from: None,
                stage: Stage {
                    index: None,
                    tree: Some(Change::Untracked),
                },
            });
        }
    }
    let detached = branch.is_none() && head.is_some();
    let branch = branch
        .or_else(|| {
            head.as_deref()
                .map(|head| head[..head.len().min(7)].to_owned())
        })
        .unwrap_or_default();
    Repo::new(root, branch, head, changes)
        .detached(detached)
        .with_sync(sync)
        .with_stashes(stashes)
}

// How a file listed with its index and work tree letters stands against the last commit; none
// when it is new in the index and gone from the work tree.
fn change(xy: &str) -> Option<Change> {
    let mut letters = xy.chars();
    let (x, y) = (letters.next()?, letters.next()?);
    Some(match (x, y) {
        ('A', 'D') => return None,
        ('A', _) => Change::Added,
        ('D', _) | (_, 'D') => Change::Deleted,
        ('R' | 'C', _) => Change::Renamed,
        _ => Change::Modified,
    })
}

// Where each half of the change sits.
fn stage(xy: &str) -> Stage {
    let mut letters = xy.chars();
    let side = |letter: Option<char>| match letter? {
        'M' | 'T' => Some(Change::Modified),
        'A' => Some(Change::Added),
        'D' => Some(Change::Deleted),
        'R' | 'C' => Some(Change::Renamed),
        _ => None,
    };
    Stage {
        index: side(letters.next()),
        tree: side(letters.next()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_branch_and_each_kind_of_entry() {
        let out = [
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "1 .M N... 100644 100644 100644 aaa bbb src/a b.rs",
            "1 A. N... 000000 100644 100644 000 ccc new.rs",
            "1 .D N... 100644 100644 000000 ddd ddd gone.rs",
            "2 R. N... 100644 100644 100644 eee eee R100 to.rs",
            "from.rs",
            "u UU N... 100644 100644 100644 100644 f g h both.rs",
            "? notes.md",
            "",
        ]
        .join("\0");
        let repo = parse(Path::new("/r"), out.as_bytes());
        assert_eq!(repo.branch(), "main");
        assert!(!repo.is_detached());
        assert_eq!(repo.head(), Some("1234567890abcdef"));
        let got: Vec<_> = repo
            .changes()
            .iter()
            .map(|c| (c.path.to_string_lossy().into_owned(), c.change))
            .collect();
        assert_eq!(
            got,
            [
                ("/r/both.rs", Change::Conflicted),
                ("/r/gone.rs", Change::Deleted),
                ("/r/new.rs", Change::Added),
                ("/r/notes.md", Change::Untracked),
                ("/r/src/a b.rs", Change::Modified),
                ("/r/to.rs", Change::Renamed),
            ]
            .map(|(path, change)| (path.to_owned(), change))
        );
        let renamed = repo.changes().iter().find(|c| c.change == Change::Renamed);
        assert_eq!(
            renamed.and_then(|c| c.from.as_deref()),
            Some(Path::new("/r/from.rs"))
        );
    }

    #[test]
    fn splits_each_change_between_the_index_and_the_work_tree() {
        let out = [
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "1 MM N... 100644 100644 100644 a b both.rs",
            "1 M. N... 100644 100644 100644 a b staged.rs",
            "1 .M N... 100644 100644 100644 a b unstaged.rs",
            "? new.rs",
            "",
        ]
        .join("\0");
        let repo = parse(Path::new("/r"), out.as_bytes());
        let stage = |name: &str| {
            repo.changes()
                .iter()
                .find(|c| c.path.ends_with(name))
                .map(|c| c.stage)
                .unwrap()
        };
        assert_eq!(
            stage("both.rs"),
            Stage {
                index: Some(Change::Modified),
                tree: Some(Change::Modified)
            }
        );
        assert!(stage("staged.rs").staged() && !stage("staged.rs").unstaged());
        assert!(!stage("unstaged.rs").staged() && stage("unstaged.rs").unstaged());
        assert_eq!(stage("new.rs").tree, Some(Change::Untracked));
    }

    #[test]
    fn reads_the_upstream_how_far_apart_and_the_stashes() {
        let out = [
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +7 -2",
            "# stash 3",
            "",
        ]
        .join("\0");
        let repo = parse(Path::new("/r"), out.as_bytes());
        assert_eq!(repo.sync().upstream.as_deref(), Some("origin/main"));
        assert_eq!((repo.sync().ahead, repo.sync().behind), (7, 2));
        assert_eq!(repo.stashes(), 3);
    }

    #[test]
    fn a_detached_head_goes_by_its_short_commit() {
        let out = "# branch.oid 1234567890abcdef\0# branch.head (detached)\0";
        let repo = parse(Path::new("/r"), out.as_bytes());
        assert_eq!(repo.branch(), "1234567");
        assert!(repo.is_detached());
    }

    #[test]
    fn before_the_first_commit_there_is_no_head() {
        let out = "# branch.oid (initial)\0# branch.head main\0";
        let repo = parse(Path::new("/r"), out.as_bytes());
        assert_eq!((repo.branch(), repo.head()), ("main", None));
    }
}
