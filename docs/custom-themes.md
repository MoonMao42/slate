# Custom themes

Save a regular TOML file in `~/.config/slate/themes/` (or the `themes/` directory inside your selected Slate profile). Each file can contain one theme as below, or several `[[theme]]` entries with `[theme.palette]` tables.

```toml
id = "my-night"
name = "My Night"
family = "Personal"
appearance = "dark"
auto_pair = "catppuccin-latte"

[palette]
foreground = "#d8dee9"
background = "#20242c"
black = "#878c95"
red = "#c7747b"
green = "#a3be8c"
yellow = "#ebcb8b"
blue = "#81a1c1"
magenta = "#b48ead"
cyan = "#88c0d0"
white = "#e5e9f0"
bright_black = "#858c9a"
bright_red = "#c7747b"
bright_green = "#a3be8c"
bright_yellow = "#ebcb8b"
bright_blue = "#81a1c1"
bright_magenta = "#b48ead"
bright_cyan = "#8fbcbb"
bright_white = "#eceff4"
```

Use `slate list --ids` to check that `my-night` loaded, then `slate theme my-night` to apply it. The picker, tool sync, prompt layouts and automatic dark/light pairing use the same profile catalog. For example, `slate config pairing --dark my-night --light catppuccin-latte` saves an explicit pair.

- IDs use lowercase ASCII letters, digits, `-` and `_`, with a maximum of 64 characters. They cannot begin or end with `-` or `_`; `none` is reserved. Built-in IDs take precedence over custom files.
- All 18 palette values shown above are required and must use `#RRGGBB`. Optional fields include `cursor`, `selection_bg`, `selection_fg` and `brand_accent`; missing semantic colors and tool references are generated from the palette.
- `family`, `appearance` and `auto_pair` are optional. Appearance is inferred from the background when omitted. An automatic pair must name a loaded theme of the opposite appearance; otherwise that pair is ignored.
- Files are considered in path order. Slate loads at most 128 regular `.toml` files and reports when this limit is exceeded. Other files do not use this limit. Invalid definitions are skipped; duplicate custom IDs use the first loaded definition.
- Built-in palettes and generated text/background pairs are checked at 4.5:1 contrast on opaque backgrounds. ANSI bright colors and selections are included. Custom colors, personal overrides and wallpaper visible through transparency can change actual contrast; choose Solid opacity for predictable contrast.
- OpenCode sync writes `~/.config/opencode/themes/slate-sync.json` and selects `slate-sync` in the TUI config. It keeps the canvas transparent; reopen OpenCode after syncing. Clean removes the owned theme and its selection, while restore points recover previous settings.
- Bat sync generates custom `.tmTheme` assets and rebuilds its cache. Delta uses those Bat assets. Recovering a configuration restores files, but does not rebuild external caches.
- Run `slate setup` after upgrading to install the current Neovim loader. Subsequent Slate applies send the current custom palette to an already-running editor. Run setup again if you also want `:colorscheme slate-my-night` shims for themes added later.
- Share codes carry theme IDs and settings, not palette definitions. Copy the TOML file to the receiving profile before importing a code that names a custom theme.
