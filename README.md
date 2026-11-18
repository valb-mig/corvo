# corvo

A small, fast code editor written in Rust on top of [GPUI Kit](https://github.com/longbridge/gpui-kit).

## Run

```sh
cargo run
```

## Workspaces

corvo opens on the list of workspaces, where you add, remove or enter one. A workspace links any number of project folders, none required.

Inside a workspace the window has the center content (today the code editor) and, on the right, the file tree of the selected project. The **Projects** dropdown above the tree lists the linked projects: pick one to show its tree, link a folder, or unlink with ×. Open tabs belong to the workspace, so switching projects only changes the tree; when the active tab belongs to another linked project, the tree switches to it.

corvo draws its own title bar: a back button to the workspace list, the workspace name, and the window controls the compositor supports (Hyprland has no minimize, so it only shows maximize and close). Leaving a workspace or closing the window is refused while a tab has unsaved changes.

Any file opens in a tab: text goes to the editor, images (png, jpg, webp, gif, svg, bmp, tiff, ico) render in a viewer, and other binary files show their size and a notice.

Workspaces are saved to `$XDG_CONFIG_HOME/corvo/workspaces.json` (`~/.config/corvo/workspaces.json` by default).

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
| `crates/ui`      | Design system over GPUI Kit                            |
| `crates/corvo`   | The application: composition and panes                 |

## Requirements

Rust with edition 2024 support, Vulkan drivers, and Wayland or X11 development libraries.
