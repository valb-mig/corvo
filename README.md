# corvo

A small, fast code editor written in Rust on top of [GPUI Kit](https://github.com/longbridge/gpui-kit).

## Run

```sh
cargo run
```

## Workspaces

corvo opens on the list of workspaces, where you add, remove or enter one. A workspace links any number of project folders, none required.

Inside a workspace the window has the center content (the home or the code editor) and, on the right, the file tree of a project: the one open in the home, or the one the active tab belongs to. Projects are linked and unlinked from the home: right click the workspace card for **Link folder…**, or a project card for **Unlink**. Open tabs belong to the workspace, so switching projects only changes the tree.

A status bar runs along the bottom of the window. On the left, a button reorganizes the graph: it shows the home, puts every moved card back where the layout says, and fits the whole graph in view, centred, at 100% at most; next to it, the count of cards hidden from the home (see Home). On the right it shows the languages of the project open in the home, the logos of the three with the most files, in their brand colors (from [Simple Icons](https://simpleicons.org), CC0, in `crates/ui/assets/languages`; a color too dark for the dark theme is lifted); click it for all of them, each with its files and share. They are counted in the background when a project is opened in the home, from the extensions of its files, leaving out `node_modules`, `target`, `vendor`, `dist` and `build`; data, docs and settings (JSON, YAML, TOML, Markdown) are not counted as languages.

corvo draws its own title bar: a back button to the workspace list, the workspace name, and the window controls the compositor supports (Hyprland has no minimize, so it only shows maximize and close). Leaving a workspace or closing the window is refused while a tab has unsaved changes.

Files carry the icons of edit.r in the tree and the tabs: a seal in the tone of the file kind with the language letters (`Rs`, `TS`, `Py`), a glyph or a mark (React, Markdown, Claude), or a plain grey leaf. Folders get the same treatment: known folders (`src`, `tests`, `docs`, `.github`…) carry a glyph in the corner. The tables live in `corvo_ui::file_icon`.

Any file opens in a tab: text goes to the editor, images (png, jpg, webp, gif, svg, bmp, tiff, ico) render in a viewer, and other binary files show their size and a notice.

Workspaces are saved to `$XDG_CONFIG_HOME/corvo/workspaces.json` (`~/.config/corvo/workspaces.json` by default).

## Home

The first tab of a workspace, pinned and with no close, is its home: a tree read left to right. The workspace sits on the left with its linked projects; picking a project isolates it, standing it alone as the root (click it to go back to the workspace). Any number of its folders, files and symbols can be open at once: an open folder holds its contents in the next column, an open file its structure (types, functions, methods, fields…, from the same outline as the tree), an open symbol the symbols inside it, each with a curve from its card, lit to the open ones. An open card sits halfway between the first and the last card it holds, and each branch takes the height of all it holds, so the cards around it make room. Click an open card to close it with everything open inside. The map never leaves for the code, and reads deeper than the tree: a function also holds its parameters and local variables (destructured names, loop and closure variables, `$` kept in PHP), each name once, so names can be checked without opening the file. A file with no outline shows `No structure`.

An open symbol drops a panel down from under its own header, headed by its file; its column widens to hold it and the cards under it move down. A container (impl, class, module, interface) drops nothing, its members do:

- A function, method, struct, enum or type shows its code in a small editor of its own, only those lines. `Ctrl+S` writes the panel being typed in back over the same lines of the file; an open tab of it takes the new text. The panel is locked while the file has unsaved changes in a tab, and saving is refused if those lines changed on disk since they were read. Edits not yet saved stay with the panel when you close it, and come back with it.
- A variable, parameter, constant, field or variant shows each line that mentions it, with the name marked. A local looks only inside the function that holds it; anything else looks in the whole file. Comments and the text of strings don't count; a name interpolated in a string does.

Right click a card for its menu: **Hide** takes it, and all it holds, off the home; a folder whose cards are all hidden says `All hidden`. Hidden cards stay hidden while you isolate and open others. The status bar along the bottom of the window counts them: click it to see each hidden card with where it is, and bring one back with **Show**, or all with **Show all**.

Each folder card tells what it holds (`3 folders · 12 files`), and a folder with a project marker carries a badge with its stack. The markers come from `corvo_domain::server::Registry`; the first match in this order wins, so a Laravel app with both `composer.json` and `package.json` is PHP:

| Marker                                                   | Badge  |
| -------------------------------------------------------- | ------ |
| `Cargo.toml`                                             | Rust   |
| `composer.json`                                          | PHP    |
| `pom.xml`, `build.gradle(.kts)`, `settings.gradle(.kts)` | JVM    |
| `pyproject.toml`, `setup.py`, `requirements.txt`         | Python |
| `package.json`, `tsconfig.json`, `jsconfig.json`         | Node   |

The workspace opens on its home, and so does the window whenever no tab is left. Going back to the home reads the open folders and files from disk again.

The camera is free, with no edge to stop it: drag the background, or turn the wheel, to move the graph anywhere. Drag a card by its header to move it, with all it holds, wherever reads best; moved cards keep their place while others open and close, and the layout button at the bottom right (shown once something was moved), like the one on the status bar, puts every card back and centres the graph. Hold `Ctrl` and turn the wheel to zoom, around the mouse, or use the − / % / + at the bottom right (the % goes back to where it started, at 100%). A drag that ends on a card does not open or close it. A drag or the wheel inside a dropped panel stays in the panel. Code in a panel keeps its font size when zoomed.

## Outline

The active file opens its outline right below it in the tree: types, functions, methods, fields, constants and enum variants, nested as in the code. Click a symbol to put the cursor on it; click the file, or a symbol with children, to fold it.

The outline comes from tree-sitter (`crates/syntax`), with no language server needed, for Rust, JavaScript, TypeScript, TSX, Python, Java, PHP and Kotlin. It reads the file from disk, so unsaved edits show after the next save.

## Keys

| Key          | Command          | Scope  |
| ------------ | ---------------- | ------ |
| `ctrl-s`     | Save             | global |
| `ctrl-z`     | Undo             | editor |
| `ctrl-y`     | Redo             | editor |
| `ctrl-space` | Show completions | editor |
| `f12`        | Go to definition | editor |

`ctrl-click` also goes to definition. Bindings live in `corvo_domain::keymap::Keymap` and reach GPUI through `corvo_ui::keys`.

## Language servers

Completion, diagnostics, hover and go to definition come from language servers. Each one starts the first time a file of its language opens, rooted at the nearest project marker inside the linked project that holds the file. Servers that are not installed only show a notice.

| Language       | Server                       | Install                                   |
| -------------- | ---------------------------- | ----------------------------------------- |
| Rust           | rust-analyzer                | `pacman -S rust-analyzer`                 |
| JS / TS / TSX  | TypeScript 7 (`tsc --lsp`)   | `npm i -g typescript`                     |
| PHP            | phpactor                     | phar from the phpactor releases           |
| Java           | jdtls (needs JDK 21+)        | `yay -S jdtls`                            |
| Kotlin         | kotlin-lsp                   | tarball from the kotlin-lsp releases      |
| Python         | pyright                      | `npm i -g pyright`                        |
| TOML           | taplo                        | `pacman -S taplo-cli`                     |
| JSON/HTML/CSS  | vscode-langservers-extracted | `npm i -g vscode-langservers-extracted`   |
| Bash           | bash-language-server         | `npm i -g bash-language-server`           |
| YAML           | yaml-language-server         | `npm i -g yaml-language-server`           |

The list lives in `corvo_domain::server::Registry`. Server stderr goes to `~/.cache/corvo/lsp/<server>.log`.

Known limit: servers that only speak UTF-16 positions can misplace columns on lines that contain characters outside the Basic Multilingual Plane, such as emoji.

## Crates

| Crate            | Role                                                   |
| ---------------- | ------------------------------------------------------ |
| `crates/domain`  | Domain objects and ports, no UI or framework code      |
| `crates/fs`      | Local file system adapter for the `Fs` port            |
| `crates/lsp`     | Language server client: processes, JSON-RPC, protocol  |
| `crates/syntax`  | Code outline from tree-sitter for the `Syntax` port    |
| `crates/ui`      | Design system over GPUI Kit                            |
| `crates/corvo`   | The application: composition and panes                 |

## Requirements

Rust with edition 2024 support, Vulkan drivers, and Wayland or X11 development libraries.
