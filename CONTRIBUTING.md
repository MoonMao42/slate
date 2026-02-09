# Contributing to slate

Thanks for your interest in contributing to slate!

## Development Environment

slate is a macOS-only project. You'll need:

- **macOS** (tested on Sonoma 14+)
- **Rust** (stable toolchain via rustup)
- **Xcode Command Line Tools** (provides `swiftc` for the auto-theme watcher binary)

Install Xcode CLT if you haven't:
```
xcode-select --install
```

## Building

```
cargo build
```

The build process compiles a small Swift binary (`dark-mode-notify`) via `build.rs`. This requires `swiftc` to be available in your PATH, which Xcode Command Line Tools provides.

## Testing

```
cargo test
```

Integration tests use `SLATE_HOME` environment variable to isolate test runs from your real `~/.config`. No test will modify your actual dotfiles.

## Code Quality

Before submitting changes:

```
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## Architecture

- `src/adapter/` -- Tool adapters (Ghostty, Alacritty, Starship, etc.)
- `src/cli/` -- CLI command handlers
- `src/design/` -- Brand symbols and typography
- `src/brand/` -- Brand language strings
- `build.rs` -- Swift watcher binary compilation
- `tests/integration_tests.rs` -- End-to-end CLI tests

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
