use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::port::{Depth, Fs, Syntax};
use crate::{
    Arrangement, Atlas, Content, Finder, Kind, Lang, Mark, Mention, Moved, Node, Place, Symbol,
    Tabs, Tree, excerpt, owner,
};

// A file or folder under its new name, and the tabs that followed it, each from its path before
// to the one after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    pub path: PathBuf,
    pub tabs: Vec<(PathBuf, PathBuf)>,
}

// A file read, waiting to be parsed into its outline; see `Workbench::outliner`.
pub struct Outliner {
    syntax: Arc<dyn Syntax + Send + Sync>,
    lang: Lang,
    text: Option<String>,
    depth: Depth,
}

impl Outliner {
    pub fn run(self) -> Vec<Symbol> {
        match &self.text {
            Some(text) => self.syntax.outline(self.lang, text, self.depth),
            None => Vec::new(),
        }
    }
}

pub struct Workbench {
    fs: Box<dyn Fs>,
    syntax: Arc<dyn Syntax + Send + Sync>,
    trees: HashMap<PathBuf, Tree>,
    project: Option<PathBuf>,
    tabs: Tabs,
    atlas: Atlas,
}

impl Workbench {
    pub fn new(fs: Box<dyn Fs>, syntax: Box<dyn Syntax + Send + Sync>) -> Self {
        Self {
            fs,
            syntax: Arc::from(syntax),
            trees: HashMap::new(),
            project: None,
            tabs: Tabs::new(),
            atlas: Atlas::default(),
        }
    }

    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.trees.get(self.project.as_ref()?)
    }

    pub fn tabs(&self) -> &Tabs {
        &self.tabs
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    // For what changes only how the home is arranged, with nothing to read.
    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub fn show(&mut self, project: Option<&Path>) -> io::Result<()> {
        let Some(root) = project else {
            self.project = None;
            return Ok(());
        };
        if !self.trees.contains_key(root) {
            let mut tree = Tree::new(root);
            tree.fill(root, self.fs.list(root)?);
            tree.open(root);
            self.trees.insert(root.to_path_buf(), tree);
        }
        self.project = Some(root.to_path_buf());
        Ok(())
    }

    pub fn forget(&mut self, project: &Path) {
        self.trees.remove(project);
        if self.project.as_deref() == Some(project) {
            self.project = None;
        }
    }

    pub fn expand(&mut self, dir: &Path) -> io::Result<()> {
        self.load(dir)?;
        if let Some(tree) = self.tree_mut() {
            tree.open(dir);
        }
        Ok(())
    }

    pub fn collapse(&mut self, dir: &Path) {
        if let Some(tree) = self.tree_mut() {
            tree.close(dir);
        }
    }

    pub fn reveal(&mut self, path: &Path) -> io::Result<()> {
        let Some(root) = self.project.clone() else {
            return Ok(());
        };
        let mut dirs: Vec<_> = path
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(&root))
            .map(Path::to_path_buf)
            .collect();
        dirs.reverse();
        for dir in dirs {
            self.expand(&dir)?;
        }
        Ok(())
    }

    pub fn open(&mut self, path: &Path) -> io::Result<Option<Content>> {
        if self.tabs.contains(path) {
            self.tabs.focus(path);
            return Ok(None);
        }
        let content = if Content::is_image(path) {
            Content::Image
        } else {
            Content::from_bytes(self.fs.read(path)?)
        };
        self.tabs.open(path);
        Ok(Some(content))
    }

    pub fn focus(&mut self, path: &Path) {
        self.tabs.focus(path);
    }

    pub fn close(&mut self, path: &Path) {
        self.tabs.close(path);
    }

    pub fn touch(&mut self, path: &Path) {
        self.tabs.touch(path);
    }

    // The tab's text is back to what is on disk.
    pub fn clean(&mut self, path: &Path) {
        self.tabs.clean(path);
    }

    pub fn save(&mut self, path: &Path, text: &str) -> io::Result<()> {
        self.fs.write(path, text)?;
        self.tabs.clean(path);
        Ok(())
    }

    // Makes an empty file, or a folder, named `name` in `dir`, and opens `dir` in the tree so it
    // shows; where it is.
    pub fn create(&mut self, dir: &Path, name: &str, kind: Kind) -> io::Result<PathBuf> {
        let path = dir.join(checked(name)?);
        match kind {
            Kind::File => self.fs.create_file(&path)?,
            Kind::Dir => self.fs.create_dir(&path)?,
        }
        self.uncover(&[moved(&path)]);
        // It is made: a folder above it that can't be shown open is no reason to say otherwise.
        self.expand(dir).ok();
        Ok(path)
    }

    // Gives a file or folder another name in the same folder. The tabs of what it held follow
    // it, and so do the folders open in the tree.
    pub fn rename(&mut self, path: &Path, name: &str) -> io::Result<Renamed> {
        self.refuse_root(path, "renamed")?;
        let Some(dir) = path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "it has no folder to stay in",
            ));
        };
        let to = dir.join(checked(name)?);
        if to == path {
            return Ok(Renamed {
                path: to,
                tabs: Vec::new(),
            });
        }
        self.fs.rename(path, &to)?;
        let tabs = self.tabs.moved(path, &to);
        let open = self
            .tree_mut()
            .map(|tree| tree.moved(path, &to))
            .unwrap_or_default();
        self.uncover(&[moved(path), moved(&to)]);
        for dir in open {
            self.expand(&dir).ok();
        }
        Ok(Renamed { path: to, tabs })
    }

    // Sends a file or folder to the trash; the tabs of what it held close. The paths of those.
    pub fn trash(&mut self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.refuse_root(path, "sent to the trash")?;
        self.fs.trash(path)?;
        for tree in self.trees.values_mut() {
            tree.forget(path);
        }
        let closed = self.tabs.close_under(path);
        self.uncover(&[moved(path)]);
        Ok(closed)
    }

    // Replaces what the finder finds in a file on disk; how many it replaced. A file where it
    // finds nothing is left as it was.
    pub fn replace(&mut self, path: &Path, finder: &Finder, with: &str) -> io::Result<usize> {
        let text = self.text(path)?;
        let (text, count) = finder.replace(&text, with);
        if count > 0 {
            self.fs.write(path, &text)?;
        }
        Ok(count)
    }

    pub fn chart(&mut self, projects: &[PathBuf]) -> io::Result<()> {
        self.atlas.chart(projects);
        for project in projects {
            // A project that can't be read just shows nothing inside.
            self.list(project).ok();
        }
        if let Some(root) = self.atlas.root().map(Path::to_path_buf)
            && let Err(err) = self.survey(&root)
        {
            self.atlas.isolate(None);
            return Err(err);
        }
        // What can no longer be read closes; the first failure is told.
        let mut failed = None;
        let opened: Vec<Node> = self.atlas.opened().cloned().collect();
        for node in opened {
            if let Err(err) = self.load_atlas(&node) {
                self.atlas.close(node.path());
                failed.get_or_insert(err);
            }
        }
        failed.map_or(Ok(()), Err)
    }

    // Brings the home back as it was left, with what can still be read; the rest doesn't come
    // back.
    pub fn restore(&mut self, projects: &[PathBuf], left: &Arrangement) {
        self.atlas.chart(projects);
        let root = left
            .root
            .as_deref()
            .filter(|root| projects.iter().any(|project| project == root));
        if let Some(root) = root
            && self.survey(root).is_ok()
        {
            self.atlas.isolate(Some(root));
            for node in left
                .open
                .iter()
                .filter(|node| node.path().starts_with(root))
            {
                if self.load_atlas(node).is_ok() {
                    self.atlas.open(node.clone());
                }
            }
            for (file, trail) in &left.dives {
                if let Some(at) = self.atlas.find(file, trail) {
                    self.atlas.dive(file, &at);
                }
            }
            for dir in left.spread.iter().filter(|dir| dir.starts_with(root)) {
                self.atlas.spread(dir);
            }
        }
        for (mark, name) in &left.hidden {
            let place = match mark {
                Mark::Node(path) => Some(Place::Node(path.clone())),
                Mark::Symbol(file, trail) => {
                    // The file may be closed: its outline is read to find the symbol.
                    self.load_atlas(&Node::file(file)).ok();
                    self.atlas
                        .find(file, trail)
                        .map(|at| Place::Symbol(file.clone(), at))
                }
            };
            if let Some(place) = place {
                self.atlas.hide(place, name);
            }
        }
        self.atlas
            .rearrange(left.nudges.clone(), left.groups.clone());
    }

    // Brings files changed on disk into the home and the file trees. The folders above each one
    // that were read are read again, so what came shows and what went goes; on the home, inside
    // the project open there, those above a file to show open down to it. A folder that can't be
    // read any more closes. Says whether a folder opened.
    pub fn uncover(&mut self, moved: &[Moved]) -> bool {
        let root = self.atlas.root().map(Path::to_path_buf);
        let mut reread = BTreeSet::new();
        let mut opening = BTreeSet::new();
        for Moved { path, shown } in moved {
            for dir in path.ancestors().skip(1) {
                reread.insert(dir.to_path_buf());
                if *shown
                    && let Some(root) = &root
                    && dir.starts_with(root)
                    && dir != root
                {
                    opening.insert(dir.to_path_buf());
                }
            }
        }
        self.reread_trees(&reread);
        let Some(root) = root else {
            return false;
        };
        let mut opened = false;
        // Folders before what is inside them.
        for dir in reread.iter().filter(|dir| dir.starts_with(&root)) {
            let open = opening.contains(dir);
            if !open && !self.atlas.is_loaded(dir) {
                continue;
            }
            let Ok(nodes) = self.fs.list(dir) else {
                self.atlas.close(dir);
                continue;
            };
            self.atlas.fill(dir, nodes);
            // A folder that came shows what it holds, as one surveyed does.
            let fresh: Vec<PathBuf> = self
                .atlas
                .children(dir)
                .unwrap_or_default()
                .iter()
                .filter(|node| node.is_dir() && !self.atlas.is_loaded(node.path()))
                .map(|node| node.path().to_path_buf())
                .collect();
            for fresh in fresh {
                self.list(&fresh).ok();
            }
            if open && !self.atlas.is_open(dir) {
                self.atlas.open(Node::dir(dir));
                opened = true;
            }
        }
        opened
    }

    // Each file tree reads again the folders it had read among these.
    fn reread_trees(&mut self, dirs: &BTreeSet<PathBuf>) {
        for tree in self.trees.values_mut() {
            for dir in dirs {
                if !tree.is_loaded(dir) {
                    continue;
                }
                match self.fs.list(dir) {
                    Ok(nodes) => tree.fill(dir, nodes),
                    Err(_) => tree.close(dir),
                }
            }
        }
    }

    pub fn rechart(&mut self) -> io::Result<()> {
        self.atlas.clear();
        let projects = self.atlas.projects().to_vec();
        self.chart(&projects)
    }

    pub fn isolate(&mut self, root: Option<&Path>) -> io::Result<()> {
        if let Some(root) = root {
            self.survey(root)?;
        }
        self.atlas.isolate(root);
        Ok(())
    }

    // Brings a file into sight on the home: its project stands alone, unless the one standing
    // alone already holds it; the folders down to it open; the folder it is in shows all its
    // files, out of their piles; and what hid it, or a folder above it, shows again. Says whether
    // the file is in one of the projects charted.
    pub fn show_on_home(&mut self, path: &Path) -> io::Result<bool> {
        let projects = self.atlas.projects().to_vec();
        let Some(project) = owner(path, &projects).map(Path::to_path_buf) else {
            return Ok(false);
        };
        let held = self.atlas.root().is_some_and(|root| path.starts_with(root));
        if !held {
            self.isolate(Some(&project))?;
        }
        let root = self.atlas.root().map(Path::to_path_buf).unwrap_or(project);
        let mut dirs: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(&root) && *dir != root)
            .map(Path::to_path_buf)
            .collect();
        dirs.reverse();
        for dir in dirs {
            self.survey(&dir)?;
            if !self.atlas.is_open(&dir) {
                self.atlas.open(Node::dir(dir));
            }
        }
        if let Some(dir) = path.parent() {
            self.list(dir)?;
            self.atlas.spread(dir);
        }
        for place in path
            .ancestors()
            .take_while(|at| at.starts_with(&root))
            .map(|at| Place::Node(at.to_path_buf()))
        {
            self.atlas.show(&place);
        }
        Ok(true)
    }

    // Opens a folder or file of the atlas, or closes it with everything open inside.
    pub fn toggle(&mut self, node: &Node) -> io::Result<()> {
        if self.atlas.is_open(node.path()) {
            self.atlas.close(node.path());
            return Ok(());
        }
        self.load_atlas(node)?;
        self.atlas.open(node.clone());
        Ok(())
    }

    pub fn hide(&mut self, place: Place, name: &str) {
        self.atlas.hide(place, name);
    }

    pub fn show_hidden(&mut self, place: Option<&Place>) {
        match place {
            Some(place) => self.atlas.show(place),
            None => self.atlas.show_all(),
        }
    }

    // Opens a symbol of an open file, or closes it with every symbol open inside.
    pub fn toggle_symbol(&mut self, file: &Path, at: &[usize]) {
        if self.atlas.is_dived(file, at) {
            self.atlas.surface(file, at);
        } else {
            self.atlas.dive(file, at);
        }
    }

    pub fn outlines(&self, path: &Path) -> bool {
        self.syntax.outlines(Lang::of(path))
    }

    pub fn outline(&self, path: &Path, depth: Depth) -> io::Result<Vec<Symbol>> {
        Ok(self.outliner(path, depth)?.run())
    }

    // The outline of a file to work out away from the workbench: the file is read now, and
    // parsed when `run` is called, on any thread. A file in a language with no outline is not
    // read, and gives none.
    pub fn outliner(&self, path: &Path, depth: Depth) -> io::Result<Outliner> {
        let lang = Lang::of(path);
        let text = if self.syntax.outlines(lang) {
            Some(self.text(path)?)
        } else {
            None
        };
        Ok(Outliner {
            syntax: self.syntax.clone(),
            lang,
            text,
            depth,
        })
    }

    pub fn excerpt(&self, path: &Path, first: u32, last: u32) -> io::Result<String> {
        let text = self.text(path)?;
        let span = excerpt::span(&text, first, last).ok_or_else(|| outside(first, last))?;
        Ok(text[span].to_owned())
    }

    // Writes `edited` over the lines that held `original`, and gives back the whole new text.
    // Refused when those lines no longer hold `original`.
    pub fn splice(
        &mut self,
        path: &Path,
        first: u32,
        last: u32,
        original: &str,
        edited: &str,
    ) -> io::Result<String> {
        let text = self.text(path)?;
        let span = excerpt::span(&text, first, last).ok_or_else(|| outside(first, last))?;
        if text[span.clone()] != *original {
            return Err(io::Error::other("the file changed since it was read"));
        }
        let new = format!("{}{edited}{}", &text[..span.start], &text[span.end..]);
        self.fs.write(path, &new)?;
        Ok(new)
    }

    // The lines where the name comes, inside the given lines or in the whole file.
    pub fn mentions(
        &self,
        path: &Path,
        name: &str,
        lines: Option<(u32, u32)>,
    ) -> io::Result<Vec<Mention>> {
        let lang = Lang::of(path);
        let text = self.text(path)?;
        let within = match lines {
            Some((first, last)) => {
                excerpt::span(&text, first, last).ok_or_else(|| outside(first, last))?
            }
            None => 0..text.len(),
        };
        let ranges = self.syntax.mentions(lang, &text, name, within);
        Ok(excerpt::mentions(&text, &ranges))
    }

    pub fn text(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.fs.read(path)?)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    fn load(&mut self, dir: &Path) -> io::Result<()> {
        let Some(root) = &self.project else {
            return Ok(());
        };
        let Some(tree) = self.trees.get_mut(root) else {
            return Ok(());
        };
        if !tree.is_loaded(dir) {
            tree.fill(dir, self.fs.list(dir)?);
        }
        Ok(())
    }

    fn load_atlas(&mut self, node: &Node) -> io::Result<()> {
        let path = node.path();
        if node.is_dir() {
            return self.survey(path);
        }
        if !self.atlas.is_traced(path) {
            let symbols = self.outline(path, Depth::Locals)?;
            self.atlas.trace(path, symbols);
        }
        Ok(())
    }

    // A folder of the atlas, with the folders inside it, so each of their cards can tell what
    // they hold.
    fn survey(&mut self, dir: &Path) -> io::Result<()> {
        self.list(dir)?;
        let dirs: Vec<PathBuf> = self
            .atlas
            .children(dir)
            .unwrap_or_default()
            .iter()
            .filter(|node| node.is_dir())
            .map(|node| node.path().to_path_buf())
            .collect();
        for dir in dirs {
            // A folder that can't be read just shows nothing inside.
            self.list(&dir).ok();
        }
        Ok(())
    }

    fn list(&mut self, dir: &Path) -> io::Result<()> {
        if !self.atlas.is_loaded(dir) {
            self.atlas.fill(dir, self.fs.list(dir)?);
        }
        Ok(())
    }

    fn tree_mut(&mut self) -> Option<&mut Tree> {
        self.trees.get_mut(self.project.as_ref()?)
    }

    // A project's own folder stays as it is: it is linked, not a file to work on.
    fn refuse_root(&self, path: &Path, what: &str) -> io::Result<()> {
        if self.trees.contains_key(path) || self.atlas.projects().iter().any(|p| p == path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("a project's own folder can't be {what} here"),
            ));
        }
        Ok(())
    }
}

fn moved(path: &Path) -> Moved {
    Moved {
        path: path.to_path_buf(),
        shown: false,
    }
}

// A name for a file or folder, without the blanks around it; refused when it would land
// elsewhere, or nowhere.
fn checked(name: &str) -> io::Result<&str> {
    let name = name.trim();
    let fault = if name.is_empty() {
        "a name can't be empty"
    } else if name == "." || name == ".." {
        "that is not a name"
    } else if name.contains('/') || name.contains('\0') {
        "a name can't hold a /"
    } else {
        return Ok(name);
    };
    Err(io::Error::new(io::ErrorKind::InvalidInput, fault))
}

fn outside(first: u32, last: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("lines {}–{} are not in the file", first + 1, last + 1),
    )
}
