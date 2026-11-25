jobs := "4"

dev:
    WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-$(ls -t "$XDG_RUNTIME_DIR" | grep -m1 '^wayland-[0-9]*$')}" \
    PATH="$HOME/.local/bin:$PATH" cargo run -j {{jobs}}

check:
    rustfmt --edition 2024 $(find crates -name '*.rs')
    cargo-clippy clippy -j {{jobs}} --workspace --all-targets -- -D warnings
    cargo test -j {{jobs}} --workspace
