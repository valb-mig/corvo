# corvo

A small, fast code editor written in Rust on top of [GPUI Kit](https://github.com/longbridge/gpui-kit).

## Run

```sh
cargo run -- path/to/project
```

Without a path it opens the current directory.

## Keys

| Key      | Command | Scope  |
| -------- | ------- | ------ |
| `ctrl-s` | Save    | global |
| `ctrl-z` | Undo    | editor |
| `ctrl-y` | Redo    | editor |

Bindings live in `corvo_domain::keymap::Keymap` and reach GPUI through `corvo_ui::keys`.

## Crates

| Crate            | Role                                                   |
| ---------------- | ------------------------------------------------------ |
| `crates/domain`  | Domain objects and ports, no UI or framework code      |
| `crates/fs`      | Local file system adapter for the `Fs` port            |
| `crates/ui`      | Design system over GPUI Kit                            |
| `crates/corvo`   | The application: composition and panes                 |

## Requirements

Rust with edition 2024 support, Vulkan drivers, and Wayland or X11 development libraries.
