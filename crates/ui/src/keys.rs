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
        OpenPalette,
        Dismiss,
        ToggleTree
    ]
);

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
        Command::OpenPalette => Box::new(OpenPalette),
        Command::Dismiss => Box::new(Dismiss),
        Command::ToggleTree => Box::new(ToggleTree),
    }
}

fn context(scope: Scope) -> Option<&'static str> {
    match scope {
        Scope::Global => None,
        Scope::Editor => Some("Input && mode == editor"),
        Scope::Tree => Some(gpui_kit::base::tree_key_context()),
    }
}
