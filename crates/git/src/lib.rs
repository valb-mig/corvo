mod status;

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use corvo_domain::Repo;
use corvo_domain::port::Git;

// Git as the dev has it installed, run once per question. It takes no optional locks, so it
// never stands in the way of the git the dev or an agent runs meanwhile.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitCli;

impl Git for GitCli {
    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>> {
        // The way up to the top of the repository, so the root keeps the folder's own spelling.
        let up = git(dir, &["rev-parse", "--show-cdup"])?;
        if !up.status.success() {
            return Ok(None);
        }
        let up = String::from_utf8_lossy(&up.stdout).trim().to_owned();
        let root = clean(&dir.join(up));
        let out = git(
            &root,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
        )?;
        if !out.status.success() {
            return Err(failure(&out));
        }
        Ok(Some(status::parse(&root, &out.stdout)))
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
        let out = git(
            root,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-index",
                "--",
                "/dev/null",
                &inner,
            ],
        )?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

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

fn failure(out: &Output) -> io::Error {
    io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_owned())
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
