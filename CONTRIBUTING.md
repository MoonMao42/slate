# Contributing to slate

Thanks for your interest in slate.

## Supported platforms

slate targets macOS and Linux. Official build targets:

- `aarch64-apple-darwin`, `x86_64-apple-darwin`
- `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`

Linux is primarily validated on Debian/Ubuntu + GNOME.

## Development environment

- Rust stable (via rustup)
- macOS: Xcode Command Line Tools (provides `swiftc` for the auto-theme watcher binary)
- Linux: a working C toolchain and `pkg-config`

On macOS:
```
xcode-select --install
```

## Building

```
cargo build
```

On macOS, `build.rs` compiles a small Swift helper (`dark-mode-notify`) via `swiftc`. On Linux the Swift step is skipped.

## Testing

```
cargo test
```

Integration tests use the `SLATE_HOME` environment variable to isolate runs from your real `~/.config`. No test touches your dotfiles.

## Code quality

Before submitting changes:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Layout

- `src/adapter/` — per-tool adapters (Ghostty, Kitty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting)
- `src/cli/` — CLI command handlers and the interactive picker
- `src/config/` — managed config, backups, shell integration
- `src/platform/` — OS-specific capabilities (appearance, fonts, packages, portal)
- `src/theme/` — theme registry and palette data
- `src/design/`, `src/brand/` — visual style and copy
- `themes/themes.toml` — theme source of truth
- `tests/` — integration tests
- `resources/dark-mode-notify.swift` — macOS appearance watcher
- `build.rs` — Swift build step

## License

By contributing you agree that your contributions will be licensed under the MIT License.
