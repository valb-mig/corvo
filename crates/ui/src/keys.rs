use std::rc::Rc;

use corvo_domain::keymap::{Binding, Command, Keymap, Scope};
use gpui_kit::component::input::{GoToDefinition, Redo, ShowCompletions, Undo};
use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

actions!(
    corvo,
    [
        Save,
        ShowCode,
        ShowGit,
        ShowChat,
        ShowTerminal,
        OpenPalette,
        Dismiss,
        TogglePanel,
        PickPrevious,
        PickNext,
        OpenPicked,
        RemovePicked,
        RenamePicked,
        NewWorkspace,
        FindInFiles,
        GitChanges,
        GitHistory,
        GitBranches,
        GitStash,
        GitWorktrees,
        GitConflicts,
        GitPreviousRepo,
        GitNextRepo
    ]
);

// The key context of the list of workspaces.
pub const WORKSPACES: &str = "Workspaces";

pub fn bind(keymap: &Keymap, cx: &mut App) {
    let bindings: Vec<_> = keymap
        .bindings()
        .iter()
        .filter_map(|b| binding(b, cx))
        .collect();
    cx.bind_keys(bindings);
}

fn binding(b: &Binding, cx: &App) -> Option<KeyBinding> {
    let context = context(b.scope())
        .and_then(|ctx| KeyBindingContextPredicate::parse(ctx).ok())
        .map(Rc::new);
    KeyBinding::load(b.chord(), action(b.command()), context, false, None, cx).ok()
}

fn action(command: Command) -> Box<dyn Action> {
    match command {
        Command::Save => Box::new(Save),
        Command::Undo => Box::new(Undo),
        Command::Redo => Box::new(Redo),
        Command::Complete => Box::new(ShowCompletions),
        Command::Definition => Box::new(GoToDefinition),
        Command::ShowCode => Box::new(ShowCode),
        Command::ShowGit => Box::new(ShowGit),
        Command::ShowChat => Box::new(ShowChat),
        Command::ShowTerminal => Box::new(ShowTerminal),
        Command::OpenPalette => Box::new(OpenPalette),
        Command::Dismiss => Box::new(Dismiss),
        Command::TogglePanel => Box::new(TogglePanel),
        Command::PickPrevious => Box::new(PickPrevious),
        Command::PickNext => Box::new(PickNext),
        Command::OpenPicked => Box::new(OpenPicked),
        Command::RemovePicked => Box::new(RemovePicked),
        Command::RenamePicked => Box::new(RenamePicked),
        Command::NewWorkspace => Box::new(NewWorkspace),
        Command::FindInFiles => Box::new(FindInFiles),
        Command::GitChanges => Box::new(GitChanges),
        Command::GitHistory => Box::new(GitHistory),
        Command::GitBranches => Box::new(GitBranches),
        Command::GitStash => Box::new(GitStash),
        Command::GitWorktrees => Box::new(GitWorktrees),
        Command::GitConflicts => Box::new(GitConflicts),
        Command::GitPreviousRepo => Box::new(GitPreviousRepo),
        Command::GitNextRepo => Box::new(GitNextRepo),
    }
}

fn context(scope: Scope) -> Option<&'static str> {
    match scope {
        Scope::Global => None,
        Scope::Editor => Some("Input && mode == editor"),
        Scope::Tree => Some(gpui_kit::base::tree_key_context()),
        Scope::Workspaces => Some(WORKSPACES),
    }
}
