use std::path::PathBuf;

use lsp_types::Diagnostic;

#[derive(Debug)]
pub enum Note {
    Diagnostics(PathBuf, Vec<Diagnostic>),
    Missing { server: String, hint: String },
    Failed { server: String, message: String },
    Exited { server: String, root: PathBuf },
}
