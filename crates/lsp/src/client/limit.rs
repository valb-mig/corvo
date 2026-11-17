use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn command(program: &str, mb: u64) -> io::Result<Command> {
    let path = find(program).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    if !systemd() {
        return Ok(Command::new(path));
    }
    let mut cmd = Command::new("systemd-run");
    cmd.args([
        "--user",
        "--scope",
        "--quiet",
        "--collect",
        "--expand-environment=no",
    ])
    .arg(format!("-pMemoryMax={mb}M"))
    .arg(format!("-pMemoryHigh={}M", mb * 4 / 5))
    .arg("-pMemorySwapMax=512M")
    .arg("--")
    .arg(path);
    Ok(cmd)
}

fn systemd() -> bool {
    let bus = std::env::var_os("XDG_RUNTIME_DIR")
        .map(|dir| Path::new(&dir).join("bus").exists())
        .unwrap_or(false);
    bus && find("systemd-run").is_some()
}

fn find(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return runnable(Path::new(program)).then(|| PathBuf::from(program));
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|path| runnable(path))
}

fn runnable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}
