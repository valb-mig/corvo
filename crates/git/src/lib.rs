mod op;
mod read;
mod status;

use std::collections::HashSet;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use corvo_domain::port::{Git, Side};
use corvo_domain::{
    Apart, Commit, Comparison, Detail, FileStat, Holders, Log, LogScope, Op, RefName, Refs, Repo,
    Stash, Worktree,
};

use read::COMMIT;

// How many commits a comparison lists at most.
const COMPARED: usize = 500;
// How many branches tracking nothing are measured against the main one.
const MEASURED: usize = 40;

// Git as the dev has it installed, run once per question. Reading takes no optional locks, so
// it never stands in the way of the git the dev or an agent runs meanwhile.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitCli;

impl Git for GitCli {
    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>> {
        // The way up to the top of the repository, so the root keeps the folder's own spelling,
        // and where git keeps its own files, for what it left half done.
        let up = git(dir, &["rev-parse", "--show-cdup", "--absolute-git-dir"])?;
        if !up.status.success() {
            return Ok(None);
        }
        let said = String::from_utf8_lossy(&up.stdout).into_owned();
        let mut lines = said.lines();
        let root = clean(&dir.join(lines.next().unwrap_or_default().trim()));
        let git_dir = lines.next().map(|line| PathBuf::from(line.trim()));
        let out = git(
            &root,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--show-stash",
                "--untracked-files=all",
            ],
        )?;
        if !out.status.success() {
            return Err(failure(&out));
        }
        let doing = git_dir.as_deref().and_then(read::doing);
        Ok(Some(status::parse(&root, &out.stdout).with_doing(doing)))
    }

    fn base(&self, root: &Path, file: &Path) -> io::Result<Option<String>> {
        let Some(inner) = inside(root, file) else {
            return Ok(None);
        };
        let out = git(root, &["show", &format!("HEAD:{inner}")])?;
        if !out.status.success() || out.stdout.contains(&0) {
            return Ok(None);
        }
        Ok(String::from_utf8(out.stdout).ok())
    }

    fn patch(&self, root: &Path, file: &Path) -> io::Result<String> {
        let Some(inner) = inside(root, file) else {
            return Ok(String::new());
        };
        let out = git(
            root,
            &["diff", "--no-color", "--no-ext-diff", "HEAD", "--", &inner],
        )?;
        if out.status.success() && !out.stdout.is_empty() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        // Not in the last commit, or no commit yet: the whole file is new.
        whole(root, &inner)
    }

    fn side_patch(&self, root: &Path, file: &Path, side: Side) -> io::Result<String> {
        let Some(inner) = inside(root, file) else {
            return Ok(String::new());
        };
        let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
        if side == Side::Staged {
            args.push("--cached");
        }
        args.extend(["--", &inner]);
        let out = git(root, &args)?;
        if !out.status.success() {
            return Err(failure(&out));
        }
        if out.stdout.is_empty() && side == Side::Unstaged && untracked(root, &inner)? {
            return whole(root, &inner);
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn log(&self, root: &Path, log: &Log) -> io::Result<Vec<Commit>> {
        let head = head_branch(root);
        let take = log.skip + log.limit;
        let text = log.text.trim();
        let format = |reflog: bool| {
            let note = if reflog { "%x00%gd: %gs" } else { "" };
            format!("--format={COMMIT}{note}%x1e")
        };
        let run = |extra: &[&str], pathspec: Option<&str>| -> io::Result<Vec<Commit>> {
            let reflog = log.scope == LogScope::Reflog;
            let mut args: Vec<String> = vec![
                "log".into(),
                "--decorate=full".into(),
                format(reflog),
                format!("-n{take}"),
            ];
            match &log.scope {
                LogScope::All => {
                    args.extend(["--date-order".into(), "--exclude=refs/stash".into()]);
                    args.push("--all".into());
                }
                LogScope::Rev(rev) => {
                    args.push("--date-order".into());
                    args.push(rev.clone());
                }
                LogScope::Reflog => args.push("--walk-reflogs".into()),
            }
            args.extend(extra.iter().map(|a| (*a).to_owned()));
            if let Some(pathspec) = pathspec {
                args.extend(["--".into(), pathspec.to_owned()]);
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = git(root, &args)?;
            if !out.status.success() {
                // A branch with no commit yet, or a repository with none: nothing to list.
                return Ok(Vec::new());
            }
            Ok(read::commits(
                &String::from_utf8_lossy(&out.stdout),
                head.as_deref(),
            ))
        };
        let commits = if text.is_empty() {
            run(&[], None)?
        } else {
            // The text is looked for in the message, the author, a hash and a path; the lists
            // come together newest first, each commit once.
            let grep = format!("--grep={text}");
            let author = format!("--author={text}");
            let mut found = run(&["-i", "--fixed-strings", &grep], None)?;
            found.extend(run(&["-i", "--fixed-strings", &author], None)?);
            if !text.contains(char::is_whitespace) {
                found.extend(run(&[], Some(&format!(":(glob,icase)**/*{text}*")))?);
            }
            if text.len() >= 4 && text.chars().all(|c| c.is_ascii_hexdigit()) {
                let rev = format!("{text}^{{commit}}");
                let out = git(root, &["rev-parse", "--verify", "--quiet", &rev])?;
                if out.status.success() {
                    let hash = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                    let out = git(
                        root,
                        &[
                            "log",
                            "--decorate=full",
                            &format!("--format={COMMIT}%x1e"),
                            "-n1",
                            &hash,
                        ],
                    )?;
                    found.extend(read::commits(
                        &String::from_utf8_lossy(&out.stdout),
                        head.as_deref(),
                    ));
                }
            }
            let mut seen = HashSet::new();
            found.retain(|commit| seen.insert(commit.hash.clone()));
            found.sort_by(|a, b| b.when.cmp(&a.when));
            found
        };
        Ok(commits.into_iter().skip(log.skip).take(log.limit).collect())
    }

    fn detail(&self, root: &Path, rev: &str) -> io::Result<Detail> {
        let format = "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%D%x00%B%x1e";
        let out = checked(root, &["show", "-s", "--decorate=full", format, rev, "--"])?;
        let head = head_branch(root);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        // The whole message stands where the subject does; split it after.
        let mut commit = read::commits(&text, head.as_deref())
            .into_iter()
            .next()
            .ok_or_else(|| io::Error::other(format!("No commit {rev}.")))?;
        let message = std::mem::take(&mut commit.subject);
        let (subject, body) = message.split_once('\n').unwrap_or((&message, ""));
        commit.subject = subject.trim().to_owned();
        let files = match commit.parents.first() {
            Some(parent) => files(root, &[parent.as_str(), commit.hash.as_str()], false)?,
            None => files(root, &[commit.hash.as_str()], true)?,
        };
        Ok(Detail {
            body: body.trim().to_owned(),
            commit,
            files,
        })
    }

    fn show(&self, root: &Path, rev: &str) -> io::Result<String> {
        let parent = format!("{rev}^1");
        let has_parent = git(root, &["rev-parse", "--verify", "--quiet", &parent])?
            .status
            .success();
        let out = if has_parent {
            checked(
                root,
                &[
                    "diff",
                    "--no-color",
                    "--no-ext-diff",
                    "-M",
                    &parent,
                    rev,
                    "--",
                ],
            )?
        } else {
            checked(
                root,
                &[
                    "show",
                    "--format=",
                    "--no-color",
                    "--no-ext-diff",
                    "-M",
                    rev,
                    "--",
                ],
            )?
        };
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn holders(&self, root: &Path, rev: &str) -> io::Result<Holders> {
        let out = checked(
            root,
            &["branch", "--all", "--contains", rev, "--format=%(refname)"],
        )?;
        let mut holders = Holders::default();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            match RefName::from_full(line.trim()) {
                Some(RefName::Local(name)) => holders.local.push(name),
                Some(RefName::Remote(name)) => holders.remote.push(name),
                _ => {}
            }
        }
        Ok(holders)
    }

    fn refs(&self, root: &Path) -> io::Result<Refs> {
        let format = format!("--format={}", read::REFS);
        let out = checked(
            root,
            &[
                "for-each-ref",
                "--sort=-creatordate",
                &format,
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
        )?;
        let (mut local, remote, tags) = read::refs(&String::from_utf8_lossy(&out.stdout));
        let main = main_branch(root, &local, &remote);
        if let Some(main) = &main {
            let out = git(
                root,
                &["branch", "--merged", main, "--format=%(refname:short)"],
            )?;
            let merged: HashSet<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|line| line.trim().to_owned())
                .collect();
            let mut measured = 0;
            for branch in &mut local {
                branch.merged = branch.name != *main && merged.contains(&branch.name);
                if branch.upstream.is_some() || branch.name == *main || measured >= MEASURED {
                    continue;
                }
                measured += 1;
                branch.base = apart(root, main, &branch.name).ok();
            }
        }
        // The branch checked out first, then the main one, then the newest.
        local.sort_by_key(|branch| (!branch.head, Some(&branch.name) != main.as_ref()));
        Ok(Refs {
            local,
            remote,
            tags,
            main,
        })
    }

    fn compare(&self, root: &Path, base: &str, head: &str) -> io::Result<Comparison> {
        let range = format!("{base}..{head}");
        let format = format!("--format={COMMIT}%x1e");
        let limit = format!("-n{COMPARED}");
        let out = checked(
            root,
            &["log", "--decorate=full", &format, &limit, &range, "--"],
        )?;
        let commits = read::commits(&String::from_utf8_lossy(&out.stdout), None);
        let behind = apart(root, base, head).map(|a| a.behind).unwrap_or(0);
        let parted = format!("{base}...{head}");
        let files = files(root, &[parted.as_str()], false)?;
        // A merge done in memory, written nowhere but the object store, lists what would stop.
        let tried = git(
            root,
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                base,
                head,
            ],
        )?;
        let conflicts = if tried.status.code() == Some(1) {
            String::from_utf8_lossy(&tried.stdout)
                .lines()
                .skip(1)
                .take_while(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        Ok(Comparison {
            base: base.to_owned(),
            head: head.to_owned(),
            commits,
            files,
            behind,
            conflicts,
        })
    }

    fn compare_patch(&self, root: &Path, base: &str, head: &str) -> io::Result<String> {
        let parted = format!("{base}...{head}");
        let out = checked(
            root,
            &["diff", "--no-color", "--no-ext-diff", "-M", &parted, "--"],
        )?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn stashes(&self, root: &Path) -> io::Result<Vec<Stash>> {
        let out = checked(root, &["stash", "list", "--format=%gd%x00%H%x00%ct%x00%gs"])?;
        Ok(read::stashes(&String::from_utf8_lossy(&out.stdout)))
    }

    fn stash_files(&self, root: &Path, index: usize) -> io::Result<Vec<FileStat>> {
        let name = format!("stash@{{{index}}}");
        let run = |kind: &str| -> io::Result<String> {
            let out = checked(
                root,
                &[
                    "stash",
                    "show",
                    "--include-untracked",
                    "-M",
                    "-z",
                    kind,
                    &name,
                ],
            )?;
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        };
        Ok(read::files(&run("--numstat")?, &run("--name-status")?))
    }

    fn stash_patch(&self, root: &Path, index: usize) -> io::Result<String> {
        let name = format!("stash@{{{index}}}");
        let out = checked(
            root,
            &[
                "stash",
                "show",
                "-p",
                "--include-untracked",
                "--no-color",
                "--no-ext-diff",
                "-M",
                &name,
            ],
        )?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn worktrees(&self, root: &Path) -> io::Result<Vec<Worktree>> {
        let out = checked(root, &["worktree", "list", "--porcelain", "-z"])?;
        Ok(read::worktrees(&String::from_utf8_lossy(&out.stdout)))
    }

    fn run(&self, root: &Path, op: &Op) -> io::Result<()> {
        let doing = self.repo(root)?.and_then(|repo| repo.doing().cloned());
        for call in op::plan(op, doing.as_ref())? {
            let out = write(root, &call)?;
            if !out.status.success() {
                return Err(failure(&out));
            }
        }
        Ok(())
    }
}

// Git to read with: no optional locks, nothing to answer, plain English.
fn git(dir: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("--no-optional-locks")
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
}

// A read that fails with what git said.
fn checked(dir: &Path, args: &[&str]) -> io::Result<Output> {
    let out = git(dir, args)?;
    if !out.status.success() {
        return Err(failure(&out));
    }
    Ok(out)
}

// Git to change things with. Nothing may wait on the dev at a prompt no one sees: no editor
// opens (merges and rebases keep the message git prepared), and a remote that wants a password
// or a host key fails at once with what it said.
fn write(dir: &Path, call: &op::Call) -> io::Result<Output> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=off"])
        .args(&call.args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    match &call.input {
        Some(input) => {
            command.stdin(Stdio::piped());
            let mut child = command.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes())?;
            }
            child.wait_with_output()
        }
        None => command.stdin(Stdio::null()).output(),
    }
}

fn failure(out: &Output) -> io::Error {
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    let (err, out) = (said(&out.stderr), said(&out.stdout));
    // A merge that stops on a conflict says so on its output, and only hints on its errors.
    let why = if out.contains("CONFLICT") || err.is_empty() {
        if out.is_empty() { err } else { out }
    } else {
        err
    };
    io::Error::other(why)
}

// The whole of a file git does not know yet, as a patch that adds it.
fn whole(root: &Path, inner: &str) -> io::Result<String> {
    let out = git(
        root,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-index",
            "--",
            "/dev/null",
            inner,
        ],
    )?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn untracked(root: &Path, inner: &str) -> io::Result<bool> {
    let out = git(
        root,
        &["ls-files", "--others", "--exclude-standard", "--", inner],
    )?;
    Ok(!out.stdout.is_empty())
}

// The files a range touched: `a b` for a commit against its parent, `a...b` for what b has since
// it parted from a, or a root commit alone.
fn files(root: &Path, range: &[&str], root_commit: bool) -> io::Result<Vec<FileStat>> {
    let run = |kind: &str| -> io::Result<String> {
        let mut args = if root_commit {
            vec!["diff-tree", "--root", "-r", "--no-commit-id"]
        } else {
            vec!["diff"]
        };
        args.extend(["-M", "-z", kind]);
        args.extend_from_slice(range);
        args.push("--");
        let out = checked(root, &args)?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    Ok(read::files(&run("--numstat")?, &run("--name-status")?))
}

// How far `head` went from `base`.
fn apart(root: &Path, base: &str, head: &str) -> io::Result<Apart> {
    let range = format!("{base}...{head}");
    let out = checked(root, &["rev-list", "--left-right", "--count", &range])?;
    let said = String::from_utf8_lossy(&out.stdout);
    let mut counts = said.split_whitespace().map(|n| n.parse().unwrap_or(0));
    let behind = counts.next().unwrap_or(0);
    let ahead = counts.next().unwrap_or(0);
    Ok(Apart { ahead, behind })
}

// The branch checked out, for telling it apart in the history.
fn head_branch(root: &Path) -> Option<String> {
    let out = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

// The branch a repository counts as its main one: the one its remote names as its own, kept
// here; else the usual names; else the one checked out.
fn main_branch(
    root: &Path,
    local: &[corvo_domain::Branch],
    remote: &[corvo_domain::Branch],
) -> Option<String> {
    let has = |name: &str| local.iter().any(|b| b.name == name);
    let out = git(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .ok();
    if let Some(out) = out.filter(|out| out.status.success()) {
        let remote_main = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let name = remote_main.split_once('/').map_or("", |(_, name)| name);
        if has(name) {
            return Some(name.to_owned());
        }
        if remote.iter().any(|b| b.name == remote_main) {
            return Some(remote_main);
        }
    }
    ["main", "master", "trunk", "develop"]
        .into_iter()
        .find(|name| has(name))
        .map(str::to_owned)
        .or_else(|| local.iter().find(|b| b.head).map(|b| b.name.clone()))
}

// The file's path inside the repository, as git writes it.
fn inside(root: &Path, file: &Path) -> Option<String> {
    let inner = file.strip_prefix(root).ok()?;
    let parts: Vec<_> = inner
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

// The path with the `..` of the way up taken out.
fn clean(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            part => clean.push(part),
        }
    }
    clean
}
