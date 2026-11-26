use corvo_domain::{Change, HunkMark};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Hsla};

// The color of a file by how it stands against the last commit: green for what is new, blue
// for what changed, red for what is gone, amber for a conflict.
pub fn tint(change: Change, cx: &App) -> Hsla {
    let theme = cx.theme();
    match change {
        Change::Added | Change::Untracked => theme.green,
        Change::Modified | Change::Renamed => theme.blue,
        Change::Deleted => theme.red,
        Change::Conflicted => theme.warning,
    }
}

// The color of lines by what was done to them.
pub fn mark_tint(mark: HunkMark, cx: &App) -> Hsla {
    let theme = cx.theme();
    match mark {
        HunkMark::Added => theme.green,
        HunkMark::Modified => theme.blue,
        HunkMark::Deleted => theme.red,
    }
}
