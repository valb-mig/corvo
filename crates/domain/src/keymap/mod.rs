mod binding;
mod command;
mod scope;

pub use binding::Binding;
pub use command::Command;
pub use scope::Scope;

#[derive(Clone, Debug)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn new(bindings: Vec<Binding>) -> Self {
        Self { bindings }
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn bind(&mut self, binding: Binding) {
        self.bindings
            .retain(|b| !(b.chord() == binding.chord() && b.scope() == binding.scope()));
        self.bindings.push(binding);
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new(vec![
            Binding::new("ctrl-s", Command::Save, Scope::Global),
            Binding::new("ctrl-z", Command::Undo, Scope::Editor),
            Binding::new("ctrl-y", Command::Redo, Scope::Editor),
            Binding::new("ctrl-space", Command::Complete, Scope::Editor),
            Binding::new("f12", Command::Definition, Scope::Editor),
            Binding::new("ctrl-shift-e", Command::ShowCode, Scope::Global),
            Binding::new("ctrl-shift-g", Command::ShowGit, Scope::Global),
            Binding::new("ctrl-shift-c", Command::ShowChat, Scope::Global),
            Binding::new("ctrl-shift-j", Command::ShowTerminal, Scope::Global),
            Binding::new("ctrl-shift-f", Command::FindInFiles, Scope::Global),
            Binding::new("ctrl-k", Command::OpenPalette, Scope::Global),
            Binding::new("escape", Command::Dismiss, Scope::Global),
            Binding::new("ctrl-b", Command::TogglePanel, Scope::Global),
            Binding::new("up", Command::PickPrevious, Scope::Workspaces),
            Binding::new("down", Command::PickNext, Scope::Workspaces),
            Binding::new("enter", Command::OpenPicked, Scope::Workspaces),
            Binding::new("delete", Command::RemovePicked, Scope::Workspaces),
            Binding::new("f2", Command::RenamePicked, Scope::Workspaces),
            Binding::new("ctrl-n", Command::NewWorkspace, Scope::Workspaces),
            Binding::new("f2", Command::RenamePicked, Scope::Tree),
            Binding::new("delete", Command::RemovePicked, Scope::Tree),
        ])
    }
}
