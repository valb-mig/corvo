# corvo

A small, fast code editor written in Rust on top of [GPUI Kit](https://github.com/longbridge/gpui-kit).

## Run

```sh
cargo run
```

## Workspaces

corvo opens on the list of workspaces. A workspace links any number of project folders, none required.

The workspaces stand down the left, each with how many projects it links and the logos of the three languages with the most files across them; the field at the bottom adds one. A click or `↑`/`↓` picks a workspace and shows it on the right; `Enter`, its **Open** or a double click opens it. The one picked last is picked again the next time the list comes up, and with none yet the cursor waits in the field.

On the right, the workspace picked shows its projects, two to a row, each card with its folder, its stack badge, the share of each language in a bar and a legend, its branch with how many files changed in its repository, and how many files of code were counted, with a `+` when the project is too big to look all over (the count stops after 8000 entries, as the status bar's does). The line under the name sums them up: the projects, their stacks, and the files changed. A click on a card, which says so while the mouse is on it, opens the workspace with only that project on its home, isolated. The last card links a folder. A project whose folder is gone says so, with **Locate…** to point it at where it went (it keeps its place among the others) and **Unlink**.

The stack badge is the project's marker (see Home); a project with none of its own, as a monorepo, gets one badge for each folder right under it that has one (`engine/ Python`, `web/ Node`). The languages are counted in the background, as the status bar counts them, for every project when the list comes up, and kept while the window is open, so going back to the list counts nothing again; the branches are read again for the workspace picked each time it is picked.

**Rename**, `F2` or a double click on the name turns it into a field: `Enter` or clicking away keeps it, `Escape` leaves it as it was, and a blank name changes nothing. **Remove** or `Delete` asks first, telling how many chats go with the workspace; `Enter` or **Remove** removes it, `Escape` or **Cancel** keeps it. The project folders are never touched.

Inside a workspace the window holds its home. The code comes over it, with the home dimmed behind: the open files in tabs, and a panel floating on their right, over the editor rather than beside it, with the file tree of a project (the one open in the home, or the one the active tab belongs to), the active file's outline nested under its row (see Outline). `ctrl-b` or the panel button of the window's bar opens and closes it. A click on a file's card in the home opens it there, and so does a file picked in the tree; **×**, `Escape` or a click on the dimmed home puts the code away, its tabs kept for the next time. Closing the last tab leaves the code open, empty, with the tree open to pick a file. The code, like the chat and git windows, has a bar along its top with its name, the button of its panel, `Esc` and a **×**, and in its middle the field of the window's palette: click it or press `ctrl-k`, type, and pick with the arrows and `Enter` or a click (`Esc` or a click elsewhere closes it). The code's palette opens a file: the open ones first, then those of the project in front, by their path in it, leaving out what git ignores. Projects are linked and unlinked from the home: the **Link folder…** button of the status bar links one, and right clicking a project card gives **Unlink**. Open tabs belong to the workspace, so switching projects only changes the tree.

Nothing pops up: a window rises a little into place as it opens, fading in while the home dims behind it, and going from one window to another only brings the new one in. The palette drops in under the bar, and the panel slides in from the right. With the desktop's reduced motion on, they are just there. The motion lives in `corvo_ui::motion`.

A status bar runs along the bottom of the window. On the left, the code button brings the code over the home with the tabs left open, or with none and the tree open to pick a file; with the code shown, it puts it away. Next to it, the terminal button opens or closes the terminal window (see Terminal), a button links a folder to the workspace, then the count of cards hidden from the home (see Home), then the branch of the project in front and how many files changed in its repository, which opens the git window (see Git), and **Scripts**, when the project in front has some, which lists them to run one in a terminal. On the right it shows the languages of the project open in the home, the logos of the three with the most files, in their brand colors (from [Simple Icons](https://simpleicons.org), CC0, in `crates/ui/assets/languages`; a color too dark for the dark theme is lifted); click it for all of them, each with its files and share. They are counted in the background when a project is opened in the home, from the extensions of its files, leaving out `node_modules`, `target`, `vendor`, `dist` and `build`; data, docs and settings (JSON, YAML, TOML, Markdown) are not counted as languages.

corvo draws its own title bar: a back button to the workspace list, the workspace name, and the window controls the compositor supports (Hyprland has no minimize, so it only shows maximize and close). Leaving a workspace or closing the window is refused while a tab has unsaved changes.

Files carry the icons of edit.r in the tree and the tabs: a seal in the tone of the file kind with the language letters (`Rs`, `TS`, `Py`), a glyph or a mark (React, Markdown, Claude), or a plain grey leaf. Folders get the same treatment: known folders (`src`, `tests`, `docs`, `.github`…) carry a glyph in the corner. The tables live in `corvo_ui::file_icon`.

Any file opens in a tab of the code: text goes to the editor, images (png, jpg, webp, gif, svg, bmp, tiff, ico) render in a viewer, and other binary files show their size and a notice.

Workspaces are saved to `$XDG_CONFIG_HOME/corvo/workspaces.json` (`~/.config/corvo/workspaces.json` by default), each with how its home was left (see Home), along with the one picked last on the list.

## Home

The home of a workspace is a tree read left to right. The linked projects stand down the left column, with no card for the workspace itself; picking a project isolates it, standing it alone as the root (click it to go back to all the projects). With none linked, a `No projects linked` card links a folder when clicked. Any number of its folders can be open at once: an open folder holds its contents in the next column, with a curve from its card, lit to the open ones. A click on a folder opens it; a click on a file opens its code. An open card sits halfway between the first and the last card it holds, and each branch takes the height of all it holds, so the cards around it make room. Click an open folder to close it with everything open inside.

Right click a card for its menu. **Copy path** puts its whole path on the clipboard and **Copy relative path** its path from the project, a notice telling what was copied. On a folder, **Open in file manager** opens it there and **Open terminal here** opens a terminal in it, in the terminal window; on a file, **Open containing folder** opens its folder with it picked. **Hide** takes the card, and all it holds, off the home; a folder whose cards are all hidden says `All hidden`. Hidden cards stay hidden while you isolate and open others. The status bar along the bottom of the window counts them, and hidden groups: click it to see each hidden card with where it is, and each hidden group with how many cards it has, and bring one back with **Show**, or all with **Show all**.

Pick cards to act on together: `Ctrl` or `Shift` and click a card to pick it, or let it go, or drag on the background to draw a box that picks every card it touches. A click on the background lets them go. Drag a picked card and the others go with it. Right click one of them for **Group…**, which puts them together and opens the group's title to type its name in (`Enter` or clicking away keeps it), or **Hide selected**. A group is a frame around its cards and all they hold, with its title on top: drag the title to move them all; a card dragged on its own stretches the frame. Right click the title to **Rename**, **Collapse** (the group becomes one card where its cards were, which a click expands again), **Hide group** or **Ungroup**. A card belongs to one group at most; **Remove from group** on its menu takes it out, and a group left with no card goes.

A folder's card stands apart from a file's: a little taller and a shade lighter, with a tab on top. It tells how much the folder holds all told (`992 KB`), what it holds (`3 folders · 12 files`) and where it is: a project by its whole path, any other folder from its project's name down (`corvo/crates/ui`), cut at the front when too long. The size leaves out what git ignores and the folders the languages are not counted in (`node_modules`, `target`…), so a Rust project is not its build. Every project is weighed in the background when the home charts it, and again when files move in it, a folder showing `…` until its project is first weighed; a project too big to look all over (200,000 entries) shows what was found with a `+`.

A project's stack, as the list of workspaces shows it, comes from the markers of `corvo_domain::server::Registry`; the first match in this order wins, so a Laravel app with both `composer.json` and `package.json` is PHP:

| Marker                                                   | Badge  |
| -------------------------------------------------------- | ------ |
| `Cargo.toml`                                             | Rust   |
| `composer.json`                                          | PHP    |
| `pom.xml`, `build.gradle(.kts)`, `settings.gradle(.kts)` | JVM    |
| `pyproject.toml`, `setup.py`, `requirements.txt`         | Python |
| `package.json`, `tsconfig.json`, `jsconfig.json`         | Node   |

The workspace opens on its home. Putting the code away reads the open folders from disk again. The home comes back as it was left: the project isolated, the folders open, the cards and groups hidden, the cards moved and the groups; what can no longer be read just doesn't come back.

The camera is free, with no edge to stop it: hold `Space` and drag, anywhere on the graph, cards included, or turn the wheel, to move it anywhere; the cursor turns into a hand while `Space` is held. The keys go to the home whenever nothing is over it, and to the graph again on any press on it. Drag a card by its header to move it, with all it holds, wherever reads best; moved cards keep their place while others open and close, and the layout button at the top left (shown once something was moved) puts every card back and centres the graph; groups stay, their frames around their cards where the layout puts them. Hold `Ctrl` and turn the wheel to zoom, around the mouse, or use the − / % / + at the top left (the % goes back to where it started, at 100%). A drag that ends on a card does not open or close it.

Nothing on the home jumps. When cards open, close, hide or come back, the cards that stay glide to their new place, and those that come in come out of the card holding them, fading in; those that go leave at once. The − / % / + and the layout button glide the camera too, while the wheel and drags move it at once. With the desktop's reduced motion on, nothing glides. The motion lives in `corvo_ui::motion`.

## Outline

The row of the active file in the tree of the code's panel opens on its outline: types, functions, methods, fields, constants and enum variants, nested as in the code. What a type's own `impl` blocks hold goes into the type, after its fields, and an `impl` of a trait for it goes in as a child named for the trait; a type that holds more than fields gathers them in a **fields** (or **variants**) row. All is folded but the way to the cursor, which opens as the cursor moves to other lines, the symbol it is in marked with a bar on its left; a folded type tells how many fields and methods it holds. Click a symbol to put the cursor on it; a click on an open one folds it instead, and a click on the active file's row folds or unfolds its outline. It follows the file picked in the tree and the tab picked: only the active file shows its outline, and a file in a language with no outline has none.

The outline comes from tree-sitter (`crates/syntax`), with no language server needed, for Rust, JavaScript, TypeScript, TSX, Python, Java, PHP and Kotlin. It reads the file from disk, so unsaved edits show after the next save.

## Keys

| Key            | Command                                   | Scope  |
| -------------- | ----------------------------------------- | ------ |
| `ctrl-s`       | Save                                      | global |
| `ctrl-z`       | Undo                                      | editor |
| `ctrl-y`       | Redo                                      | editor |
| `ctrl-space`   | Show completions                          | editor |
| `f12`          | Go to definition                          | editor |
| `ctrl-shift-e` | Open the code                             | global |
| `ctrl-shift-g` | Open or close the git window              | global |
| `ctrl-shift-c` | Open or close the chat window             | global |
| `ctrl-shift-j` | Open or close the terminal window         | global |
| `ctrl-k`       | Open or close the window's palette        | global |
| `escape`       | Close the palette, or put the window away | global |
| `ctrl-b`       | Open or close the window's panel          | global |
| `space` (held) | Drag to move the graph instead of picking | home   |
| `up` / `down`  | Pick the workspace above or below         | list   |
| `enter`        | Open the workspace picked                 | list   |
| `delete`       | Remove the workspace picked (asks first)  | list   |
| `f2`           | Rename the workspace picked               | list   |
| `ctrl-n`       | Put the cursor in the new workspace field | list   |

`ctrl-click` also goes to definition. Bindings live in `corvo_domain::keymap::Keymap` and reach GPUI through `corvo_ui::keys`.

## Git

corvo reads git as the dev has it installed (`crates/git`), with no optional locks, so it never stands in the way of the git run in a terminal or by an agent. Each linked project is read for the repository it is in, projects that share one sharing it: its branch, the commit it is on, and what changed since that commit, untracked files included. It is read again when a file is saved, when an agent's step is done or its reply ends, when the window comes back in front, when the git window opens, and every 3 seconds while the window is in front.

What changed takes a color: green for what is new (added or untracked), blue for what changed (modified or renamed), red for what is gone, amber for a conflict. A changed file shows it in the file tree, with a light fill behind its row, in its tab, and on its card in the home; a folder takes the color of the weightiest change under it, a conflict first, then a change to what was there, then something new.

Files that come, change, go or settle as git reads them, whoever touched them (an agent, a terminal, a commit), show on the home: the folders above them are read again, so a new card shows and a gone one goes, and inside the project open alone the folders down to a file that came or changed open, kept open after. With all the projects shown, only the project's color tells. The file tree reads its open folders again the same way, opening none. The first read of a repository is where things stand, so what had already changed opens nothing; a file git ignores does not open on its own.

In the editor, the text as it is typed, saved or not, is set against the file in the last commit: a bar beside the line numbers marks the lines added (green) and changed (blue), with a light fill behind them, and a red mark sits where lines were taken out; ticks on the scrollbar show where each is. A file the last commit does not have shows all its lines as added, with no fill.

The git window (`ctrl-shift-g`, or the branch in the status bar) takes turns with the code and the chats over the home. It holds the patch of the file picked, from the last commit to the file on disk; its panel, floating on the right (`ctrl-b` or the bar's button), lists the changed files of each repository under its name, branch and count, each with its letter (`A`, `M`, `D`, `R`, `U`, `!`). **Open**, or a double click on a file, opens it in the code. The window's palette finds a changed file by its path and shows its patch.

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

## Terminal

The terminal window (`ctrl-shift-j`, or the terminal button of the status bar) takes turns with the code, the chats and git over the home. It holds the screen of the terminal picked; its panel, floating on the right (`ctrl-b` or the bar's button), lists the terminals of the workspace, each with its name and what runs in it (`Running cargo`, a green dot beside it) or the folder it is in, and a **+** for a new one. Opened with none, a terminal starts in the project in front. A click on a terminal shows it; right click it, or hover for its **×**, to **Rename** or **Close** it, which hangs up on its shell. A terminal goes when its shell ends (`exit`, `ctrl-d`). The window's palette finds a terminal by its name, starts a new one, or runs a script.

Each terminal runs the dev's shell (`$SHELL`) on a pseudoterminal, read by the emulator of [Alacritty](https://github.com/alacritty/alacritty) (`alacritty_terminal`, Apache-2.0), so colors, full screen programs (vim, less, htop) and the shell's own line editing work as in any terminal. The keys go to the program, but the window's own (`Esc`, `ctrl-k`, `ctrl-b`, `ctrl-s`, `ctrl-shift-…`) keep their place: `Esc` puts the window away. `Tab` and `ctrl-c` go to the program too. A drag picks text, a double click a word, a triple click lines, and what is picked is copied as the mouse lets go; `ctrl-shift-v` or `shift-insert` pastes. The wheel goes back through what scrolled off the top, ten thousand lines, and typing brings the screen back down; in a full screen program it sends the arrows.

**Scripts** in the status bar lists the scripts at the root of the project in front, under the file each comes from, and a click runs one in a terminal of its own, named after it, the terminal window coming over the home to show it. The command is typed in the shell once it is ready, so the terminal stays after the script ends, with the script in its history. The scripts come from:

| File                                  | Each                                  | Runs with                                                                               |
| ------------------------------------- | ------------------------------------- | --------------------------------------------------------------------------------------- |
| `justfile`, `Justfile`, `.justfile`   | recipe, but private ones              | `just <recipe>`                                                                         |
| `package.json`                        | script, but `pre`/`post` hooks        | `pnpm`, `yarn`, `bun` or `npm run <script>`, by the lock file, then by `packageManager` |
| `composer.json`                       | script, but composer events           | `composer run <script>`                                                                 |
| `Makefile`, `makefile`, `GNUmakefile` | target, but special ones and patterns | `make <target>`                                                                         |

Beside each, what it does: the comment above a recipe, a `## comment` after a target, composer's `scripts-descriptions`, or else what it runs. A recipe that needs arguments is typed without running, for the dev to finish. The list is read when the project in front changes, when files change on disk, and each time it opens.

Leaving the workspace or closing the window while a terminal runs something other than its shell asks first, naming what runs where; leaving stops it.

## Agents

The agent bar sits at the foot of the window, centred, over the home and over the code and chat windows alike. It holds the agent button, the field, the attach button and the chat button; `Enter` sends what is written to the picked chat, or to a new one. While the agent works, a stop button joins them.

Images go with a message: paste one (`ctrl-v`, a screenshot or an image file copied), drop image files on the bar, or pick them with the attach button. They wait above the field, each with a **×** to take it back out, and `Enter` sends them with what is written, or alone. PNG, JPEG, GIF and WebP up to 10 MB are taken; anything else is turned away with a notice. When sent they are copied beside the chat, in `$XDG_CONFIG_HOME/corvo/chats/<workspace>/<chat>/`, so the chat keeps them whatever happens to the originals, and they go with the chat when it is deleted. In the chat window they show above the dev's words; a click opens one in the code. An agent that says it reads images gets them in the message; one that does not is told where they are on disk, and a notice says so. Above the bar, with no background, two lines follow the picked chat: the dev's last message, and what the agent is at now (the step it runs, with the file or command, and the time so far), what it waits on (`Waiting for you`, in amber), or how it ended (`Done · 2 files changed`).

The agent button shows the agent, its model, its effort and its mode, the mode in amber when it goes on without asking (bypass, YOLO, full access). It opens a picker upwards: the agents on the left, only those installed (and the chat's own, when it is gone), and on the right what the agent offers for the chat's session: its models, its effort levels and its modes. A section the agent does not offer is not there. While a chat's own session starts, the picker shows what the agent offered its last session, kept between runs in `$XDG_CONFIG_HOME/corvo/agents.json`, and what is picked meanwhile reaches the session once it is there. Picking another agent in a chat with messages keeps the chat: the new agent is told the conversation so far, and a line marks the switch.

The chat button, or `ctrl-shift-c`, opens the chat window, which takes turns with the code over the home (`Esc`, the **×** or a click on the dimmed home puts it away). Its panel, floating on the right over the chat (`ctrl-b` or the bar's button), lists the chats of the workspace, grouped by day, each with its agent, when it last spoke and how it is doing. The window's bar names the workspace and counts its chats; its palette starts a new chat or finds one by its title and messages. Right click a chat, or its **⋯**, to **Rename**, **Archive** (archived chats fold into their own section, and come back with **Unarchive**) or **Delete…**, which asks first. The window holds the picked chat: the dev's messages, and each reply as the agent's words and the steps it took (read, edit, search, run…). A step with a diff or an output opens with a click, and a step the agent asks about carries its answers (**Allow**, **Reject**…). A chat is named after its first message.

Agents speak the [Agent Client Protocol](https://agentclientprotocol.com) over their stdio (`crates/agent`). One process per agent serves all its chats. The agent the bar shows starts in the background when the workspace opens, and any other the first time a chat needs it; corvo answers only for permissions: the agent reads, writes and runs with its own tools. Its stderr goes to `$XDG_CACHE_HOME/corvo/agents/<agent>.log`. A chat keeps its agent's session, to load it back the next time when the agent can; when it cannot, a new session is told the conversation. An agent counts as installed when its command is on the `PATH`, and for those started through `npx`, its own CLI too (`claude`, `codex`). An agent not installed shows how to install it. Each agent carries its logo in its brand color (from [Simple Icons](https://simpleicons.org), CC0, and [LobeHub Icons](https://github.com/lobehub/lobe-icons), MIT, in `crates/ui/assets/agents`).

| Agent       | Command                                                         |
| ----------- | --------------------------------------------------------------- |
| Claude Code | `npx --prefer-offline -y @agentclientprotocol/claude-agent-acp` |
| opencode    | `opencode acp`                                                  |
| Gemini CLI  | `gemini --experimental-acp`                                     |
| Codex       | `npx --prefer-offline -y @zed-industries/codex-acp`             |

Chats are saved one file each to `$XDG_CONFIG_HOME/corvo/chats/<workspace>/<chat>.json`, once they have a message. A chat left working when corvo closed comes back stopped.

## Crates

| Crate             | Role                                                   |
| ----------------- | ------------------------------------------------------ |
| `crates/domain`   | Domain objects and ports, no UI or framework code      |
| `crates/fs`       | Local file system adapter for the `Fs` port            |
| `crates/git`      | Git adapter for the `Git` port, over the `git` command |
| `crates/lsp`      | Language server client: processes, JSON-RPC, protocol  |
| `crates/agent`    | Agent client over ACP: processes, JSON-RPC, sessions   |
| `crates/syntax`   | Code outline from tree-sitter for the `Syntax` port    |
| `crates/terminal` | Terminal: shell on a pseudoterminal, emulator, keys    |
| `crates/ui`       | Design system over GPUI Kit                            |
| `crates/corvo`    | The application: composition and panes                 |

## Requirements

Rust with edition 2024 support, Vulkan drivers, and Wayland or X11 development libraries.
