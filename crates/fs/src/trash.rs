// The trash of the dev's home, as the freedesktop.org spec keeps it: what goes there is moved
// whole into files/, with a note in info/ of where it was and when it went, so a file manager
// can put it back.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// The error Linux gives for a move between two file systems.
const EXDEV: i32 = 18;

// Where the trash of the home is: under XDG_DATA_HOME, or ~/.local/share.
pub fn home() -> io::Result<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home to keep a trash in"))?;
    Ok(data.join("Trash"))
}

// Moves a file or folder into the trash at `trash`, under a name no other there has, with the
// note of when it went, `now` as the spec writes it; the path it has in the trash. One on
// another file system than the trash stays where it is.
pub fn send(path: &Path, trash: &Path, now: &str) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "that can't go to the trash"))?;
    let (files, info) = (trash.join("files"), trash.join("info"));
    fs::create_dir_all(&files)?;
    fs::create_dir_all(&info)?;

    // The note is made first, never over another, so two moves never take the same name.
    let (kept, note, mut file) = (1..)
        .map(|n| keep_as(name.as_bytes(), n))
        .find_map(|kept| {
            if files.join(&kept).symlink_metadata().is_ok() {
                return None;
            }
            let note = info.join(format!("{kept}.trashinfo"));
            match OpenOptions::new().write(true).create_new(true).open(&note) {
                Ok(file) => Some(Ok((kept, note, file))),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => None,
                Err(err) => Some(Err(err)),
            }
        })
        .expect("a free name is always found")?;
    let text = format!(
        "[Trash Info]\nPath={}\nDeletionDate={now}\n",
        encode(path.as_os_str().as_bytes())
    );
    if let Err(err) = file.write_all(text.as_bytes()) {
        fs::remove_file(&note).ok();
        return Err(err);
    }

    let into = files.join(&kept);
    if let Err(err) = fs::rename(&path, &into) {
        fs::remove_file(&note).ok();
        if err.raw_os_error() == Some(EXDEV) {
            return Err(io::Error::other(
                "it is on another disk than the trash, so it was left where it is",
            ));
        }
        return Err(err);
    }
    Ok(into)
}

// The time now, as the trash notes it: local, to the second, with no zone.
pub fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs()) as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given, which lives through the call.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

// The name kept in the trash: the first as it is, the next ones with a number before the
// extension, as `notes.2.txt`.
fn keep_as(name: &[u8], n: usize) -> String {
    let name = String::from_utf8_lossy(name);
    if n == 1 {
        return name.into_owned();
    }
    match name.rfind('.').filter(|&dot| dot > 0) {
        Some(dot) => format!("{}.{n}{}", &name[..dot], &name[dot..]),
        None => format!("{name}.{n}"),
    }
}

// A path as a trash note writes it: every byte but the plain ones and `/` as %XX.
fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
