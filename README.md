# corvo

A small, fast code editor written in Rust on top of [GPUI Kit](https://github.com/longbridge/gpui-kit).

## Run

```sh
cargo run
```

## Workspaces

corvo opens on the list of workspaces, where you add, remove or enter one. A workspace links any number of project folders, none required.

Inside a workspace the window holds its home. The code comes over it, with the home dimmed behind: the open files in tabs, and on the right a sidebar with the file tree of a project (the one open in the home, or the one the active tab belongs to) over the structure of the active file (see Outline). A click on a file's card in the home opens it there, and so does a file picked in the tree; **×**, `Escape` or a click on the dimmed home puts the code away, its tabs kept for the next time. Closing the last tab leaves the code open, empty, with a note to pick a file in the tree. Projects are linked and unlinked from the home: the **Link folder…** button of the status bar links one, and right clicking a project card gives **Unlink**. Open tabs belong to the workspace, so switching projects only changes the tree.

A status bar runs along the bottom of the window. On the left, the code button brings the code over the home with the tabs left open, or with none and a note to pick a file in the tree; with the code shown, it puts it away. Next to it, a button links a folder to the workspace, then the count of cards hidden from the home (see Home). On the right it shows the languages of the project open in the home, the logos of the three with the most files, in their brand colors (from [Simple Icons](https://simpleicons.org), CC0, in `crates/ui/assets/languages`; a color too dark for the dark theme is lifted); click it for all of them, each with its files and share. They are counted in the background when a project is opened in the home, from the extensions of its files, leaving out `node_modules`, `target`, `vendor`, `dist` and `build`; data, docs and settings (JSON, YAML, TOML, Markdown) are not counted as languages.

corvo draws its own title bar: a back button to the workspace list, the workspace name, and the window controls the compositor supports (Hyprland has no minimize, so it only shows maximize and close). Leaving a workspace or closing the window is refused while a tab has unsaved changes.

Files carry the icons of edit.r in the tree and the tabs: a seal in the tone of the file kind with the language letters (`Rs`, `TS`, `Py`), a glyph or a mark (React, Markdown, Claude), or a plain grey leaf. Folders get the same treatment: known folders (`src`, `tests`, `docs`, `.github`…) carry a glyph in the corner. The tables live in `corvo_ui::file_icon`.

Any file opens in a tab of the code: text goes to the editor, images (png, jpg, webp, gif, svg, bmp, tiff, ico) render in a viewer, and other binary files show their size and a notice.

Workspaces are saved to `$XDG_CONFIG_HOME/corvo/workspaces.json` (`~/.config/corvo/workspaces.json` by default), each with how its home was left (see Home).

## Home

The home of a workspace is a tree read left to right. The linked projects stand down the left column, with no card for the workspace itself; picking a project isolates it, standing it alone as the root (click it to go back to all the projects). With none linked, a `No projects linked` card links a folder when clicked. Any number of its folders can be open at once: an open folder holds its contents in the next column, with a curve from its card, lit to the open ones. A click on a folder opens it; a click on a file opens its code. An open card sits halfway between the first and the last card it holds, and each branch takes the height of all it holds, so the cards around it make room. Click an open folder to close it with everything open inside.

Right click a card for its menu: **Hide** takes it, and all it holds, off the home; a folder whose cards are all hidden says `All hidden`. Hidden cards stay hidden while you isolate and open others. The status bar along the bottom of the window counts them, and hidden groups: click it to see each hidden card with where it is, and each hidden group with how many cards it has, and bring one back with **Show**, or all with **Show all**.

Pick cards to act on together: `Ctrl` or `Shift` and click a card to pick it, or let it go, or hold `Shift` and drag on the background to draw a box that picks every card it touches. A click on the background lets them go. Drag a picked card and the others go with it. Right click one of them for **Group…**, which puts them together and opens the group's title to type its name in (`Enter` or clicking away keeps it), or **Hide selected**. A group is a frame around its cards and all they hold, with its title on top: drag the title to move them all; a card dragged on its own stretches the frame. Right click the title to **Rename**, **Collapse** (the group becomes one card where its cards were, which a click expands again), **Hide group** or **Ungroup**. A card belongs to one group at most; **Remove from group** on its menu takes it out, and a group left with no card goes.

Each folder card tells what it holds (`3 folders · 12 files`), and a folder with a project marker carries a badge with its stack. The markers come from `corvo_domain::server::Registry`; the first match in this order wins, so a Laravel app with both `composer.json` and `package.json` is PHP:

| Marker                                                   | Badge  |
| -------------------------------------------------------- | ------ |
| `Cargo.toml`                                             | Rust   |
| `composer.json`                                          | PHP    |
| `pom.xml`, `build.gradle(.kts)`, `settings.gradle(.kts)` | JVM    |
| `pyproject.toml`, `setup.py`, `requirements.txt`         | Python |
| `package.json`, `tsconfig.json`, `jsconfig.json`         | Node   |

The workspace opens on its home. Putting the code away reads the open folders from disk again. The home comes back as it was left: the project isolated, the folders open, the cards and groups hidden, the cards moved and the groups; what can no longer be read just doesn't come back.

The camera is free, with no edge to stop it: drag the background, or turn the wheel, to move the graph anywhere. Drag a card by its header to move it, with all it holds, wherever reads best; moved cards keep their place while others open and close, and the layout button at the top left (shown once something was moved) puts every card back and centres the graph; groups stay, their frames around their cards where the layout puts them. Hold `Ctrl` and turn the wheel to zoom, around the mouse, or use the − / % / + at the top left (the % goes back to where it started, at 100%). A drag that ends on a card does not open or close it.

## Outline

The structure card at the foot of the editor's sidebar shows the outline of the active file: types, functions, methods, fields, constants and enum variants, nested as in the code. It follows the file picked in the tree and the tab picked. Click a symbol to put the cursor on it; a click on a symbol with children also folds or unfolds it. A file in a language with no outline has no card, the tree taking the whole sidebar; one with nothing in its outline shows `No structure`. With no tab open there is no card either.

The outline comes from tree-sitter (`crates/syntax`), with no language server needed, for Rust, JavaScript, TypeScript, TSX, Python, Java, PHP and Kotlin. It reads the file from disk, so unsaved edits show after the next save.

## Keys

| Key            | Command                    | Scope  |
| -------------- | -------------------------- | ------ |
| `ctrl-s`       | Save                       | global |
| `ctrl-z`       | Undo                       | editor |
| `ctrl-y`       | Redo                       | editor |
| `ctrl-space`   | Show completions           | editor |
| `f12`          | Go to definition           | editor |
| `ctrl-shift-e` | Open the code              | global |
| `escape`       | Put the code away          | global |
| `ctrl-b`       | Hide or show the file tree | global |

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

## Agents

The agent bar sits at the foot of the window, centred, over the home and over the code and chat windows alike. It holds the agent button, the field and the chat button; `Enter` sends what is written to the picked chat, or to a new one. While the agent works, a stop button joins them. Above the bar, with no background, two lines follow the picked chat: the dev's last message, and what the agent is at now (the step it runs, with the file or command, and the time so far), what it waits on (`Waiting for you`, in amber), or how it ended (`Done · 2 files changed`).

The agent button shows the agent, its model, its effort and its mode, the mode in amber when it goes on without asking (bypass, YOLO, full access). It opens a picker upwards: the agents on the left, and on the right what the agent offers for the chat's session: its models, its effort levels and its modes. A section the agent does not offer is not there. While a chat's own session starts, the picker shows what the agent offered its last new session, and what is picked meanwhile reaches the session once it is there. Picking another agent in a chat with messages keeps the chat: the new agent is told the conversation so far, and a line marks the switch.

The chat button opens the chat window, which takes turns with the code over the home (`Esc`, the **×** or a click on the dimmed home puts it away). On the left are the chats of the workspace, grouped by day, each with its agent, when it last spoke and how it is doing. Above them, a search through titles and messages, and a new chat. Right click a chat, or its **⋯**, to **Rename**, **Archive** (archived chats fold into their own section, and come back with **Unarchive**) or **Delete…**, which asks first. On the right is the picked chat: the dev's messages, and each reply as the agent's words and the steps it took (read, edit, search, run…). A step with a diff or an output opens with a click, and a step the agent asks about carries its answers (**Allow**, **Reject**…). A chat is named after its first message.

Agents speak the [Agent Client Protocol](https://agentclientprotocol.com) over their stdio (`crates/agent`). One process per agent serves all its chats. The agent the bar shows starts in the background when the workspace opens, and any other the first time a chat needs it; corvo answers only for permissions: the agent reads, writes and runs with its own tools. Its stderr goes to `$XDG_CACHE_HOME/corvo/agents/<agent>.log`. A chat keeps its agent's session, to load it back the next time when the agent can; when it cannot, a new session is told the conversation. An agent not installed shows how to install it. Each agent carries its logo in its brand color (from [Simple Icons](https://simpleicons.org), CC0, and [LobeHub Icons](https://github.com/lobehub/lobe-icons), MIT, in `crates/ui/assets/agents`).

| Agent       | Command                                                         |
| ----------- | --------------------------------------------------------------- |
| Claude Code | `npx --prefer-offline -y @agentclientprotocol/claude-agent-acp` |
| opencode    | `opencode acp`                                                  |
| Gemini CLI  | `gemini --experimental-acp`                                     |
| Codex       | `npx --prefer-offline -y @zed-industries/codex-acp`             |

Chats are saved one file each to `$XDG_CONFIG_HOME/corvo/chats/<workspace>/<chat>.json`, once they have a message. A chat left working when corvo closed comes back stopped.

## Crates

| Crate            | Role                                                   |
| ---------------- | ------------------------------------------------------ |
| `crates/domain`  | Domain objects and ports, no UI or framework code      |
| `crates/fs`      | Local file system adapter for the `Fs` port            |
| `crates/lsp`     | Language server client: processes, JSON-RPC, protocol  |
| `crates/agent`   | Agent client over ACP: processes, JSON-RPC, sessions   |
| `crates/syntax`  | Code outline from tree-sitter for the `Syntax` port    |
| `crates/ui`      | Design system over GPUI Kit                            |
| `crates/corvo`   | The application: composition and panes                 |

## Requirements

Rust with edition 2024 support, Vulkan drivers, and Wayland or X11 development libraries.
