<p align="center">
  <img
    width="180"
    src="./assets/logo-icon.svg"
    alt="slate logo"
  />
</p>

<h1 align="center">slate</h1>

<p align="center">
  A one-command terminal setup for macOS and Linux — themes, prompts, fonts, and tools all in sync.
</p>

<p align="center">
  English · <a href="./README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/MoonMao42/slate/releases"><img src="https://img.shields.io/github/v/release/MoonMao42/slate?style=flat-square&color=585b70" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-585b70?style=flat-square" alt="macOS and Linux" />
  <img src="https://img.shields.io/badge/built_with-Rust-585b70?style=flat-square&logo=rust&logoColor=white" alt="Built with Rust" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme picker swapping Solarized Dark and Light" width="700" />
  <br />
  <sub>Pick a theme — slate previews the whole stack live, no reload.</sub>
</p>

## Why I built this

I could never find a terminal-beautification tool that actually fit the way I use my machine. Every time I wanted a nice setup, I ended up chasing dotfile repos, copy-pasting snippets, and stacking plugins on top of plugins. After all that effort the environment would usually end up a mess, and when I needed to recover I had to dig through everything to figure out what had actually been changed.

So I wrote slate. One command sets up a coordinated look across your terminal, prompt, fonts, and CLI tools. Colors use managed files with targeted edits to existing configuration. Use `slate clean` to remove integration, or a restore point to recover earlier personal settings.

## Install

```bash
# macOS — Homebrew
brew install MoonMao42/tap/slate-cli

# macOS or Linux — install script
curl -fsSL https://raw.githubusercontent.com/MoonMao42/slate/main/install.sh | sh

# Rust users
cargo install slate-cli
```

Then run `slate` to open the menu. Use `slate setup` when you want the full terminal, font, and shell integration workflow.

To upgrade a script-installed copy, rerun the install script. It verifies the
archive's SHA-256, stages the new executable beside the destination, then replaces
the old file atomically. Failures before the final rename leave the previous
executable in place. An interruption at the rename boundary may leave either the
old or the complete new executable. It does not run the downloaded binary or change
your Slate configuration. A successful upgrade does not keep a backup executable.
If the destination is a symlink, use the package manager that owns it (for Homebrew,
`brew upgrade slate-cli`) or choose a separate directory with `SLATE_INSTALL_DIR`.

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub>One command: <code>slate setup</code>.</sub>
</p>

## Try one change first

Run `slate --help` and check that your executable offers `tools` and `prompt`.
This guide describes this branch; older installed releases may lack these entries.
If a command is missing, check which binary your terminal runs (`command -v slate`
on macOS/Linux) before rerunning full setup.

1. **Browse without applying:** `slate prompt --list` shows five illustrative
   layouts; `slate tools` opens tool discovery. Opening details neither installs
   software nor syncs colors. Samples are not screenshots of your current terminal.
2. **Change only the prompt layout:** inspect `slate prompt classic --dry-run`,
   then use `slate prompt classic` to review and confirm. This needs a saved theme
   and the relevant integration; it does not install tools.
3. **Sync one tool:** start with `slate tools info btop`, then
   `slate tools sync btop --dry-run`. Review the paths before confirming
   `slate tools sync btop`. The tool must be detected and a theme saved; sync does
   not install software or edit shell startup files.
   During confirmation, changes to target file contents, permissions or identity
   invalidate the review. Reads are bounded to 8 MiB per file and 64 MiB total;
   this recheck is not an atomic lock against external editors.
4. **Undo a change:** find its recovery point with `slate restore --list`, inspect
   `slate restore <ID> --dry-run`, then enter the restore flow with
   `slate restore <ID>`. Replace `<ID>` with the actual listed ID. File recovery
   does not automatically undo derived caches or running application state.

Live preview in `slate theme` may temporarily write to several detected adapters;
it is not read-only browsing. Esc restores preview files. `slate clean` removes
integration rather than restoring your previous personal configuration. Change
one thing, check its actual application behavior, then move to the next.

## What it does

- One palette across Ghostty, Kitty, Alacritty, Neovim, Starship, bat, btop, Yazi, Zellij, delta, ls, eza, lazygit, fastfetch, tmux, and zsh-syntax-highlighting.

- 🌓 Auto dark/light pairing — native watcher on macOS, XDG Desktop Portal (with GNOME fallback) on Linux.
- Colors live in managed files, with small integration edits to your existing configuration. Snapshots and read-only restore previews help you inspect and recover changes.
- One visual language across every command. Headings, severity markers, and tree receipts all flow through the same render contract, so `slate setup`, `slate status`, and an error message all look like they came from the same tool.
- Small sounds on theme apply, picker navigation, setup completion, and errors. Quiet by design; turn it off with `slate config set sound off`. (The README recordings are silent; runtime feedback is included.)

<p align="center">
  <img src="./assets/fastfetch-preview.png" alt="fastfetch themed output" width="600" />
  <br />
  <sub>Terminal, prompt, system info, CLI utilities — same palette everywhere.</sub>
</p>

<p align="center">
  <img src="./assets/promo/list-9-families.png" alt="slate list output showing 9 theme families" width="600" />
  <br />
  <sub><code>slate list</code> — 9 family bands, Solarized landing right after Catppuccin.</sub>
</p>

<sub>* GitHub README recordings render silently; the installed CLI still plays the curated runtime SFX unless disabled.</sub>

## Zsh syntax colors

Zsh syntax highlighting uses native token styles for commands, quoted arguments,
options, paths and unknown tokens. It updates named color entries without replacing
the highlighter list or clearing unrelated custom styles. It also preserves
existing underline, bold and background attributes while replacing
foregrounds; a `none` reset is removed so it cannot cancel the selected foreground.
Repeated sourcing does not accumulate attributes or leak helper variables. Native checks
cover builtin, quoted-string, comment and unknown-command regions across all palettes;
other token contexts and the live prompt remain unverified. Existing shells need
to load the updated snippet before the colors change.
`tools sync zsh-syntax-highlighting` updates only its managed snippet, preserving
permissions and avoiding identical rewrites. It never sources the plugin or edits
`.zshrc`; its file restore point covers the snippet, not a running shell's styles.
Comment colors target 4.5:1 against the palette's opaque background, preserving
the original shade when sufficient and using a theme gray/body fallback otherwise.
This is not a contrast guarantee for transparent windows or personal overrides.

## Fastfetch preset sync

`slate tools sync fastfetch --dry-run` reviews one managed `config.jsonc`.
It is Slate's preset layout with theme colors, not a merge of personal modules
or logos. Sync preserves personal configuration and shell startup; it updates only
the managed preset, keeps its permissions and skips identical content. The file
restore point covers this preset, not shell variables or installed packages.
The existing Slate shell wrapper must select it; generation is not activation proof.
Updated Bash/Zsh/Fish wrappers omit the managed `-c` argument if the preset is
missing, unreadable, nonregular or a final symlink, leaving native config selection
and user arguments intact. This requires regenerated shell integration; palette-only
sync does not install the wrapper change.
Explicit `-c`/`--config` arguments bypass Slate's injected config argument, since
fastfetch accepts only one config file.
Help, version and read-only `--list-*` discovery flags also bypass the injected
preset, so a damaged Slate config does not block these commands. Config generation
and ordinary rendering are not treated as read-only discovery.
Native fastfetch 2.68.1 rendered the generated key, separator and output colors
for all bundled palettes in a fixed-text fixture. That check replaces system
modules and disables the logo; it does not verify the full system-information
layout, logo rendering or your shell's integration.

## Eza listing colors

Eza's managed `theme.yml` uses native `filekinds`, `perms`, `size`, `links`, `users`, `git` and metadata
style objects, without setting icons or background colors. A fresh shell loads
Slate's `EZA_CONFIG_DIR` and color exports. `EZA_COLORS`/`LS_COLORS` may override
file colors; successful file generation alone does not prove the launch environment
uses it. Native eza 0.23.5 output was checked for directories, regular files,
executables and symlinks across all bundled palettes in isolated profiles.

`tools sync eza --dry-run` reviews one managed file. Applying preserves its mode,
skips identical bytes and rejects unsafe or oversized existing files. It does
not rewrite shell startup or personal eza configuration; the sync restore point
covers that one palette file, not package installation or live shell variables.
Configuration lookup captures `EZA_CONFIG_DIR`; `SLATE_HOME` ignores this host
override and uses its isolated `.config/eza`. A custom selection is not a sync
destination: generated colors remain under Slate's managed directory.

Run `slate doctor eza` from the shell used to launch eza, or choose **Check Theme
Setup** in its tool page. This read-only check distinguishes managed palette
matches, captured directory selection and the presence of `EZA_COLORS`/`LS_COLORS`
overrides. It never launches eza, reads personal YAML or evaluates color expressions;
directory aliases, shell startup and actual rendering remain unverified.
If the captured directory selects Slate but its palette is missing, the check
flags that separately from an unreadable file and never recreates it automatically.

## Lazygit GUI colors


Lazygit integration generates GUI colors only; personal layout, keybindings and
pagers are retained. Updated shell integration combines readable files using
Lazygit's comma-separated `LG_CONFIG_FILE`, placing personal settings last. An
explicit custom `LG_CONFIG_FILE` is left untouched and can bypass Slate's colors.
Reopen Lazygit from a new shell after applying. `tools sync lazygit` updates only
the palette fragment; older shell integration needs regeneration through reviewed
`slate setup`. Parser validation is not a live appearance check.

Run `slate doctor lazygit` (or the tool detail's **Check Theme Setup**) from the
shell used to launch it. This read-only check compares the generated fragment,
reports the captured config selection, exact legacy colon lists and missing
members, and preserves custom overrides. It does not source startup files, parse
personal YAML or inspect a running Git client. JSON output is available with
`--json`; isolated profiles ignore host `LG_CONFIG_FILE`.

## Fastfetch preset checks

Fastfetch's managed preset can be checked with `slate doctor fastfetch` or
**Check Theme Setup** in its tool page. The check compares the full generated
preset against the saved theme without running Fastfetch or collecting system
information. Missing/unreadable files and differences are reported separately;
personal layouts, shell wrappers, explicit `--config` options and live colors
are not evaluated. Use `--json` for a machine-readable report.
Comments and standard JSON whitespace are ignored for preset comparison; literal
values, field order and punctuation still need to match the generated preset.

## btop follows your palette

btop is available in guided tool setup and follows all 20 bundled palettes.
Slate writes one marked asset at `$XDG_CONFIG_HOME/btop/themes/slate-sync.theme`
(default `~/.config/btop/themes/slate-sync.theme`) and changes only the
`color_theme` value in `btop/btop.conf`. Other settings and their formatting stay
intact, including layout, update rate and `theme_background`. This uses btop's
[enumerated user-theme mechanism](https://github.com/aristocratos/btop/blob/v1.4.7/src/btop_theme.cpp).
Custom `btop --config` profiles and custom theme-directory precedence are not
automatically discovered. No wrapper or shell startup hook is added.

Reopen btop after applying. An already-running instance may save its older theme
on exit; reapply Slate afterward if needed. The adapter does not send signals or
claim live reload. Quick setup never installs missing btop automatically; guided
installation is optional (Homebrew or the mapped `btop` apt package, subject to
availability). `slate setup --only btop` explicitly retries installation; it is
not a preview or a theme-only command.

Theme checkpoints and setup baselines capture both files, including absence and
permissions. `slate clean --dry-run` previews disconnection: clean resets only
Slate's exact theme reference to btop's Default and removes only the marked asset.
Other themes remain; restoring an older personal theme uses the pre-apply snapshot.
Unmarked same-name assets are not overwritten/deleted. Ambiguous configs, unsafe
links, oversized files and detected concurrent changes fail closed. This is not
an atomic multi-file transaction. All direct adapter tests use disposable profiles.

## Yazi file manager colors

The Yazi adapter generates UI and code-preview colors for all 20 bundled themes
using the [official flavor merge](https://yazi-rs.github.io/docs/flavors/overview/).
It changes only `[flavor].dark/light` in `theme.toml` to `slate-sync` and writes
`flavors/slate-sync.yazi/flavor.toml` plus `tmtheme.xml`. Keymaps, plugins, opener
rules and startup scripts are untouched. Other personal theme fields/comments
remain and personal style overrides still win, so live appearance can differ.
Both flavor slots follow Slate's selected theme; no second auto-theme policy is introduced.

```sh
slate tools sync yazi --dry-run   # review three potential write targets
slate tools sync yazi            # explicit confirmation, no software installation
```

Paths use absolute `YAZI_CONFIG_HOME` or the standard XDG directory; `SLATE_HOME`
ignores host overrides. Empty overrides are unset; relative overrides are rejected
before writes rather than guessing a destination. Fields target Yazi 26.9.1;
older versions are not guaranteed. Inventory/preview never launch Yazi. Reopen
the file manager after applying; no live reload is claimed. Optional guided
installation uses Homebrew where available. Linux has no unverified apt mapping:
install Yazi manually first. Quick setup does not install missing Yazi automatically.

Sync checkpoints and setup baselines capture all three files, including absence
and permissions. Clean preview/execution removes only exact Slate flavor choices
and marked assets, keeping other flavors, personal overrides and empty directories.
Use the pre-apply snapshot to recover a previous flavor. Unmarked same-name assets,
unsafe links, non-regular or over-8-MiB files and detected concurrent edits stop
overwrites. Assets publish before selection; multi-file failure can leave partial
changes, inspectable/recoverable through the operation's retained checkpoint.

## Zellij workspace colors

Zellij tabs, pane frames and status components follow all 20 bundled palettes,
using the [native component theme format](https://zellij.dev/documentation/themes).
Slate writes a marked `themes/slate-sync.kdl` and updates only the top-level
`theme`, `theme_dark` and `theme_light` string values in `config.kdl`. All three
follow Slate's saved selection; native automatic hue detection does not choose a
different palette. Personal keybindings, layouts, plugins, comments and formatting
are preserved. This adapter targets Zellij 0.45.1 / KDL v1; older versions are not
guaranteed. No Zellij process or session action is invoked by discovery or sync.

```sh
slate tools info zellij
slate tools sync zellij --dry-run # review the two paths
slate tools sync zellij           # confirm before writing; does not install Zellij
```

Paths follow Zellij's [configuration lookup](https://zellij.dev/documentation/configuration),
including independent absolute `ZELLIJ_CONFIG_DIR` / `ZELLIJ_CONFIG_FILE` overrides
and an absolute `theme_dir`. A config-file override alone does not move the theme
directory. Without overrides, `~/.config/zellij` takes precedence over the platform
directory (macOS Application Support or Linux XDG). `SLATE_HOME` ignores host
overrides. Empty/relative overrides, implicit `/etc/zellij`, ambiguous KDL, unsafe
files and same-name personal themes stop writes rather than guessing or overwriting.
The first inspection pins destination paths and their directory aliases for that
Slate invocation; if they change, reopen Slate and review again before syncing.
Other `.kdl` themes are checked for name collisions (256 directory entries / 8 MiB
total inspection limit). Optional [Homebrew installation](https://formulae.brew.sh/formula/zellij)
uses the existing reviewed installer; no apt mapping or automatic missing-tool
installation in Quick setup is added.

KDL parsing also has a per-document complexity limit of 128: maximum child-block
nesting plus the number of `/-` directives, including discarded nodes/entries.
Text inside strings, raw strings and line/block comments does not count. Inputs
over this conservative limit are reported before parsing or adapter writes, not
silently simplified. Block-comment interiors are masked only in a private parse
copy at identical byte offsets, preventing the KDL v1 library's recursive comment
handling from overflowing. Original comments, formatting and edited-value spans
are preserved. Parsing uses an in-process worker with a 32-MiB stack reservation;
this does not start Zellij or change the original file.

Native file watching may update running sessions, but Slate does not verify live
appearance. Layout/CLI overrides can still win; start a new session with this
configuration if needed. Sync checkpoints and setup baselines cover both files,
including absence and permissions. Clean changes only exact Slate selections to
`default` and removes only the marked asset; use the pre-apply snapshot to restore
previous personal choices. Assets publish before config selection; partial failures
retain a recovery point, not an automatic multi-file rollback.

## Start here

Run `slate` in a terminal for the menu. Preview Themes / Switch Theme is the
first action; Enter applies the selected look and Esc restores preview changes.
Connect Tools opens a tool menu: sync one available adapter to the saved theme,
inspect availability, or explicitly open guided setup for installation/startup hooks.
Shell Preferences controls the prompt, highlighting and startup banner, not the
adapter catalog. Check Status is read-only. Invalid saved settings are reported,
never displayed as an applied default. Non-interactive entry shows read-only
status and next commands. Interrupted-preview recovery still takes priority.

After a successful action, the main menu returns with refreshed saved settings;
canceling a theme preview restores its files and returns here too. Tool pages let
you preview potential writes, review/decline a sync, or check supported theme
connections without reopening Slate. Use Back to return one level, and Quit /
Leave Tools to finish. Confirmed changes are kept when leaving; Back is not undo.
Esc/Ctrl+C exits ordinary menus; the theme picker retains its restore-on-cancel
behavior. Errors stop the action rather than silently retrying it.

Opening Shell Preferences or the Auto-Theme submenu, then going Back, creates no
profile, sound cache or write lock. Writing begins only after selecting a change;
pairing configuration and guided setup still have their own review flows. Explicit
actions such as `slate tools sync btop` and `slate prompt minimal` remain one-shot;
non-interactive inspection does not enter a loop.

## Know which build you are trying

The hub's **About This Build** page shows the running executable's path, embedded
source tag, target, Cargo profile class/features and built-in theme/adapter/prompt
capabilities. It returns to the main menu without changing settings. For support
or when comparing an installed binary with a development build:

```sh
slate -V                         # short package version, unchanged
slate --version                  # version plus embedded source tag and target/profile
slate about                      # this executable and its compiled capabilities
slate about --json               # read-only report, schema_version 1
```

The direct commands work without HOME, valid settings or Git, including while a
configuration writer or preview recovery blocks mutations. They do not inspect
installed tools, read personal configuration, run native probes or check for
updates. Capability lists come from the compiled registries; they are not proof
that an adapter is installed, configured or visually active. The executable path
can expose your local username; review it before sharing the report. Unavailable
paths/source tags are `null` in JSON, and non-UTF-8 paths are labeled as lossy.

`fnv1a64-v1-…` is a non-cryptographic, build-time label over sorted relative paths
and bytes of `src/**/*.rs`, Cargo manifest/lockfile, the build scripts and selected
embedded resources (listed in `build_metadata.rs`). Unrelated docs, `.git`, build
outputs, timestamps and absolute build paths are excluded. Missing, unreadable,
symlinked or oversized inputs yield an unavailable tag, never a partial tag;
scans are limited to 16,384 source-tree entries, 64 directory levels and 64 MiB.
This is not a Git revision, signature, binary checksum or exact-artifact identity:
toolchain, native helper and compiler settings can differ despite equal tags.
Cargo-normalized feature names and Cargo's `debug`/`release` profile classification
are separate labels. Packaged/normalized manifests can produce a different tag
from the checkout; no repository is needed when running the compiled binary.

## Browse tools by workflow

The Tools menu also offers **按用途找工具** (browse by workflow): terminal windows, shell and
prompt, files and system, development tools, and split-pane workspaces. For example,
the workspace group shows tmux and Zellij together. These are discovery groups,
not install/sync bundles; the full supported-tool catalog remains available.

## Prompt layouts, independently of colors

The style page's **Check Current Prompt** action inspects the captured Starship
config selection, saved layout and activation preference without running tools or
changing files. It remains available when preferences are unreadable and returns
to the same style page. This does not verify the live prompt or source shell files.

Before confirmation, file review also shows any captured `STARSHIP_CONFIG`
override. This environment hint does not read that file or add it to the write
plan; the listed change targets remain authoritative.

The hub's **Prompt Style** action offers Rainbow segments, Clean two-line,
Compact one-line, Classic shell, Focus one-line and Branch one-line. Themes recolor the layout without switching
it back. Minimal, compact, classic, focus and branch presets use ASCII symbols; personal substitutions/icons are retained
and may still need your chosen font. Rainbow keeps the plain-font fallback.

Focus shows only the directory and input symbol: green `>` for success, red `x`
for failure, with no Git, clock, host display or second line. Review it with
`slate prompt focus --dry-run`. Personal Git/custom module definitions remain,
but are not included in this layout; switching presets can use them again.
This is not a Git performance benchmark or a switch that disables Git itself.

Branch keeps the directory, Git branch and input on one line: `~/project on main >`.
Unlike Compact, it does not include Git change counts; duration and clock modules
are also absent from the layout. Personal module settings remain intact.
Review with `slate prompt branch --dry-run` or choose Branch one-line in the menu.

Classic is a traditional two-line layout: user, optional `@host`, directory and
Git context above a literal `$` prompt. The dollar is green after success and red
after failure, using the selected theme's palette. Hostname visibility follows
Starship's SSH-only default or your personal visibility/detection rules; aliases,
hostname trimming and other personal settings are preserved. The example shows
an SSH host, not a live connection. Selecting Classic never opens SSH or installs
an icon font, and recoloring/plain-font regeneration keeps the selected layout.

Interactive `slate prompt` and the hub now let you compare examples before a
theme is saved. Selecting a layout opens an illustration page; it does not prepare
or write personal prompt files. Review File Changes appears for a recognized saved
theme and still asks for default-No consent before applying. Declining returns to
the example, Choose Another Layout returns to the catalog, and Refresh Saved Theme
rechecks a theme changed in another terminal. Unsafe/unreadable theme state never
becomes a guessed default; examples remain available even with broken personal
configs or pending recovery. Actual review/application retains its safety checks.

Choose a Theme First has separate default-No consent: theme preview may temporarily
change detected adapters, and Enter saves theme/opacity across them. It does not
apply the layout being browsed. Saving or canceling the picker returns to that same
layout page; Esc restores preview files. Leaving the layout browser keeps any theme
change already confirmed. Explicit `slate prompt <style>` remains a one-shot review
and apply command; outside a terminal, bare `slate prompt` prints the catalog.

```sh
slate prompt                      # browse examples first, then review before applying
slate prompt --list                # built-in examples, no profile reads
slate prompt minimal --dry-run    # sample and three-file change plan; no writes
slate prompt compact              # defaults to No until you confirm
slate prompt classic --dry-run    # classic user/host context, no changes
slate prompt rainbow --yes        # explicit consent for non-interactive use
slate status --json               # prompt_style is the last saved preset, not a live check
```

Examples are illustrations, not executions of Starship or personal custom
commands. A confirmed change edits the standard XDG `starship.toml`, the managed
plain-font fallback, and `[prompt].style` in Slate's `config.toml`. The preset
replaces the root/right prompt and participating modules' presentation; custom
command definitions, timeouts, detection settings, directory substitutions and
unrelated settings remain. Modules outside the preset are not added to its layout.
No software/font is installed, shell activation is unchanged, and other tools or
the global theme are not reapplied. Existing shell integration is required to see
the result; a custom `STARSHIP_CONFIG` can override these standard files.

File snapshots precede changes, preserve permissions and include original absence.
Use the reported restore point to get your earlier personal layout back; `clean`
is not historical layout restoration. Invalid sources, unsafe links and detected
edits since review stop publication. The saved preference is published last;
partial writes remain recoverable, not automatically rolled back. Repeated
identical selections keep file identities and do not create another snapshot.
Theme/font regeneration and missing-config setup seeding honor the saved layout.
`--list --json` and `<style> --dry-run --json` are read-only, versioned outputs.

## Sync one tool, without reinstalling

```sh
slate tools                       # tool menu; read-only inventory outside a terminal
slate tools list --json            # executable/config evidence, not activation proof
slate tools info yazi             # purpose, installation route and next steps; read-only
slate tools info starship --json  # versioned detail, even without a saved theme
slate tools install yazi --dry-run # review one missing tool's installation route
slate tools install yazi          # separate default-No consent; does not configure themes
slate tools sync btop --dry-run    # review potential writes, no native probes or files
slate tools sync btop             # defaults to No until you confirm
slate tools sync btop bat --yes    # explicit non-interactive consent
```

Detected tools have direct entries in `slate tools`, even before a theme is saved.
Opening an entry only shows details; without a theme, syncing remains unavailable
and entering theme preview requires a separate confirmation. Back returns to Tools.

**Browse Supported Tools** is available before setup or theme selection. It lists
all 16 adapters, including undetected tools, in an eight-row scrollable menu with
no search. A tool page explains its purpose, detected availability, installation
route and next steps. Sync actions appear only with a recognized saved theme and
a detected tool; this permits a review, not a compatibility or activation claim.
Refresh Availability re-reads these prerequisites after changes in another terminal.
After synchronization, applied tools receive activation guidance and a read-only
doctor/detail command. Partial failures retain guidance for successful adapters,
not for skipped or failed ones. This does not verify live colors or regenerate
shell startup; full setup remains a separate, reviewed workflow.
In tool details, recoverable preview/sync/install/check errors return to refreshed
details without automatic retry. Earlier confirmed changes may remain; inspect
the result and restore point before retrying. Direct commands still report failures
through their exit status, and Esc/Ctrl+C exits the interactive menu.
The full Guided Setup wizard requires a separate default-No confirmation from a
tool page, since it may change other tools, the theme, font and shell integration.
Back from a catalog detail returns to the catalog; merely browsing or declining
setup creates no files and runs no tools/installers.

`tools info <id> --json` exposes schema version 1, tool evidence, purpose, saved
theme/warning, `sync_review_available`, installation advice, next steps and
`recommended_action` (action, label, reason and suggested command). Tool menus
select that recommendation initially: refresh unknown availability, review a
missing tool's installation, separately confirm theme preview without a saved theme,
or check existing wiring before syncing. Without a file checker, Preview Sync is
recommended. Opening the page never executes the recommendation.
`theme_selection_available` distinguishes a missing/unknown theme from unreadable
state. **Choose a Theme First** opens the existing picker with default-No consent:
previews affect detected adapters, not only this tool. Saving or canceling returns
to the same tool and refreshes its actions without an implicit sync or install.
Unreadable state hides this shortcut and is rechecked after confirmation.
Installation routes reuse the existing setup policy: some adapters are manual
install only, and a route is not proof of helper availability, network or write
permissions. Info remains usable with missing/unknown themes and pending recovery;
it does not inspect application configuration contents or validate live rendering.

### Install just one missing tool

Tool details offer **Install This Tool** when the tool is not detected and the
current setup policy supports an installation route. `slate tools install <id>`
uses that same review without opening the full wizard. No saved theme is needed.
Already-detected tools are a read-only no-op, even with `--yes`; this is not an
upgrade/reinstall command. Terminal apps, tmux, Neovim and OpenCode remain manual
installs. Yazi requires the Homebrew route; no apt mapping is guessed.

Use `--dry-run` (optionally `--json`, schema version 1) to inspect the requested
tool, action, route, possible Starship fallback and scope notes. Missing tools
require default-No interactive consent or explicit `--yes` outside a terminal.
Previewing, declining, invalid input and already-detected no-ops create no lock,
profile, snapshot or sound cache, and launch no tools/installers. After consent,
Slate rechecks tool presence and the installation route, runs only exact-tool
platform checks, and acquires its writer guard before calling the shared installer.
Pending recovery stops installation; its read-only preview remains available.

Only one tool is requested, but package managers may change dependencies, caches
and package records outside the Slate profile, including with `SLATE_HOME`.
There is no package snapshot/automatic rollback or package-version/executable pin.
The reviewed Starship fallback follows existing policy; an uncertain installer
result stops without another automatic attempt. A successful installer exit must
also leave the tool detectable, otherwise the command reports failure and asks
you to inspect installation/PATH before retrying. Slate does not apply colors,
change fonts or activate shell/editor hooks. After installation, the tool page
refreshes; choose a separate sync or setup action when you want configuration.

### Sync saved colors separately

Sync uses your recognized saved theme, never a fallback. Only selected adapters
run: no installs, global theme/pairing changes, shared shell regeneration, or
unselected editor notifications. Shell-dependent tools still need existing Slate
shell integration; Neovim sync only notifies an existing loader. Missing entry
configs need guided setup, not an implicit setup inside sync. Terminal adapters
may reapply saved appearance (including Ghostty's font) and reload their windows;
bat rebuilds its cache; btop needs reopening. These effects appear in the review.

The preview is a potential-file plan, not a byte diff or a compatibility verdict.
Native compatibility checks run only after confirmation. The plan is rechecked
under writer exclusion; changed saved themes/destinations require another review.
A file recovery checkpoint is required before adapter writes. Partial failure
retains successful writes and reports its restore point, without claiming global
success. Recovery covers configuration files, not derived caches, running apps,
adapter-local backup copies or newly created empty directories. Listing, previews,
and declined confirmations create no locks, settings or sound cache.

### Installed, but colors or the prompt did not change?

**Check Theme Setup** in the tool menu and tool-detail pages opens btop, Starship,
Yazi or Zellij checks, even without a saved theme or a detected executable.
Use `slate doctor <tool>` with one of those tool IDs and optional `--json`.
They read files only: no tools/custom prompt commands,
installation, configuration repair or native version probe.

btop checks its standard config's exact asset reference, the generated-file marker
and byte equality with the saved theme's generated asset. Formatting or comments
alone can cause a mismatch; this is not a verdict on live colors. Custom `--config`,
theme-directory overrides and named-theme precedence are not resolved. A running
instance may save its old selection on exit; close it, sync again, then reopen.
Starship checks captured `STARSHIP_CONFIG` (otherwise the standard XDG path),
Slate's activation preference, palette entries and saved preset presentation fields.
It distinguishes the plain fallback from the standard profile. Custom overrides
are not edited by `slate prompt` or selected-tool sync; relative overrides are
reported without guessing a read target. No saved preset means personal layouts
are allowed, not an assumed Rainbow layout.

Yazi checks both flavor slots, separate UI/syntax assets, ownership markers and
exact generated-file matches. Other personal `theme.toml` sections are flagged as
possible overrides, not removed or evaluated as a complete native merge.
Zellij checks static/dark/light choices separately, resolves the selected profile's
theme directory and checks the generated asset plus same-name inline/directory
conflicts. The conflict scan shares sync's 256-entry / 8-MiB limits; unreadable,
malformed or unsafe inputs are errors, not proof of a clean scan. Custom or missing
slots do not establish Slate selection, while a matching asset alone does not
establish selection or live activation. Unknown saved themes skip color comparisons.

File matches do not prove shell initialization, fonts, native compatibility or
live rendering. Checks remain available during writer/recovery conflicts or broken
backup storage. Bounded reads allow 4 KiB theme records, 256 KiB Slate preferences
and 8 MiB tool files, rejecting final file symlinks, non-regular files and isolated
profile escapes without echoing configuration bodies. Files are observed separately,
not atomically; `SLATE_HOME` ignores host tool-specific profile overrides. Runtime
layouts, command-line overrides and native version compatibility are not checked.

## Safe live preview

Live picker previews are temporary: Esc, Ctrl+C or `q` restores the captured terminal files,
including comments and permissions, and removes files created only for preview.
Pressing Enter also clears the preview before taking the commit's safety snapshot.
Explicitly saved auto-theme choices are kept. Detected external edits or redirected symlinks
are left untouched and reported instead of being overwritten.

Queued keys are handled in order, so quick navigation followed by Enter saves the
requested row. The first confirm/cancel ends input processing. Navigation is batched
into one preview update per changed final selection; Tab, resize, and `s` save feedback
do not reapply terminal settings. The explicit `s` auto-theme save remains saved on cancel.

Browse directly with ↑↓ (or j/k), use ←→ for supported terminal opacity, Tab
for full preview, and Enter to apply. There is no picker search mode. On terminals
supporting bracketed paste, pasted content is ignored as a whole with a brief
hint: pasted newlines and shortcut letters cannot apply, cancel, save auto-theme
preferences or change the selection. Clipboard contents are never echoed.
Cleanup disables the requested paste mode. Unmarked keystrokes cannot be
distinguished from normal typing.

Picker frames now use the actual window row/column budget. Family headings count
against list space; selection and cancel help take priority over optional
mini-preview and opacity chrome. Narrow lines are clipped without wrapping, and
the last row has no trailing newline that could scroll the header away. Full
preview keeps all eight blocks reachable: PageUp/PageDown scroll with a one-line
overlap, Home/End jump to the top/bottom, and a fixed footer shows the visible line
range and input help. ↑↓ still selects themes.
Scrolling neither applies a theme nor reruns an already cached prompt. Changing
themes or toggling Tab resets the scroll position; resizing
clamps it, while End stays anchored to the bottom even if the prompt height changes.
Long colored prompts retain their styles when paging into the middle; clipping
preserves complete numeric SGR (including colon forms) and resets before the footer.
Below six list rows or seven full-preview rows, a compact selected-theme view
retains cancel help when space permits; paging is inert there. Resizing changes
layout, not the selected theme.

Tab's real Starship prompt is optional: its configuration input and generated preview
are limited to 8 MiB, and prompt capture has a 750 ms post-spawn deadline with a 64 KiB
combined stdout/stderr cap. Invalid configs, failed commands, oversized output and
timeouts use the built-in sample prompt; partial output and stderr are not displayed.
Non-UTF-8 output also uses the sample. Display filtering preserves readable Unicode,
newlines and bounded numeric SGR styles, removes cursor/screen/window/clipboard control
sequences and control-string payloads, and resets styling around the prompt block.
Hyperlink labels remain text without clickable OSC links; tabs become four spaces and
carriage returns are removed. This limits terminal effects, not the content's meaning.
Failures are cached per theme until resize or a new picker session. Config checks also
reject resolved preview paths escaping the managed directory. These are not a sandbox,
an external-editor lock, or a hard time bound on filesystem, spawn and OS termination.

Preview undo records the intended bytes and published permissions of Slate's actual
writes, not everything found in a later readback. Parallel terminal adapters explicitly
share those receipts; unrelated threads do not. Tracked writes check the captured
destination and current contents before opening a temporary file and again before
publication. Unrecorded changes detected afterward are preserved for recovery review.
These checks are not an atomic lock against an external editor between check and rename.

Cleanup is attempted on normal exit, errors, and the panic hook. Reentrant cleanup
while a preview operation is still active stops without changing files or clearing
the record. After an incompletely recorded write, only files still matching the last
recorded preview writes are automatically restored; unrecorded changes are left for review.
Unavailable or inconsistent expected state blocks cleanup instead of disabling its
conflict checks. A private, atomically written recovery record also survives a forced
process exit. If a preview was
interrupted, use the same HOME/XDG environment and run:

```sh
slate recover --dry-run          # inspect changes; add --json for automation
slate recover                   # confirm and restore recorded preview changes
slate recover --export ./preview-originals  # inspect originals without restoring
```

Recovery refuses an active preview and preserves conflicting external edits.
If interruption occurred before a write was fully recorded, export the originals
for review. To keep the current files and abandon the recovery copy explicitly,
use `slate recover --discard` (or add `--yes` for non-interactive confirmation).

`recover --dry-run` text/JSON output tolerates an early pipe-reader exit, but active,
conflicting or unreadable recovery still fails. Actual recovery, export and discard
require the initial plan to be written successfully before proceeding; this also
applies to the explanation preceding an explicit corrupt-record discard. Output
failure at that point leaves the recovery action unperformed. If an action completes
but its final message cannot be written, the error explicitly says what completed
and that it was not rolled back. Text escapes paths/reasons; the JSON plan is unchanged.
A successful write does not prove the reader reviewed it or make inspection atomic;
existing confirmation and journal validation still apply.

Actual recovery, export and discard now hold the existing cooperative writer lock
from plan preparation through confirmation and execution. They recheck the captured
record's bytes, identity, permissions and resolved path before acting; detected
changes require a fresh inspection. Recovery also rechecks target-file conflicts.
Canceling only releases the lock, and dry-run inspection remains read-only. Explicit
discard still accepts corrupt private regular records; oversized records are bound
by metadata without reading their contents. If the record changes after files have
been restored, it is retained and the error says restoration was not rolled back.
This is not a filesystem transaction or an atomic lock against external editors.

Restored bytes receive their saved permissions before publication. A failure midway
through recovery can leave some files restored; the error names failed paths and
leaves record finalization unattempted. Reinspect with `slate recover --dry-run`
before retrying. Finalization errors distinguish a record that could not be removed
from one already removed whose directory sync failed (removal durability unconfirmed).
They report completed restoration, or unchanged configs for discard, without claiming
an automatic rollback. These guarantees do not make multi-file recovery crash-atomic.

The record and exported files are private; JSON previews do not include file contents.
Preview input reads are limited to 8 MiB per file and 16 MiB per collected state;
recovery checks apply the same per-file limit. Binary bytes and captured Unix modes
are preserved, including configurations reached through a resolved dotfile link.
Oversized or unsafe current targets block restoration, but saved originals can still
be exported and the record explicitly discarded. Journals keep their 32 MiB encoded
limit, now enforced during serialization. Encoding failure leaves the previous record
unchanged; this is not a rollback of any preview writes already performed. Failed
initial capture may leave private lock metadata, without changing target configs.
Power-loss recovery is not guaranteed. Live terminal reload is best-effort;
Kitty reloads its original configuration through
[`load-config`](https://sw.kovidgoyal.net/kitty/remote-control/#kitten-load-config).

## Neovim follows along

Slate ships 20 Neovim colorschemes mirroring every terminal family and reloads open buffers the moment you switch.

<p align="center">
  <img src="./assets/nvim-before.png" alt="Neovim with Catppuccin Frappé" width="700" />
</p>

<p align="center">
  <img src="./assets/nvim-after.png" alt="Neovim with Kanagawa Lotus" width="700" />
</p>

Works with LazyVim, kickstart.nvim, or a bare init.lua.

Live reload debounces rapid switches and survives atomic state-file replacement
or recreation, provided the state directory remains available. Use
`:lua require('slate').stop()` to pause syncing, and
`:lua require('slate').setup()` to read the current theme and restart the watcher.
Repeated setup does not accumulate watchers or exit hooks. Watch failures report
a warning; retry setup after fixing the directory or permissions.
Existing installations need a new `slate setup` with Neovim integration enabled
to regenerate the loader, followed by a Neovim restart.

`slate config set editor disable` remembers a profile-local opt-out and removes
Slate-owned activation blocks, keeping colorschemes and the loader for manual use.
Future setup skips Neovim activation, including quick mode. Choosing “show me the
line” or “skip” in setup also remembers manual activation. To allow setup again,
run `slate config set editor enable`, then `slate setup`; enabling alone does not
insert a hook. Unmarked user hooks and running editors are unchanged: stop the
watcher or restart Neovim if needed. `doctor nvim` reports the choice separately
from actual hooks. New snapshots capture it; `clean` removes it with Slate's loader
directory. Older versions did not record opt-outs, so rerun `editor disable` once
after upgrading to remember an earlier choice. If hook removal fails, the opt-out
stays saved and the command reports incomplete removal rather than success.

Custom `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, and `NVIM_APPNAME` paths are supported.
For example, use `NVIM_APPNAME=nvim-work slate setup` for a separate Neovim profile;
use the same environment when diagnosing or cleaning that profile.
Zsh integration follows exported `ZDOTDIR`, falling back to `~/.zshrc` when unset.

## tmux and SSH sessions

Theme switches reload Slate's colors in the current tmux server without replaying
your plugins or other startup commands. Outside tmux, Slate tries the default
server; it never starts one just to apply colors. Configuration remains saved if
reload is unavailable, and the normal command output explains the warning.

The tmux integration uses the first existing file among `~/.tmux.conf`,
`$XDG_CONFIG_HOME/tmux/tmux.conf`, and `~/.config/tmux/tmux.conf`, falling back to
`~/.tmux.conf` when none exists. Paths containing spaces are supported.

Over SSH, Slate can configure remote tools and reload the remote tmux server.
Terminal fonts, opacity, and graphical terminal reloads belong to the client
machine: configure them by running Slate locally. Preview stays inside the picker.
`SLATE_HOME` isolation disables live terminal and tmux reloads.

## Auto-theme

```
Light mode  →  your light theme + matching prompt, syntax, tools
Dark mode   →  your dark theme + matching prompt, syntax, tools
```

Enable from the hub (`slate` → Auto-Theme). Every theme family ships a built-in dark/light pair, and you can override the pairing there too.

## Support

Official targets: `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. Linux is validated on Debian/Ubuntu + GNOME.

| Tier | Platform | Status & Notes |
|------|----------|----------------|
| Tier 1 — first-class (CI smoke-tested every release) | macOS (Apple Silicon + Intel) | Ghostty, Kitty, Alacritty, Terminal.app (partial — no live preview, no opacity, font cannot auto-apply). |
| Tier 1 — first-class (CI smoke-tested every release) | Debian / Ubuntu + GNOME (x86_64 + aarch64) | Ghostty, Kitty, Alacritty all wired up; live reload works on each. |
| Tier 2 — best effort (wired up, not in CI) | Other Linux distros (Fedora, Arch) and other desktops (KDE, Sway) | Themes still apply; live reload depends on the terminal you run. |
| Tier 3 — out of scope | Windows | No plans to support. |

Shells: `zsh`, `bash`, `fish`. `zsh` is locally verified; `bash` and `fish` are wired up but pending broader testing.

Generated Fish configuration and its setup loader use
[Fish-specific literal quoting](https://fishshell.com/docs/current/language.html#quotes),
preserving backslashes and apostrophes in UTF-8 paths and values instead of
reusing Bash/Zsh quoting. This covers PATH entries, exported tool configuration
paths and wrapper arguments. It does not add support for non-UTF-8 shell paths
or make external tool configuration formats accept otherwise unsupported paths.

Generated shell environments keep PATH entries, color/config exports (including
`STARSHIP_CONFIG` when enabled), and the manual Fastfetch wrapper available to
scripts. Prompt initialization, the minimal prompt, Zsh highlighting, Fastfetch
autorun and the automatic watcher launch run only in interactive shells. Bash/Zsh
check the shell's `i` flag; Fish uses `status is-interactive`, following
[Bash's interactive-shell check](https://www.gnu.org/software/bash/manual/html_node/Is-this-Shell-Interactive_003f.html)
and [Fish's startup-output guidance](https://fishshell.com/docs/current/faq.html#why-won-t-ssh-scp-rsync-connect-properly-when-fish-is-my-login-shell).
The guards do not exit/return out of the calling script. Sourcing again in an
interactive shell still refreshes features and may run autorun/helpers again;
this change is not a once-per-session cache. Existing managed files gain the
guards on their next successful setup/theme/config regeneration. User-written
startup commands and commands explicitly called by scripts are unaffected.

<details>
<summary><strong>bat configuration and cache rebuilds</strong></summary>

Slate uses the detected `bat` or `batcat` executable, including home-local fallback
locations. The config file (`BAT_CONFIG_PATH`), custom assets (`BAT_CONFIG_DIR`),
and compiled cache (`BAT_CACHE_PATH`) are separate paths; a config-file override
does not relocate `themes/`. Defaults are `$XDG_CONFIG_HOME/bat` and
`$XDG_CACHE_HOME/bat`. This follows [bat's directory selection](https://raw.githubusercontent.com/sharkdp/bat/master/src/bin/bat/directories.rs).

Paths are captured once per operation; relative overrides are anchored to the
starting working directory. An explicitly empty override selects that directory,
not the XDG default. `SLATE_HOME` and injected test profiles ignore ambient bat
overrides. Theme apply never edits the bat config file itself.

Cache rebuilds have a 30-second post-spawn deadline and a combined 256 KiB output
cap. Failed exits, timeouts, output overflow and bat's known “built without
build-assets” response fail the apply, even when that response exits zero.
The last committed Slate theme remains unchanged on failure; generated theme
files and partially changed external caches are not automatically rolled back.
Native output is omitted from errors. Fix the underlying bat/cache issue and retry.
The executable is not sandboxed; OS calls and detached processes are outside the
deadline guarantee. Successful exit is not a native rendering test.

</details>

<details>
<summary><strong>Per-terminal status</strong></summary>

| Terminal | Status | Notes |
|----------|--------|-------|
| Ghostty | Recommended | Full support — live reload, opacity, watcher relaunch |
| Kitty | Full | Live palette push via `kitten @ set-colors`; opacity + Nerd Font sync |
| Alacritty | Full | Inline preview and reload |
| Terminal.app | Partial | macOS only — no live preview, no opacity, font cannot be auto-applied |
| Other | Best effort | Shell and CLI tool theming works; terminal visuals depend on the app |

</details>

<details>
<summary><strong>All commands</strong></summary>

```bash
slate                         # interactive hub
slate setup                   # guided setup
slate setup --quick           # non-interactive, defaults
slate setup --only starship   # retry a single tool
slate theme                   # live preview picker
slate theme <name>            # apply by name
slate theme --auto            # follow system dark/light
slate font                    # Nerd Font picker
slate font --list             # read-only candidate list and download catalog
slate font --list --json      # versioned font inventory, including partial-scan evidence
slate font --list --search "mono jetbrains"  # filter families and catalog IDs
slate font jetbrains-mono --dry-run  # preview file changes; no download or write
slate config set opacity frosted  # solid / frosted / clear
slate config set sound off    # toggle feedback sound
slate config get sound        # read one preference, without changing files
slate config list --json      # list resolved preferences, defaults and read errors
slate config pairing --json   # inspect saved dark/light pairing without desktop queries
slate config pairing --dark nord --light catppuccin-latte --dry-run  # preview only
slate config pairing --dark nord --light catppuccin-latte  # save pairing only
slate config pairing --clear-dark --clear-light --dry-run  # preview removing both overrides
slate export                  # export current config as URI
slate export --raw            # one plain share-code line for scripts
slate import <uri>            # re-apply from URI
slate import <uri> --dry-run  # inspect requested settings without applying
slate import <uri> --dry-run --json # versioned, profile-independent preview
slate share                   # screenshot terminal with watermark
slate about                   # identify this binary and its built-in capabilities
slate about --json            # read-only build/capability report, no profile required
slate status                  # show current config
slate status --json           # read-only saved settings and preview recovery summary
slate doctor ghostty          # inspect Ghostty config issues
slate doctor ghostty --files-only --json # inspect references without launching Ghostty
slate doctor kitty            # inspect theme includes and live-reload settings
slate doctor alacritty        # inspect TOML and theme imports
slate doctor opencode --json  # inspect Slate's selected TUI file and theme setting
slate doctor opacity --json   # compare saved opacity with generated files, without writes
slate doctor font --json      # inspect saved family, candidate scan and generated font files
slate doctor nvim --json      # inspect the active Neovim profile as JSON
slate doctor nvim --check-version --json # explicitly run a bounded version check too
slate doctor zsh              # inspect the active .zshrc and Slate loader
slate doctor bash --json      # inspect the startup file selected by Bash setup
slate doctor fish             # inspect Slate's conf.d loader and managed environment
slate doctor auto-theme --json # read-only watcher, launcher and conditional theme choices
slate list                    # list available themes
slate list "rose dawn"        # search IDs, display names and families
slate list --appearance light # keep only light themes
slate list catp --json        # versioned, read-only catalog for scripts
slate list --ids              # canonical IDs only, one per line
slate completions zsh         # print static completions (also bash or fish)
slate restore                 # pick a snapshot to roll back
slate restore --list          # list restore points
slate restore --list --all    # include undo checkpoints
slate restore --list --json   # read-only history summary and issues
slate restore <id> --dry-run   # preview snapshot changes without restoring
slate restore <id> --dry-run --json  # machine-readable restore preview
slate clean --dry-run         # inspect cleanup without writing files or stopping tools
slate clean --dry-run --json  # machine-readable cleanup plan
slate clean                   # back up, then remove Slate hooks and managed files
```

`slate config pairing [--json]` inspects saved dark/light slots. It does not resolve
the current system appearance or infer a saved value for an unset slot. Add
`--dark <ID>` and/or `--light <ID>` to save exact catalog IDs of the matching
appearance; the unspecified slot, unrelated TOML fields, comments and permissions
are preserved. `slate list --appearance dark|light --ids` lists choices, and static
completion filters each option accordingly. Add `--dry-run` to preview selected
changes without writes or a lock. Reads/previews remain available during another
writer or pending preview recovery; real saves are blocked in those states.

Use `--clear-dark` and/or `--clear-light` to remove saved overrides and return those
slots to normal automatic fallback. You can clear one slot while setting the other;
setting and clearing the same slot is rejected. Clear supports the same `--dry-run`
and JSON receipts. It does not disable the watcher, change the active theme or force
brand defaults: fallback may still use the current theme or its catalog pair.
Inspect `slate config pairing` afterwards to see the conditional choices.
Only selected keys are removed; the file itself is retained, even if it becomes
empty. Clearing an already-unset slot creates neither a new preference file nor a
restore point (ordinary writer-lock setup may still occur). Existing unrelated data
and permissions are preserved; comments attached to removed assignments are kept as
standalone footer comments, so their positions may move. Unknown string IDs can be
cleared, but malformed TOML or non-string pairing fields must first be corrected.

Inspection also explains the automatic choice for **each possible appearance**,
without querying the desktop. Additive JSON v1 `resolution.dark`/`resolution.light`
entries have `resolved`/`error` status, requested and selected appearance, a known
theme ID and `source`: `configured`, `current_theme`, `catalog_pair` or
`brand_default`. Defaults explain missing/unknown current tracking or a missing
catalog pair through `fallback_reason`. Saved-slot `unset` remains separate from
these conditional choices; previews and save receipts do not include resolution.
This is selection guidance, not a check that target files can be applied or that a
running watcher is healthy. Pairing and current tracking are separate observations,
not an atomic configuration snapshot.

The same selection policy is used by `theme --auto` and the watcher. Known stored
overrides and catalog self-pairs (such as Nord) remain authoritative even when the
selected theme has a different appearance; the report exposes that difference.
Unknown selected pairing IDs fail rather than silently falling back or echoing
their contents. Automatic selection now uses the same strict regular-file read
contract as inspection: 256 KiB for `auto.toml`, and 4 KiB for `current` only when
a fallback needs it. Final links, special files and isolated-profile escapes are
rejected; safe directory aliases remain supported. An unreadable current file
does not hide an explicit choice that does not depend on it. Actual application
still validates its write targets separately.

Saving changes only `auto.toml`, after a `pre-config` checkpoint; identical repeats
do not rewrite it or create another checkpoint. Invalid documents, unsafe paths or
backup failures stop before publication. Reads are bounded to 256 KiB and late file
or resolved-parent changes are checked before writing; external editors are not
locked out. JSON v1 uses `inspect`, `preview`, `saved` or `unchanged` actions,
per-slot `set`/`unset`/`error` statuses, before/after selections for changes and a
restore ID when a checkpoint was created. Unknown saved IDs and document errors
are reported without echoing their contents. Inspection issues still exit zero;
invalid requests or failed previews/saves exit nonzero.

The interactive `config set auto-theme configure` menu and hub now use this same
file-only save policy. Declining confirmation does not save, refresh shell files
or restart the watcher. **Saving pairing no longer restarts the watcher or applies
a theme immediately**, and does not change the enable flag. An existing watcher
reads the new pairing on the next appearance event; run `slate theme --auto` to
apply now. The interactive menu requires a terminal; scripts should use the new
options. File recovery does not restore processes or automatically apply a theme.

`slate config set fastfetch enable|disable` and `slate config set auto-theme enable|disable`
prepare generated shell files before changing the preference. A `pre-config`
checkpoint captures the affected files (including watcher helpers for auto-theme);
backup failure stops the update. Generated files are published first, the preference
last, preserving existing permissions and unrelated TOML values and comments.
Repeating an identical Fastfetch setting skips file writes and a new checkpoint;
auto-theme can still repair helpers or retry its lifecycle.

Errors distinguish helper preparation, file updates and watcher start/stop. If the
lifecycle fails after saving, the new preference stays saved; Slate does not reset
it silently. Inspect `slate doctor auto-theme` and the printed
`slate restore <id> --dry-run` before file recovery. Checkpoints restore files only,
not processes. These stages are not an all-files/crash-atomic transaction: late
failures may retain earlier writes. Captured inputs and upcoming output files are
rechecked, but external editors are not locked out. This applies to these enable/
disable commands, not the separate auto-theme pairing/configure flow or every
configuration setter.

`slate config get <key> [--json]` reads one preference; `slate config list [--json]`
reads all five public keys without creating files, taking a writer lock, starting
tools or initializing sound. Values are resolved preferences, including defaults,
not a claim about live terminals/editors/watchers. Missing opacity is `unset`, not
an inferred preset. With no stored settings, auto-theme/Fastfetch are disabled and
sound/editor setup permission enabled. `editor` describes future setup consent,
not an installed hook; `auto-theme` is its enable flag, not the configured pairing.
JSON v1 entries include `key`, typed `value`, `status` (`ok`, `unset`, `error`),
meaning, source path and `set_values` (accepted set actions, not stored-value types).
Errors have a null value and do not hide valid preferences. Inspect entry statuses:
like doctor, an emitted report exits successfully even when it contains issues.
Invalid command arguments and output I/O failures still fail; an early pipe reader
exit is normal. Reads remain available during a writer or pending preview recovery.

Inspection refuses final links, special files and isolated-profile escapes; ordinary
safe parent aliases remain supported. Documents are bounded to 256 KiB, state files
to 4 KiB, and configuration contents are omitted from errors. Reads are independent,
not an atomic snapshot. Fastfetch's shared marker getter also rejects unsafe files
and state over 4 KiB, rather than treating a directory as enabled or a broken link
as disabled; bounded ordinary payloads retain presence semantics. Invalid set/get
keys and set actions are rejected before profile or lock setup, with bounded,
display-escaped messages. Static completion includes get/list and get/set keys.

Auto-theme doctor and `config get/list` share the strict preference reader and TOML
field validation. Broken parent links and isolated-profile escapes report unknown/error,
not a disabled default, even when the final file appears missing. Valid parent aliases
inside the profile still work. Auto-theme and sound inspections reject final file links
at open time as well as during path validation; ordinary runtime getters retain their
existing linked-dotfile compatibility. Neither diagnostic initializes configuration,
sound or a watcher, and separate reads are not an atomic snapshot.

Font copies from Caskroom recovery or downloaded releases are staged as a complete
family before installation. Identical existing files are kept; differing files or
unsafe links stop the copy without replacement. Mid-batch failure rolls back this
operation's unchanged additions; externally changed files are retained with review
paths. Direct downloads use HTTPS-only redirects and bounded curl execution, with
implicit curl configuration disabled. Rust checks ZIP/ZIP64 paths, types, sizes
and font CRCs before publication; system `unzip` is no longer required. These checks
do not authenticate the upstream publisher, cover Homebrew's own installation or
guarantee native font activation.

Linux cache refresh is a separate, bounded step. Missing `fc-cache`, timeouts or
refresh failures produce a warning while keeping installed fonts; they do not
trigger another download. Setup and the font picker report the actual outcome.
Selecting an already-installed font does not run or claim a new cache refresh.

On Linux, user-font discovery, file installation and cache refresh share
`$XDG_DATA_HOME/fonts`, defaulting to `~/.local/share/fonts` when the variable is
unset, empty or relative. Absolute custom data roots can live outside HOME;
explicit root aliases are resolved, while a linked `fonts` child stops writes
and refresh. Installation also rejects parent traversal in the configured root.
Old fonts are not moved or removed, and the old default directory is not treated
as active when an override is selected. Legacy `~/.fonts` and the existing system
search roots remain. macOS still uses `~/Library/Fonts`; `SLATE_HOME` isolation
ignores the external override. Arbitrary native Fontconfig search configuration
and `XDG_DATA_DIRS` are not interpreted by Slate.

Font discovery now includes nested directories, normal linked fonts and
case-insensitive `.ttf`, `.otf`, `.ttc` and `.otc` names. A regular-file and small
header check filters out directories, empty files and obvious non-fonts. If a scan
is incomplete, known candidates stay selectable but missing-font downloads are
held until the scan issue is resolved. Candidate names still come from filenames;
this is not OS registration, internal family-name or glyph-coverage verification.

Use `slate font --list [--json]` to inspect candidates without opening a picker,
reading saved settings or acquiring a writer lock. It stays available during
pending recovery and reports partial scans with known candidates intact. Catalog
entries are separate: `candidate_found`, `not_observed` or `unknown`; unknown
entries are not offered for download by the picker. These are filename-derived
observations, not native installation or glyph-coverage claims. JSON includes
exact candidate family strings, catalog IDs and matching names, search roots,
scan issues and lossiness flags. A successful exit means report production;
check `scan_complete` before inferring absence. Listing never selects, installs
or refreshes anything. `--json` requires `--list` or `--dry-run`, and a font name cannot accompany
`--list`. Choose later with `slate font -- '<exact family>'`; a catalog choice may
install fonts. Static shell completions include the inspection flags.

Use `slate font --list --search "mono jetbrains" [--json]` to narrow the view.
Search ANDs case-insensitive alphanumeric terms across family names and catalog
IDs; punctuation separates terms and their order does not matter. It is substring
search, not fuzzy selection. Empty/whitespace searches show everything, while
symbol-only queries match nothing; use `--search=---` for leading-dash values.
Queries are limited to 256 bytes. The full scan still runs: search does not alter
scan issues, catalog presence, matching-family evidence or download eligibility.
No matches means only that the query matched no displayed choices, not that a
font is absent. Schema-v1 JSON adds `search` only when supplied, with the original
`query`, total/matched candidate counts and total/matched catalog-entry counts.

The picker shares this model, retains all JetBrainsMono variants and never derives
the selected family from display decorations. Literal names ending in
`(recommended)` or `(not installed)` are kept as data, not stripped as UI badges.

After a complete scan, an unknown name offers up to three nearby exact-family
hints, labeled as observed candidates or catalog choices that may download.
Ambiguous aliases list matching exact names instead of choosing one. Hints are
advisory and display-escaped, never shell commands or automatic selections;
inspect your chosen exact name with `--dry-run`. The same guidance appears in
the preview's `blocker.reason`; incomplete scans retain their existing block.
Suggestions score at most 4096 unique observed names in sorted order and disclose
any truncation; `--list` retains the full captured inventory.

Preview a named selection with `slate font jetbrains-mono --dry-run [--json]`.
Unlike listing, this reads your current configuration and uses the same resolver
and prepared transforms as application. It lists ordered file actions (`create`,
`update`, `unchanged`, `preserve_absent`) and byte counts without exposing file
contents, plus whether catalog installation, a pre-font checkpoint or a
session-eligible Ghostty reload would be requested. `--dry-run` requires a name
and cannot be combined with `--list`; it never opens the picker.

A produced preview exits successfully even when blocked: check
`file_plan_complete` and `blocker` in schema-v1 JSON. Only the first observed
blocker is reported; incomplete plans contain no file actions. Known candidates
remain usable during partial scans, but unknown catalog selections stay blocked.
Preview acquires no lock, creates no backup, writes nothing and launches no tool;
it is available during writer contention or pending recovery. It does **not**
test those mutation gates, write permissions, backup creation, network access or
native rendering (`execution_readiness_checked: false`). A complete projection
is not a reservation or a guarantee that later application will succeed.

Font selection now prepares terminal and Shell output before writing. A changed
selection creates a `pre-font` file-recovery point **before** any catalog download;
an unreadable/unsafe target, invalid Alacritty document or failed checkpoint stops
application. Limits are 8 MiB per terminal/Shell file, 256 KiB for preferences and
4 KiB for tracked state. Writable final symlinks and conflicting target aliases
are rejected; absent optional terminal entry files stay absent. `current-font`
is published last, and session-eligible Ghostty reload is requested only after
all writes succeed. Unchanged files retain bytes, modes and identity; an identical
repeat needs no new checkpoint. Imports reuse their wider `pre-import` point.

Explicit names and interactive selections use the same application flow: a
failed download exits with an error and does not begin font configuration writes.
Installer font-file/cache changes can still remain. `--quiet` hides font success
receipts and download progress, not the picker prompt, errors, cache warnings or
recovery guidance on stderr. `--auto` suppresses the new-shell reminder and sound;
ordinary successful selections show the reminder in both modes. Basic Starship
selection is based on the family name, not verified glyph coverage.

Homebrew **font** installation has a 10-minute wait limit after process startup
and a 512 KiB combined stdout/stderr limit. Timeout, excessive output, signal
termination or an unconfirmed capture stops automatic shared-cache/download
fallbacks. Inspect Homebrew before retrying: font files, package records or caches
may already have changed. Ordinary completed failures keep the existing fallback
order. Setup and direct/picker/import selection now share that policy: all-path
failures retain each attempted stage's reason, including shared-cache failures.
Recovered failures remain notices (also visible in quiet font selection), and
setup reports the successful source. A cache-refresh warning after successful
font publication does not trigger another installer or download. Native
Homebrew output is classified but not echoed. These are not filesystem/spawn/
OS-termination deadlines.

Homebrew **tool** installs share this execution path with separate limits:
30 minutes after startup and 2 MiB combined output. An unconfirmed tool install
stops the remaining setup installers and configuration steps, including any
Starship local fallback. Setup reports its existing file-recovery checkpoint;
earlier installs/configuration are not automatically undone. Ordinary completed
permission failures retain the existing Starship fallback policy. These changes
do not sandbox Homebrew or detached subprocesses.

Linux apt tool installs now use the same bounded capture mechanism: 30 minutes
after startup and 2 MiB combined output. Effective root invokes apt-get directly;
other users use `sudo -n` without a password prompt in Slate. If authentication is
needed, authenticate in your own terminal and retry Slate there as your normal
user, or ask an administrator to install the package. Slate does not weaken sudo
policy or retry through a privileged shell. Debconf uses noninteractive defaults;
package scripts/conffile decisions can still fail and need manual attention.
`--no-remove` refuses transactions requiring package removal. No automatic
repository update, lock deletion or dpkg repair is performed. Normal failed exits
report an issue and may leave partial changes; setup can continue independent
steps. Unconfirmed capture, timeout, output overflow or signal termination stops
remaining setup. Elevated/detached installers may still be running: inspect
apt/dpkg before retrying, without deleting lock files. These are post-spawn wait
limits, not OS termination deadlines or package rollback. Errors classify bounded
native output without echoing it. Package availability depends on configured
repositories. On the apt backend, Starship directly uses the staged user-local
installer below, without first attempting sudo/apt.

The user-local Starship fallback now stages the official installer in a private
temporary directory. Fetching the script uses HTTPS-only redirects, disables curl
configuration and allows 75 seconds after startup / 1 MiB combined output; the
installer allows 10 minutes / 2 MiB. Slate validates a nonempty, owner-executable,
regular non-symlink binary (at most 64 MiB), then atomically replaces
`~/.local/bin/starship` with mode `0755`. HOME aliases are resolved, but linked or
non-directory `.local`/`bin` components and linked/unsafe targets are refused
before download. Target identity/content is rechecked before publication so an
observed external change is not overwritten. Unconfirmed installer execution or
publication stops the remaining setup. No binary is executed for validation.
This still trusts the upstream script: staging is not a sandbox, an extraction
disk quota, authenticity/runtime verification or exclusion of external writers.
Pre-publication failures do not make Slate replace the old binary, but arbitrary
installer side effects and newly created empty directories can remain. File
recovery does not uninstall or restore this executable.

Use the printed `slate restore <id> --dry-run` to inspect recovery, then explicitly
restore if needed. A late failure can leave earlier writes in place: this is not
automatic multi-file rollback, crash atomicity or exclusion of external editors.
Recovery covers captured configuration bytes, modes and absence, not installed
fonts, external caches, empty directories or live windows. A successful config
write/reload request is not proof of native font matching or rendering.

If a selected font is not appearing, run `slate doctor font [--json]`. It reports
the saved family, selected user-font directory, all search roots and bounded scan
issues, then compares Ghostty, Alacritty and Kitty font outputs against the same
serializers used by their writers. Missing files can be normal for unused
integrations; differing bytes are not a syntax or runtime judgment. A positive
candidate remains reportable during an incomplete scan; a negative result never
proves a font is uninstalled, because discovery uses filenames and a limited
system-family whitelist. Review include chains, overrides and native font matching
separately. No terminal, cache builder, installer or writer is launched, including
while a write lock or recovery record is present. Valid saved family names are
shown; invalid state and generated file contents are omitted. JSON adds a bounded
`font_inventory` summary to the versioned diagnostic report. Exit success means a
report was produced, not that every check passed.

The same diagnostic now also checks direct terminal entry references using the
font receipt's shared bounded reader/parser. Schema-v1 JSON adds `font_references`:
one summary for Ghostty, Alacritty and Kitty, each with its managed target, selected
candidate entries, `state`, `inspection_complete` and loss-aware paths. States are
`found`, `not_found`, `missing` and `uninspectable`. A known positive entry remains
`found` if another entry is unreadable, with `inspection_complete: false` and the
individual warning retained. Missing optional entries can simply be unused tools.
Generated bytes and direct references are separate evidence, not activation proof.

Entry checks follow regular file links read-only, reject targets outside isolated
`SLATE_HOME`, and cap each UTF-8 file at 8 MiB. Invalid TOML, NUL/binary contents,
unsafe files and Ghostty's pinned line/reference limits are unknown, not missing
references. Ordinary profiles may inspect configured external entry links; accepting
a link for diagnosis does not authorize replacing it during an application. These
checks never traverse includes or launch native validation. Relative/variable or
nested references, cross-file resets, overrides and actual rendering remain outside
this check; inspect the per-entry evidence before deciding to reapply or reinstall.

`slate doctor ghostty [--json]` retains candidate selection, duplicate managed
references and literal include-cycle detection. Its scan now reads regular UTF-8
files with limits of 8 MiB/file, 32 MiB total, 256 paths, 4096 references and 64
include levels. Ordinary symlinked configs remain supported; references resolving
outside `SLATE_HOME` are reported without reading their contents. FIFOs, dangling
links, unreadable/oversized files and scan limits produce `scan_issues` and
`scan_complete: false`, not a claim that no cycle exists. Missing files alone do
not imply a cycle. JSON retains the existing fields and adds `schema_version: 1`,
`scope` and path-lossiness indicators; `cycle_risk` describes only observed findings.

Entry candidates follow the pinned [Ghostty 1.3.1 default-file order](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/config/Config.zig):
XDG `config`, then `config.ghostty`; on macOS, App Support `config`, then
`config.ghostty`. Slate writes managed references to the last existing candidate,
and still uses XDG `config.ghostty` when none exists, including on macOS. The
adapter, font/config detection, doctor labels and backup keys share this path list;
existing backup keys keep their original meaning. JSON's `entry_order` names the
pinned ordering baseline. This is Slate's write-target policy, not an installed
version probe or a simulation of native preferred-file checks, command-line
overrides, recursive load order or window state. In particular, native macOS
preferred-file checks can skip an empty or unreadable current App Support file.

Ghostty window layout remains your choice. Slate updates the palette and
`window-theme` light/dark appearance, but no longer writes `macos-titlebar-style`.
Choose that option in your own Ghostty config or an included file, not Slate's
regenerated `managed/ghostty/theme.conf`. Applying a theme with this version removes
the older forced `transparent` line from that generated file. With no explicit
choice, Ghostty uses its own default (`transparent` in 1.3.1).

`slate doctor ghostty` now points out `macos-titlebar-style` assignments still
present in a referenced, regular `managed/ghostty/theme.conf`, with the file and
first assignment line. Schema-v1 JSON adds `window_style`: `managed_override`
means an assignment was observed, `not_found` means none was found in a complete
scan, and `unknown` means an incomplete scan found none. Positive findings survive
other scan failures with `inspection_complete: false`. This advisory neither
makes the reference scan incomplete nor prevents native syntax validation.
Values are omitted from the advisory; user-owned settings, lookalike paths,
unreferenced files and targets of managed-file symlinks are not attributed to
Slate. Inspection uses the same bounded include scan and does not change files.
Reapply your desired theme with this version to regenerate an old managed file;
the diagnostic does not claim which style is effective or whether tabs look right.

If the macOS tab strip looks gray against a colored terminal, a successful theme
apply does not prove that native tabs match the palette. `transparent` retains
native controls; `tabs` integrates tabs into the titlebar, changing the layout.
Compare in a new window after reloading: Ghostty applies titlebar-style changes
only to new windows. System tab materials and selected/unselected contrast still
need visual checking; Slate does not recolor native widgets or automatically
reopen windows. See [Ghostty's titlebar options](https://ghostty.org/docs/config/reference#macos-titlebar-style).

Reference parsing follows Ghostty 1.3.1's
[line reader](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/cli/args.zig)
and [path parser](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/config/path.zig),
not Shell syntax. Unquoted spaces, `#`, `=`, single quotes and backslashes remain
literal path data; surrounding double quotes are handled by both upstream stages.
UTF-8 BOMs are accepted. Optional `?` references may be absent; missing required
references produce `required_file_missing`, even after an optional reference to
the same missing file. Optional does not waive safe-read checks. Absolute and `~/`
file links retain their nested-relative base; existing relative links resolve to
the real file first. JSON's `reference_syntax` names this reference-grammar baseline,
not the installed binary's version or the adapter's entry-selection policy.
List resets discard earlier local references but report `reference_reset`, because
cross-file reset order is not modeled. A line exceeding the upstream reader's
4094-byte input capacity produces `line_limit`; subsequent lines are not scanned.
These limitations are explicit incomplete scans, not evidence of invalid native syntax.

Ghostty font attachment and Slate-reference cleanup now share this literal
grammar with diagnostics. A path embedded in an unrelated value is not a managed
reference; single-quoted relative values and `..` ownership ambiguities are kept.
Font attachment checks references after the last local list reset and does not
duplicate a valid later reference merely because an earlier legacy hint exists.
Cleanup preserves other bytes and the leading UTF-8 BOM. These byte transforms
do not expand arbitrary paths or perform whole-file/native validation.

Font receipts report **observed** direct references, not effective activation.
Ghostty uses complete literal values and local resets; Kitty uses its logical
continuation lines and complete include values; Alacritty retains effective import
precedence. An uninspectable entry is distinguished from an absent/unlinked file.
The receipt does not traverse include chains or verify cross-file resets, overrides,
native font matching or window rendering. Use the relevant doctor for further checks.

On a complete, non-isolated profile it still runs the selected Ghostty's
`+validate-config`, with closed stdin, a five-second post-spawn deadline and a
64 KiB combined stdout/stderr capture limit. Timeout or excess output terminates
only that invocation's process group and reaps its leader, including when an
inherited pipe remains open. Validation distinguishes `passed`, `failed`, `skipped`,
`timed_out`, `output_limit` and `error`; JSON exposes the limits and truncation flag.
Text escapes terminal controls and shows at most eight nonempty native output lines;
native output may contain configuration values, so review before sharing. A complete
report exits zero; scripts must check both scan completeness and validation status.
This is a best-effort file scan, not a full Ghostty parser or a live-window check.
Slate does not edit settings, but native validation runs Ghostty itself. Isolated
or incomplete scans do not launch it. Filesystem IO/OS process teardown have no
hard wall-clock guarantee, and external edits are not locked out.

Font application and theme reapplication use terminal-specific serialization:
Alacritty gets valid TOML, Ghostty keeps its literal quoted values, and ambiguous
Kitty names use the explicit `family=` syntax (Kitty 0.36+). Ordinary Kitty family
names retain their legacy format. Control characters, line separators and names
over 256 bytes are rejected. Exact installed family names take priority over loose
aliases; ambiguous aliases require an exact choice instead of silently selecting
another family. Unset fonts fall back only to discovered installed families, not
the picker's “not installed” recommendation label.

Setup's initial font/theme hints are read-only and use the supplied profile,
including custom XDG directories. Font hints inspect direct Ghostty entries first,
then the adapter-selected Alacritty TOML file (also accepting inline/dotted font
tables); imports and live window settings are not inspected. Reads require regular
files, with 8 MiB per terminal config and 4 KiB for the saved theme. Unsafe or
unreadable sources provide no hint; font names must pass the same validation as
writers, and control-bearing theme IDs are omitted. Ordinary dotfile links remain
supported, but isolated profiles do not read linked sources outside their resolved
home. This does not repair invalid settings, approve them for later writes, impose
a filesystem IO deadline or lock out external directory edits. Startup inspection
alone creates no directories and launches no tools.

The wizard retains that profile for tool selection and uses its captured theme and
session for hints/review rendering; it does not reread another profile for those
screens. `slate setup --only <tool>` retries installation only, without reapplying
themes or shell integration. It keeps the supplied profile for installation and
returns failure when preflight or installation fails. Exact-tool retry checks only
OS, architecture and the selected installation route; it does not scan unrelated
fonts/tools, probe DNS, check shell integration or create a configuration write
probe. The normal write guard and installer-specific checks still apply.
Unknown/detect-only targets are rejected before profile/lock/sound initialization.
For full setup, the write-permission probe creates and removes its own random temporary file in the configured XDG
directory; it never reuses `.slate_preflight_test`, and individual write targets
still need later checks. A profile is not a package-manager sandbox: executable
discovery and system package installations still use the host's existing backend.

On macOS/Linux without Homebrew/apt, `slate setup --only starship` can use the staged
user-local installer directly. Guided setup no longer requires a package manager
before you choose anything: its inventory labels manual-install-only tools, offers
only supported automatic-install routes, and preserves configuration choices for
already-detected tools. Standalone font-download intent does not itself require a
package manager. Quick derives its core tools from the supported shell named by
`SHELL`: Starship for Bash/Fish, plus zsh-syntax-highlighting for Zsh. Bash/Fish
do not select the Zsh-only plugin for installation or theme configuration, so its absence
does not require a package manager or imply a download. Existing Zsh files and
preferences are not removed; Manual and `--only zsh-syntax-highlighting` retain
explicit installation support. Quick still requires routes for every missing
core tool relevant to that shell; it will not silently drop unavailable tools.
This also applies to Quick chosen inside the wizard. Final selections are checked
again before snapshots, preferences or installation; `--force` does not bypass
route validation. A supported route does not prove curl/network access, writable
targets, package availability or runtime success: installers check their own
dependencies. Network preflight reports DNS evidence, not verified download access.

The confirmation card now shows each tool's actual route: Homebrew formula/cask
and package name, the mapped apt package and administrator access, or the full
user-local executable destination. Starship's known-failure Homebrew fallback is
shown before confirmation, together with the configuration-only backup scope.
Those tool actions are retained in the execution plan. Changed selections,
package metadata, local destination or routes require a new review before the
snapshot; routes are checked again before execution and before each tool starts.
Execution consumes the captured route rather than choosing another backend.
A later change stops remaining setup and retains existing recovery guidance;
earlier completed/partial changes are not undone. This does not pin package
versions, executable identities or repositories, freeze font installation paths,
exclude external changes or sandbox installers. The disclosed Starship fallback
still applies to known Homebrew failures; unconfirmed installer outcomes stop.

Full setup also returns a failure status if a requested installation, font choice,
theme/shell stage or Neovim activation step fails; a partial run no longer emits the
whole-setup completion event. Optional notices and deliberately skipping Neovim's
activation line do not fail setup. Font availability and a saved font choice are
reported separately, neither proving live rendering. SSH/isolated receipts do not
claim local window activation. Fastfetch/opacity preference-write errors stop the
run before installation. Neovim marker inspection accepts regular files up to
8 MiB (including non-UTF-8 surrounding bytes), and reports unsafe/unreadable sources
instead of silently treating them as an absent installation.

If setup fails after its safety snapshot, the error includes
`slate restore <id> --dry-run` for inspecting captured file recovery. Earlier
successful changes remain in place; there is no automatic rollback. File recovery
does not uninstall packages or fonts or restore live windows.

After confirmation, setup prepares a read-only execution plan before its safety
snapshot, preference writes or installation. Unknown installation/configuration
targets, detect-only installation requests, malformed font names, invalid themes
and unsupported shells are rejected early; saved themes are checked when keeping
the current selection. Repeated tool IDs run once in
their original order; known font display names resolve to their catalog IDs.
Execution keeps the planned profile, theme and shell instead of resolving them
again. Successful shell-only setup also saves its chosen theme after connecting
the loader. Planning does not verify installed fonts, network access or all write
targets, and does not prevent external filesystem changes during execution.

Setup also prepares the selected shell's loader before its safety snapshot or
installers. It accepts regular files up to 8 MiB, rejects final symlinks (including
dangling links), unsafe parents and malformed Bash/Zsh marker blocks, and checks
the generated size. Execution rechecks the captured bytes, ordinary permissions,
file identity and resolved parent path before installing tools, before theme/shell
work and before loader publication. Detected changes require a fresh setup run.
Loader updates use atomic replacement, preserve existing ordinary permissions,
create new entries with mode 0600, and leave identical files untouched. Bash/Zsh
preserve non-managed bytes; Fish's Slate-owned `conf.d/slate.fish` remains a whole
generated file (not `config.fish`). This is not a setup-wide transaction: later
write failures can leave earlier tool/theme changes and created directories in
place, with the existing file-recovery guidance. Parent-directory synchronization
is best effort, and external writers are not excluded.

`slate doctor bash|zsh|fish [--json]` uses the same startup-file selection as setup
and checks its managed `env.<shell>` separately. macOS setup targets login Bash:
it selects the first present `.bash_profile`, `.bash_login` or `.profile`, in that
order, and creates `.bash_profile` only if no login entry exists. This prevents a
new file from shadowing an existing profile under
[Bash's startup rules](https://www.gnu.org/software/bash/manual/html_node/Bash-Startup-Files.html).
Unlike Bash's runtime readable-file search, Slate deliberately stops on an unsafe
or unreadable higher-priority candidate rather than writing a different entry.
Linux continues to target `.bashrc`. Existing `.bashrc` files are left intact on
macOS; setup does not add login/non-login chaining or migrate old loader blocks.
The shared `.profile` gets a Bash-version guard around only Slate's source block
so other shells can keep using its user content. Snapshots and clean/restore cover
all four Bash entries; cleanup removes only the managed marker block. Setup
rechecks both the selected entry and managed environment path before writes, so a
new higher-priority file requires a fresh run. This is not concurrent-writer exclusion.
Doctor reports the platform's convention and creates no files. Zsh honors the
selected ZDOTDIR; Fish checks only Slate's `conf.d/slate.fish`, not `config.fish`.
The checks report missing/unreadable/oversized files, final links, special files,
isolated-profile escapes and malformed Bash/Zsh markers. Files are read separately,
up to 8 MiB each, without interpreting or echoing source bytes. Shell diagnostics
permit opaque non-UTF-8 file contents; non-UTF-8/multiline paths make literal-reference
comparison unavailable. No shell or installer is launched and no configuration is
repaired, even while a configuration writer is active.

A matching literal `source` line (or `.` for Bash/Zsh) is only textual evidence,
not proof it executes: startup precedence, shell syntax, control flow, functions,
heredocs, multiline quotes and indirect/dynamic sources are not evaluated. Printed
paths, comments and assignments no longer count as source-line matches. JSON v1
adds stable check codes; text and JSON carry the same scope and next-step guidance.
All three static completion scripts include the new doctor targets.

Tool-version probes (including Neovim activation checks) now have a two-second
post-spawn deadline and a 64 KiB combined stdout/stderr cap. Timeout, excessive
output, nonzero exit or invalid output never establish a version from a partial
prefix. Errors omit raw program output. Ghostty validation shares the same
capture/cleanup implementation while retaining its five-second deadline.
Only the probe's owned process group is terminated on timeout/overflow; spawn,
filesystem IO, OS termination and deliberately detached descendants are outside
the hard-time-bound guarantee. Minimum thresholds are unchanged.

Command lookup now requires a regular file and execute access for the current
effective user/group, following ordinary symlinks. A non-executable same-name
file, directory, FIFO or broken link no longer masks a later usable candidate.
Existing PATH/fallback ordering and tool aliases are retained; Homebrew discovery
uses the same check. Lookup does not execute candidates or inspect their contents.
It is advisory, not an authorization boundary or a guarantee of executable
format, interpreter availability or a later successful launch. If Neovim has no
usable executable but a rejected/inaccessible candidate remains, activation
reports an error and its explicit version diagnostic retains the candidate path,
instead of suggesting that Neovim is simply missing.

Version parsing now requires a complete `MAJOR.MINOR.PATCH` on the first non-empty
output line, with an optional `v` prefix and valid prerelease/build metadata.
Known executable names (Neovim, Ghostty, Alacritty) must match that header. Invalid
headers are errors; later dependency/compiler versions cannot substitute for the
tool's version. Truncated numbers, extra components and invalid suffixes are not
repaired. Comparison follows [SemVer precedence](https://semver.org/spec/v2.0.0.html):
`0.8.0-dev` is below the Neovim `0.8.0` floor, while `0.8.1-dev` is above it, and
build metadata does not affect the decision. This lower-bound check is not a
stable-release allowlist or a runtime compatibility guarantee for development builds.

Neovim availability checks use the executable found for the selected profile,
including home-local fallbacks outside PATH. Setup's activation stage probes it
once, then reuses the result for hook handling and its receipt. Missing or old
Neovim remains an ordinary skip; a failed version probe is an error, not an
instruction to reinstall. That failure prevents Neovim setup writes and makes
setup incomplete (earlier setup stages are not rolled back). Theme application
also reports the check failure instead of silently skipping it. Remembered manual
activation still bypasses the activation-stage probe entirely. Without an explicit
`--check-version`, `doctor nvim` remains file-only and does not launch a version probe.

Use `slate doctor nvim --check-version [--json]` to diagnose version problems without
rerunning setup. The optional `version_probe` JSON object reports `supported`,
`unsupported`, `missing` or `failed`, the selected executable and PATH/fallback
origin, the parsed version, minimum version and shared probe limits. The same
result appears in a `version_probe` check. Missing executables have a null binary;
failed checks have no accepted version. Non-UTF-8 binary display paths are marked
lossy. A failed check gives its reason, not an instruction to reinstall. Default
reports omit this object; schema version 1 and the existing file checks remain.

The flag is supported only for the explicit `nvim` target. It does not install
anything, write Slate configuration or change activation consent, and diagnostics
remain available while a Slate writer holds its lock. Explicit version checking
also works when automatic activation is disabled. It does launch the resolved
executable with `--version`: that executable is not sandboxed and may have its own
side effects. Only its parsed version/failure reason is shown, not stdout/stderr
bodies. A successful exit means the report was produced, not that every check
passed; inspect `version_probe.status` and the file-check statuses.

Alacritty application keeps the effective import list in place: a top-level
`import` takes precedence over `general.import`, matching the
[upstream loader](https://github.com/alacritty/alacritty/blob/master/alacritty/src/config/mod.rs).
Slate appends missing managed entries without merging, replacing or migrating
the user's lists. New lists use `general.import`; inline tables and dotted keys
are supported. Font changes clear only the main file's `font.normal.family`
override when this operation supplies a managed font, retaining style, size and
other font faces. Already connected, unchanged main files are not rewritten.

User-level Alacritty TOML entry paths are checked in this order, following the
[documented Unix locations](https://alacritty.org/config-alacritty.html#LOCATION):

1. `$XDG_CONFIG_HOME/alacritty/alacritty.toml`
2. `$XDG_CONFIG_HOME/alacritty.toml`
3. `$HOME/.config/alacritty/alacritty.toml`
4. `$HOME/.alacritty.toml`

Duplicate paths/directory aliases are collapsed. Setup reuses an existing entry
instead of creating a higher-priority shadow file; an obstructed preferred path
is surfaced, not silently bypassed. Detection, application, font reports and
doctor share this selection. System locations (including `XDG_CONFIG_DIRS`), YAML
and per-process `--config` overrides are not discovered or edited automatically;
check these manually before setup. With no user TOML candidate, setup retains its
default first-path creation behavior. `ALACRITTY_SOCKET_PATH` is not a config override.

Baselines, live-preview recovery and cleanup capture every user candidate,
including missing files. Cleanup removes Slate imports from inactive candidates
too, so later selection changes do not leave dangling hooks. An unsafe candidate
can block capture/cleanup. A newly created higher-priority file during live preview
is an external edit: Slate stops preview and preserves it. Setup's initial file
creation is exclusive and never initializes through an already present dangling
link; these checks are not a transaction against concurrent directory changes.

These Alacritty writers validate the selected import list and prepare the full
entry-file edit before publishing managed outputs. Entry files must be regular,
non-symlink UTF-8 TOML, with an 8 MiB input/output limit. Detected changes to the
entry's bytes, identity or ordinary permissions require retry. Publications are
still per-file, not an external-editor transaction; later I/O failures can leave
partial managed outputs. Font connection reports and `doctor alacritty` use the
same import precedence. These application diagnostics read bounded regular UTF-8 files,
report FIFOs/oversize input without opening them, and omit TOML source from errors.

OpenCode TUI theme edits preserve JSONC comments, whitespace and unrelated values,
including their numeric spelling. The adapter sets only the root `theme` to
`"system"` and adds `$schema` if absent. An already-system config is not rewritten
or backed up again by the adapter (the overall theme workflow may still capture a
restore point). Inputs must be regular, non-symlink UTF-8 files, at most 8 MiB;
malformed syntax, duplicate root keys or a non-string theme require manual repair.
Existing-file changes require backup and a source recheck; this is not a transaction
against external editors. `SLATE_HOME` isolation ignores host `OPENCODE_TUI_CONFIG`.
Cleanup uses the same parser and preserves even comment-only documents. It still
treats root `theme: "system"` as the integration setting, regardless of who set it;
inspect `slate clean --dry-run` first if you independently chose that theme.

`slate doctor opencode [--json]` reports Slate's selected entry, existing alternate
TUI candidates, unsafe files, invalid JSONC and system/custom/unset theme states.
It shares the adapter's path selection and edit-safety parser, omits configuration
values, and does not initialize settings, create backups or launch OpenCode. The
report is about that file, not the effective live theme: [OpenCode also supports
project TUI configuration and custom paths](https://opencode.ai/docs/config/#tui).
The system setting uses [the terminal's palette/default colors](https://opencode.ai/docs/themes/#system-theme);
this doctor does not test terminal color support. Relative overrides are resolved
to an absolute path once per `SlateEnv`; `relative_config` reports that conversion
as information. Changes to cwd/process variables do not retarget the captured
environment. A new invocation still resolves relative paths from its own cwd;
use an absolute override for consistent selection across invocations. Directory
aliases are captured once in clean/import; final file links are not followed.
`..` uses the real traversed directory, including directory links. An unresolvable
prefix or trailing `/` or `/.` is an explicit error, never a fallback to the default
file. Such errors remain visible as `unresolved_config` in doctor, do not break
unrelated diagnostics, and block relevant edits/checkpoints/clean before changing
configuration. File-only recovery uses its recorded targets even if the override changes.
Detection, application, doctor and recovery discovery share the injected override;
`SLATE_HOME` and `SlateEnv::with_home` ignore it. Integration doctors
(OpenCode, opacity, Kitty, Alacritty, Neovim, Zsh) retain `target` and `checks` in JSON and
add `schema_version: 1`, `scope`, and per-check `path_is_lossy`; OpenCode and opacity checks also
include stable `code` identifiers. A successfully printed report exits zero even
when checks contain warnings/errors; scripts should inspect `checks[].status`.
Text escapes terminal controls, lossy paths are labeled, and closed output pipes
are handled without panic. Diagnostics stay available during active writers or
pending preview recovery; they do not prove write permission or restore readiness.

`slate doctor opacity [--json]` compares the saved preset with Slate's four generated
Ghostty/Alacritty/Kitty opacity and blur files, using the same templates as the writers.
Missing or empty state is `preset_unset`, not an inferred default; unknown or non-UTF-8
state is `preset_invalid`. Recognized noncanonical spelling is reported separately.
Per-file codes distinguish `output_matches`, `output_differs`, `output_missing`,
`output_unreadable` and `output_uncompared`; use each check's `path` to identify the file.
Missing output is informational because an integration may be unused. A byte difference
(including comments or line endings) is not evidence of invalid syntax or a live fault.
It reads at most 4 KiB of state and 8 MiB per output, rejects final file links and
unsafe isolated-profile paths, and never prints file contents or starts a terminal.
Metadata checks also flag conflicting output aliases and blocked recovery-storage
layouts; a broken backup directory does not hide otherwise readable outputs.
No permissions probe, include-chain, user-override, compositor or live-appearance
validation is performed. Inspect manual edits first before using the suggested
`slate config set opacity <preset>` repair. The report is a set of observations,
not an atomic snapshot during concurrent edits, and follows the exit-zero contract above.

Before applying a shared code, try
`slate import "slate://nord/jetbrains-mono/frosted/s,h" --dry-run`.
The preview explains the request without reading your profile, discovering or
downloading fonts, or creating a lock. It works without HOME and during pending
recovery or another active writer. It is not a diff against your current settings
or a guarantee that application will succeed: font availability and write access
are checked only when applying. `none` in the theme/font/opacity positions means
keep the current value (`null` in JSON); the tool list replaces all three toggles,
so omitted tools are disabled and `none` disables all three. Actual imports still
apply in steps; a later failure does not automatically undo earlier changes.

Before changing settings or installing a requested font, actual import must save
a `pre-import` recovery point. It prints `slate restore <id> --dry-run` before
applying, and repeats the command on failure. Inspect that plan, then omit
`--dry-run` to restore the captured file bytes and ordinary Unix permissions;
files created by the import are removed when they were absent in the checkpoint.
These restores do not regenerate a theme over your original files. Coverage
includes saved settings, shared shell files and the selected adapters' config
outputs (including bat theme files), not installed fonts, external tool caches,
empty directories or running application state. Reload/restart tools as needed
after restoring. Recovery is not an automatic or concurrent-editor transaction.
Affected final symlinks, special files, unsafe isolated-profile paths, files over
8 MiB, or a total checkpoint over 64 MiB stop import before applying settings.
Recovery history is private under the profile's cache/backups directory; review
the plan before restoring, since later manual file edits would also be replaced.

`export` and screenshot sharing generate `slate://v1/theme/font/opacity/tools`.
The versioned font segment uses UTF-8 percent-encoding, preserving spaces, Unicode,
slashes, quotes and literal percent signs. Legacy four-part codes still work and
are not percent-decoded; recipients need a build supporting v1 to read new codes.
`export --raw` prints exactly one unstyled line; ordinary export suggests a safely
quoted preview command. Export reads only bounded regular settings files and
rejects invalid values, final symlinks and special files without printing their
contents. Unset theme/font/opacity is encoded as `none`; missing tool flags retain
Slate's defaults (Starship/highlighting on, Fastfetch startup off). Export does not
probe running tools or guarantee a consistent snapshot across concurrent writes.

`slate share` keeps existing images: it saves `slate-share.png`, then unused names
such as `slate-share-2.png`, without overwriting files, directories or links.
Capture and optional watermarking use separate private temporary files; cancellation
or an absent/invalid result never reports an image as saved. A failed watermark
warns and keeps the original capture. New exports are private (0600).
An existing `XDG_PICTURES_DIR` is used only when its resolved directory is inside
HOME; otherwise Slate tries an in-home Desktop, then HOME. Escaping aliases and
`..` paths are ignored. Portal screenshot sources are copied, never deleted.
Image reads require regular, non-linked files up to 64 MiB and check the PNG
signature; this is not full image decoding. Optional watermark processing has a
10-second post-spawn deadline and 64 KiB combined stdout/stderr limit; interactive
capture itself has no such deadline. `export --raw` remains available without capture.

On Linux, Portal result subscription precedes the screenshot request so a fast
response is not missed, including when an older portal returns a different handle.
Only the request's returned path and original service owner are accepted. Service
restart/disconnection ends waiting with an error; retry the command if needed.
Connection/version/subscription setup has a shared 2-second budget; waiting for
the screenshot interaction has no idle timeout. The connection remains open until
the borrowed image has been copied. Details and fixture limits are in CONTRIBUTING.

Ordinary saved-setting reads also reject special files, broken links and invalid
UTF-8: tracking files are limited to 4 KiB, and `config.toml` / `auto.toml` to
256 KiB. A missing file still means unset/default; malformed TOML reports its path
and location without quoting saved contents. Invalid sound preferences keep
feedback silent. Flag changes preserve comments and other fields, and an auto-pair
update leaves the unspecified side intact. Valid links to regular dotfiles remain
readable, but Slate's atomic writers refuse final symlinks; export/import retain
their stricter link policy. Baseline/current/pre-restore snapshots have an 8 MiB
per-file and 64 MiB total bound, retain binary bytes and ordinary permission bits,
and publish only after all copies succeed. Reading a linked dotfile does not make
that link a permitted restore destination. These checks do not provide a transaction
against external editors or a timeout for an unresponsive filesystem.

`list` searches case-insensitively, ignoring common accents and word separators;
all search terms must match. Search and `--appearance dark|light` can be combined.
Search is for discovery only: applying a theme still requires its full ID or display
name. `theme --list` remains the unfiltered compatibility alias.
Unknown names return an error with up to three advisory IDs, including close spelling
matches; Slate never applies a suggestion automatically. Invalid names, incomplete
`theme set` forms, and combining a theme name with `--auto` are rejected before
profile initialization, lock creation or sound. This also applies to the `set` alias.
Valid selections still respect active writers and pending preview recovery. Error
messages remain on stderr under `--quiet` so scripts can diagnose a nonzero exit.
JSON schema version 1 includes the query, appearance filter, count, and themes
(ID, name, family, lowercase appearance, description, and optional auto-pair ID).
`--json` and `--ids` are mutually exclusive and never read saved settings. No matches
is a successful empty result (`themes: []` / no ID lines); text explains how to broaden
the search. All catalog modes leave config/cache files untouched and stay available
during pending recovery. Piped output, `NO_COLOR`, and `TERM=dumb` contain no ANSI
escapes; exact palette swatches are shown only in truecolor terminals.

`status` does not initialize configuration, backup directories, or sound caches.
An unset or unknown theme is not presented as an applied default; invalid settings
are reported separately. Toolkit marks indicate availability, not integration health.
`status --json` reports saved settings, paths, warnings, and a content-free recovery
summary (`clear`, `busy`, `active`, `pending`, `conflicted`, or `unreadable`); `busy`
means another configuration operation holds the lock. JSON status does not probe
running tools. A successful status read can still contain warnings: inspect those
fields rather than treating exit code zero as a health check.

When a preview is unfinished, bare `slate` opens a recovery-first menu. Restoring
or keeping the current files requires explicit confirmation; Quit changes nothing.
A running preview cannot be recovered by this menu. Non-interactive invocations
only print recovery guidance. Direct theme, font, config, setup, import, restore,
and cleanup commands use the same writer lock: they fail before changing config
when another operation is running or a preview record remains. Status, doctor,
list, export, and restore preview/list commands remain available.

The preview-to-commit transition retains the lock without an unlock window, and
nested calls within one operation reuse it. Locks coordinate Slate processes
using the same cache root; use consistent HOME/XDG settings. They do not prevent
manual edits or coordinate independently configured cache roots. Do not delete
the lock file to bypass contention. Auto-theme keeps a pending appearance event
through contention or preview recovery and retries when configuration writes are
available again; disabling auto-theme cancels this pending work.

Auto-theme startup, status, stop, and Shell relaunch now use the same per-config
watcher identity. A private lifetime lock proves ownership; stop requests carry an
instance token, so stale records and unrelated same-named processes are not killed.
Profiles with different Slate config directories can share a cache without claiming
one another's watcher. Use the same cache root for a profile: independent cache roots
do not coordinate. Generated launchers pin HOME/XDG, ZDOTDIR, and NVIM_APPNAME from
setup; re-enable auto-theme after changing those bindings. Isolated `SLATE_HOME`
commands never start a desktop watcher.

The Rust watcher owns appearance events and theme writes. Its macOS helper only
emits events; GNOME uses an owned `gsettings monitor`, and portal sessions use D-Bus.
Pending appearance notifications are coalesced instead of accumulating a queue;
source failures remain visible. Native helpers accept only their backend's complete,
known records, with a 1 KiB per-record limit. Oversized records stop the source with
a content-free error; arbitrary helper stderr is not copied into the watcher log.
Idle native readers wait for output or cancellation without periodic polling.
Stopping the source cancels and joins its reader and terminates/reaps its owned
process group; it never signals a process discovered by name or an old PID record.
The managed Portal worker is also cancelled and joined on stop, including while
connecting or waiting without notifications. Connection and subscription setup
share a 2-second deadline (separate from initial backend discovery); readiness
means both signal subscriptions are installed, not merely that a thread started.
Normal idle watching has no such timeout. Portal owner loss/replacement, bus loss,
or malformed relevant signals stop the watcher with a content-free error; it does
not silently remain ready or reconnect. A later shell launch or re-enabling
auto-theme can start a fresh instance once the backend is available.
Private control files and logs live under `$XDG_CACHE_HOME/slate/watchers/<profile-id>`
(default `~/.cache/slate/watchers/...`). Startup failures report that log path.
Existing lock/control files alone do not mean a watcher is running; do not delete
the runtime directory to bypass an active instance. Startup acknowledgement means
the Rust event loop is ready, not that a theme has already applied successfully.

Upgrade note: re-enable auto-theme to refresh the launcher and Shell hooks. Legacy
watchers started by older binaries have no ownership record: they are not adopted
or stopped by process-name matching. Stop any known old instance explicitly before
using the new watcher; status reports only the new managed runtime.

`slate doctor auto-theme [--json]` compares the launcher/helper against this binary
and profile without executing them, reads the saved preference, and checks managed
lifetime-lock/control state. It distinguishes absent, starting, ready, stopping,
normally stopped, recorded failure, unconfirmed stale records, changing ownership,
and unreadable state. New instances save a private generation-matched exit receipt;
missing receipts do not prove a crash. Legacy artifacts do not prove legacy
processes are running. Refreshing a legacy/unrecognized launcher emits a warning
that untracked processes are not stopped.

The same report explains conditional dark/light choices using the runtime selection
policy and the same `resolution` JSON v1 fields as `slate config pairing`. A ready
watcher does not hide an unknown saved theme ID, invalid pairing document or unsafe
required current-theme file. Selection errors appear in `issues` even when automatic
switching is disabled; valid defaults, explicit cross-appearance overrides and catalog
self-pairs are not errors. Review `slate config pairing` before replacing or clearing
saved overrides. These separate file/runtime observations are not an atomic snapshot,
a desktop appearance probe or proof that either theme can be applied successfully.

The doctor reports issues, next steps, and the private log path, never log contents
or instance tokens. File reads are bounded and reject link/non-regular targets;
no desktop backend or process-list probe runs. Exit code zero means inspection
completed, not that the watcher is healthy: inspect `issues`, `runtime.state`,
`installation`, and `resolution`. Inspection does not acquire the configuration
writer and remains available while a write or pending preview recovery blocks edits.

Auto-theme diagnostic text escapes path control/directional characters. JSON retains
exact UTF-8 path strings; non-UTF-8 paths use a lossy display string with additive
`path_is_lossy` flags on launcher/helper and `runtime.directory_is_lossy` /
`runtime.log_path_is_lossy` flags. Unavailable runtime paths and their flags are null.
Lossy displays are not exact paths or proof that a watcher can use them. Both output
formats use the shared doctor writer: an early pipe-reader exit is normal (and does
not prove the reader received a complete report); other write errors still fail.

Restore previews compare the snapshot's files with their current contents and saved
Unix permissions without writing files. Blocked targets return a non-zero exit status.
Both text and JSON `restore <id> --dry-run` output tolerate an early pipe-reader exit,
but a blocked plan still fails even when the reader has closed; other output errors
also fail without a panic. Human previews escape names, paths and reasons using the
same display rules as history listing; JSON preserves its existing plan format and
exact recorded strings. This does not relax restore-record validation. Actual restore
still requires confirmation and stops before confirmation/file restoration if its
initial plan cannot be written, including a broken pipe (writer-lock metadata may
already exist). Read-only preview remains available during another writer or pending
preview recovery and never restores files or creates an undo point.
Ordinary explicit theme changes now save a `pre-theme` operation checkpoint before
writing the selected available adapters' files. It includes generated colors,
bat theme assets and shared Shell/theme-state outputs; picker commits also capture
the following opacity stage. Byte contents, ordinary permissions and prior absence
are saved, including newly created outputs. Unsafe/non-regular files and sources
over 8 MiB each or 64 MiB combined stop the change before theme writes. Unselected
tools and unrelated dotfiles are not included. Even a first theme change is not a
full pre-install baseline. Setup's broader baseline remains a separate record.
Quiet auto-follow and callers explicitly using an existing checkpoint/preview
journal do not create an additional `pre-theme` record.

Legacy-style named-theme restores still reapply the saved theme; the preview flags
this additional step, but does not list every regenerated theme file. Baseline,
pre-theme, pre-clean, pre-import, pre-opacity and undo checkpoints are file-only: they restore recorded
bytes, ordinary permissions and prior absence without regenerating a theme over
those files. This also applies when undoing an undo. Restore prints the new undo
checkpoint ID; list and preview JSON report `may_regenerate_theme_files: false`
for these file-only points. Finishing a file-only restore does not initialize an
otherwise absent Slate config directory.

An undo checkpoint covers the selected record's target files, not every possible
effect of a later named-theme regeneration, external cache, font installation or
running application. Check the plan before restoring, and reload tools as needed.
In particular, bat's compiled cache is not restored or rebuilt automatically.

The confirmation prompt is bound to the in-memory file plan it displays. Before
restoring, Slate rechecks the parsed manifest, backup and current bytes, ordinary
permissions, file identities and resolved target paths. Changes detected while
you are deciding require a new preview and confirmation, without restoring files.
A second check runs after saving the undo checkpoint; if it detects a change,
restoration does not start and the error identifies the retained checkpoint.
Canceling discards the prepared plan. A separate `--dry-run --json` report is not
an authorization token: a later restore command prepares and confirms a fresh plan.
These checks do not lock out external editors after the final check, nor cover
the additional theme regeneration described above.

Recovery lists exclude invalid records from the selectable points, but report
their paths, reasons and inspection steps without deleting them. The picker also
shows these notes. Undo checkpoints are hidden by default with an explicit count;
`--list --all` includes them. `--list --json` (optionally with `--all`) emits schema
version 1, with `points`, `issues`, `valid_count`, `hidden_undo_count` and
`ignored_entries`. Each point includes its timestamp in Unix seconds, entry count,
tool names, baseline/undo flags and theme-regeneration behavior, never saved file
contents. Points sort newest-first, with ID ordering for equal timestamps.

The inventory checks record metadata, not current target bytes:
`target_contents_checked` is false. Exit zero means the scan completed, not that
every record is usable; inspect `issues` and preview an ID before restoring.
An inaccessible history directory fails with no partial JSON report. Names with
non-UTF-8 bytes have display-only lossy paths and no invented restore ID. Legacy
per-tool backup folders are ignored; timestamp-shaped folders missing a manifest
are reported as potentially incomplete/damaged, including captures still running.
Listing works during an active Slate writer or pending preview without reading
preferences, initializing sound or creating files. Like a directory listing, it
is not a consistent transaction across concurrent changes.

Directly previewing an invalid ID reports its error. A manifest must be a regular UTF-8 file of at most
1 MiB with no more than 512 entries, a matching directory ID and a valid UTC
timestamp. Linked point directories/manifests/backup files are rejected; backup
sources must be directly inside that point. Targets must be absolute file paths,
without parent traversal, duplicate directory-alias destinations or overlap with
recovery storage. Backup data and current-file comparisons each have an 8 MiB/file
and 64 MiB total bound. Unsafe current targets appear as `blocked` in a valid JSON
preview; invalid records fail before emitting a plan. Execution prepares all bytes
first and captures an undo checkpoint for that same target set. Ordinary directory
aliases and original-profile paths remain supported. These are structural checks,
not authentication of an edited backup or a transaction against external editors:
restore only trusted records after reviewing their target paths.

`slate clean --dry-run [--json]` projects file removals, rewrites, unchanged files,
and blocked targets using the same transformations as cleanup. It also lists
directories to remove, the preserved `slate/user` tier (not recursively scanned),
and planned watcher-stop / best-effort terminal-reload behavior. It never acquires
a writer lock, writes a backup/config/cache file, or controls a process, including
when another writer or an interrupted preview is present. File contents are never
printed; text paths escape terminal controls and lossy JSON paths are flagged.

JSON schema version 1 includes `changes`, `directories_to_remove`, `summary`,
`scan_complete` and `issues`. An unsafe/inaccessible tree produces an incomplete
report; known read/parse failures appear as blocked targets. Both return exit 1.
Best-effort parsers that retain malformed files report `unchanged` with a warning
instead. Exit zero is not permission to execute: `snapshot_write_checked`,
`target_writes_checked` and `writer_and_recovery_checked` are false. Backup and
target write permissions, active writers and pending recovery may still stop a
later clean. The report is an observation, not a replayable authorization or an
atomic snapshot of concurrent edits. Directory metadata and external process
state are outside file-restore coverage; installed tools are not uninstalled.

Preview source reads share snapshot limits (8 MiB/file, 64 MiB total). Target
enumeration is capped at 512 file entries, with 64 nested levels / 4096 managed
directories. These discovery limits also apply to execution; they do not impose
a wall-clock timeout on an unresponsive filesystem. Explicit integration targets
inside the preserved user tier are rejected.

Kitty cleanup matches complete literal `include` directives under Slate's managed
Kitty directory and Unix `listen_on` paths with the exact basename `kitty-slate`.
Neighboring path/key/socket names are preserved. Continuation lines are handled
as one directive, as described in [Kitty's configuration reference](https://sw.kovidgoyal.net/kitty/conf.html).
Alacritty cleanup removes only owned import string tokens and their following
commas from root `import` or `general.import` (including dotted/inline forms).
All comments, retained values, quoting and line endings stay intact; empty arrays
and tables are retained. Files without matching imports are not rewritten.
Neither cleanup rule expands variables, resolves include-file aliases, follows
parent traversals, or evaluates generated/glob includes: inspect custom indirect
references manually. The same rules apply to the read-only clean preview.

Clean checks its target paths and saves a pre-clean snapshot before stopping the
watcher or removing files. This includes generated assets, all tmux config candidates,
both OpenCode TUI configs, and Neovim loaders/shims/state, but excludes the preserved
`slate/user` tier. Backup failure stops cleanup; later failures report the snapshot ID
instead of claiming success. Clean and restore under `SLATE_HOME` leave running
watchers alone; isolated or SSH cleanup does not reload graphical terminals.
Starship cleanup uses the same integration path as apply/backup, not `STARSHIP_CONFIG`.

Clean refuses symbolic-link targets/managed directories, isolated paths that escape
`SLATE_HOME`, and cache roots nested inside directories being removed. New snapshots
save file bytes and ordinary Unix permissions in private snapshot directories;
backup copies remain private even for executable originals. Valid older snapshots within the
read limits remain readable but
cannot recover permissions they never recorded. Empty directory metadata, ACLs,
extended attributes, and running application state are not restored. Cleanup is not
an atomic transaction against external editors: inspect `slate restore <id> --dry-run`
before restoring and use the same HOME/XDG profile throughout.

Managed marker blocks must have one ordered START/END pair on complete, matching
comment lines (`#`, Lua `-- #`, or Vim `" #`). Reversed, duplicate, incomplete,
inline or mixed-wrapper markers are rejected before editing that file, without
printing its contents. Inspect ambiguous markers manually; Slate does not guess
which user text to remove. Valid removal includes the full comment wrappers and
preserves surrounding bytes, including non-UTF-8 content and CRLF endings. Repeated
updates do not accumulate comment prefixes, and unchanged files are not rewritten.
A cleanup error may still follow earlier changes to other files; use its pre-clean
snapshot reference for recovery. Existing stray comments from older edits are not
automatically removed.

Manual theme application creates a file-only `pre-theme` checkpoint, including on
first application; this is not a full setup baseline. Setup stops if its pre-change
snapshot cannot be saved. If a pre-commit integration fails, Slate keeps the prior
global theme, shell environment and Neovim state and reports failure;
successful tool-file writes remain in place. Partial-apply
errors include a restore-preview command when a safety snapshot was created. The picker
also checks for partial failure before saving opacity or showing its success receipt.

If the picker's theme stage succeeds but saving opacity fails, Slate attempts
file-only recovery from that selection's `pre-theme` checkpoint while retaining
the write lock. It restores captured bytes, ordinary permissions and prior file
absence, including auto pairing, rather than regenerating a remembered theme or
opacity preset. No adapters, version probes or cache builders are rerun for recovery.
The command still fails and shows no selection-success receipt even when recovery
succeeds. Blocked, missing or partial recovery is reported with the original error
and available recovery IDs. A saved pre-recovery undo point may contain the partial
selection: preview it before using it. External caches and live windows are outside
file recovery; reload tools as needed. This is not a multi-file transaction.

The final shared-file phase is checked separately from tool adapters. Slate writes
the shared shell configuration before recording the new current theme; if shell
generation/publication or current-theme tracking fails, the apply report retains
the failed stage, adapter results, and available recovery ID. The auto-theme pair
and Neovim notification are not advanced on that failure. Earlier tool or shell
files may already have changed: this is not an all-or-nothing transaction. Fix the
reported path/settings before retrying, or inspect the saved restore point first.
Quiet mode still returns a nonzero exit status and the recovery hint. A restore
whose subsequent theme reapplication fails also reports failure and its undo ID.

Selected, available Neovim notifications run once **after** shared Shell files and
the current-theme record are saved. A pre-commit failure reports `ThemeNotCommitted`
instead of claiming Neovim was applied or missing. Neovim-only theme changes still
work. If its notification write fails after commit, the operation reports a failed
Neovim result and explicitly says the new theme was already saved; it does not roll
back the theme/auto pair or silently retry the notification. Inspect the checkpoint
before restoring, or fix the write issue and retry. When Neovim is not a ready
selected adapter, the existing shared editor-state hook remains best-effort and
its failures are retained as warnings. Direct adapter/registry calls remain
immediate APIs without a global commit. A state-file publication is not proof of
editor receipt/rendering, and does not guarantee exactly one filesystem event.

Automatic theme application (`theme --auto` and the background watcher's apply callback) consumes
the resolved pair without detecting appearance again or rewriting `auto.toml`.
Fallbacks do not create a pairing file. Legacy restore reapplication also preserves
the restored pair. Manual selections still remember the slot for the detected
system appearance when auto-theme is enabled. If that preference update fails,
Slate warns that the theme was already saved and the next automatic switch may
use the previous pairing; it does not roll back or suppress editor notification.
Retained pairing, editor and reload warnings go to stderr even in quiet mode and
picker commits, and in the background watcher's log. These warnings alone do not make an otherwise successful apply
exit nonzero; required writes and selected adapter failures still do.

Short desktop-appearance commands (`defaults` / GNOME `gsettings get`) have a
2-second post-spawn deadline and an 8 KiB combined stdout/stderr cap. Slate accepts
known values, not a substring such as “dark” inside an error; macOS's recognized
missing `AppleInterfaceStyle` diagnostic still means Light. Failed, invalid or
timed-out queries stop `theme --auto` before theme files are written. A manual
selection already saved before its pairing query fails stays saved, with a warning.
Linux Portal Settings version/read queries have a separate 2-second asynchronous
deadline covering connection, proxy creation and reply. A successful Portal answer
is used directly; an available GNOME fallback can add its own query deadline.
For older Portal implementations, a `ReadOne` unknown-method reply triggers one
legacy `Read` request within the same deadline, decoding its extra variant layer.
Access errors, timeouts and malformed replies do not trigger legacy retries. Query
errors identify the stage and a safe error category without exposing response data.
Settings and Screenshot Portal version detection uses the protocol's lowercase
`version` property, including older backends. Screenshot version probes also use
a 2-second asynchronous query budget.
Missing session buses/services or unsupported settings still allow the headless
Light fallback, but access/protocol failures and timeouts are not silently Light.
These are query limits, not a whole-command or OS/filesystem timing guarantee.
Long-running appearance monitors have no idle timeout (Portal subscription setup
is bounded separately); interactive screenshot requests are not given this short
timeout. Native output and appearance/screenshot D-Bus response details are omitted from errors.

When file contents need to change, `slate config set opacity <preset>` first saves
a `pre-opacity` checkpoint for the four generated Ghostty/Alacritty/Kitty
opacity/blur files and `current-opacity`.
It prints a restore-preview command, including in quiet mode. The saved preset is
updated only after all four outputs succeed; a later failure reports its stage and
available recovery point without claiming earlier writes were rolled back. Final
links, unsafe targets, escaping isolated paths, overlapping backup storage and
conflicting terminal-output aliases are rejected. Ordinary in-profile directory
aliases remain supported; these checks do not lock out external editors.
This checkpoint restores file bytes, permissions and prior absence, not live window
state or unrelated settings. Imports reuse their wider checkpoint, theme/picker
snapshots also cover these generated files, and previews keep using their journal
without creating opacity checkpoints. Reload is best-effort and only attempted
when the captured session permits local terminal control, never for SSH/isolation.

Repeated opacity settings compare all generated file bytes and the saved preset,
not just the preset name. Identical files keep their identity, modification time
and permissions; an entirely unchanged standalone apply creates no new checkpoint.
Missing or altered files are repaired individually. Reads remain bounded (8 MiB
per managed output, 4 KiB for the preset), and unsafe/unreadable sources fail rather
than being treated as no-ops. Files are rechecked for content, identity, permissions
and directory redirection before their write/skip decision. These are per-file
checks, not a multi-file transaction; later failures can leave earlier writes.
The same templates and skip rules apply inside terminal adapters. Requested local
reloads still run on a disk no-op: the saved files do not prove the live window state.

</details>

<details>
<summary><strong>How it works</strong></summary>

Slate composes managed config files alongside your existing setup rather than replacing your dotfiles.

```text
~/.config/slate/config.toml        # preferences (theme, font, toggles)
~/.config/slate/auto.toml          # dark/light theme pairing
~/.config/slate/managed/<tool>/*   # generated assets slate owns
~/.config/<tool>/...               # your configuration, plus integration settings
```

For Ghostty: `config-file = ...`. For Kitty/Alacritty: managed `include`/`import` entries. For zsh/bash/fish: a removable marker block in the shell rc. For Neovim: a `pcall(require, 'slate')` marker block in `init.lua` (`init.vim` works too) that falls back silently if slate is uninstalled. Slate-owned files stay slate-owned; yours stay yours.

</details>

## Shell completion

`slate completions <bash|zsh|fish>` prints a script; it does not install files or edit
your shell startup configuration. Save it once and load it with your shell:

| Shell | Save the generated output as | Activation |
| --- | --- | --- |
| Zsh | `_slate` in a directory on `$fpath` | Add that directory before your existing `compinit` call. |
| Bash | A file such as `slate.bash` | Source it from your interactive Bash startup file. |
| Fish | `slate.fish` in your Fish config's `completions` directory | Fish loads it automatically. |

For example, in Zsh:

```zsh
mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions"
slate completions zsh > "${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions/_slate"
```

Put `fpath=("${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions" $fpath)` before
the existing `compinit`/framework initialization in `${ZDOTDIR:-$HOME}/.zshrc`, then
open a new shell. If you do not already initialize completion, use
`autoload -Uz compinit` followed by `compinit` there; do not duplicate a framework's
initialization. These steps are optional and must be performed explicitly.

Candidates include public commands/options, canonical theme IDs and appearance
values. Scripts do not launch Slate on startup or Tab, and do not enumerate live
fonts or restore points. Generation does not read settings or need HOME, and works
while configuration is locked or recovery is pending. Regenerate the saved script
after upgrading Slate; avoid putting the generator itself in a startup file.

## Themes

20 variants across 9 families: Catppuccin · Solarized · Tokyo Night · Rosé Pine · Kanagawa · Everforest · Dracula · Nord · Gruvbox.

<details>
<summary><strong>All 20 variants — palette gallery</strong></summary>

Regenerate with `scripts/render-theme-gallery.sh`; future drift is caught by `tests/docs_invariants.rs`. Swatch order, left to right: background · foreground · brand accent · red.

<!-- THEME-GALLERY-START -->
<!-- generated by scripts/render-theme-gallery.sh — do NOT hand-edit; regenerate from themes/themes.toml -->

| Family | Variant | ID | Appearance | Palette |
|--------|---------|----|-----------:|---------|
| Catppuccin | Catppuccin Frappé | `catppuccin-frappe` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#303446"/><rect width="20" height="14" x="20" fill="#c6d0f5"/><rect width="20" height="14" x="40" fill="#babbf1"/><rect width="20" height="14" x="60" fill="#e78284"/></svg> |
| Catppuccin | Catppuccin Latte | `catppuccin-latte` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#eff1f5"/><rect width="20" height="14" x="20" fill="#4c4f69"/><rect width="20" height="14" x="40" fill="#7287fd"/><rect width="20" height="14" x="60" fill="#d20f39"/></svg> |
| Catppuccin | Catppuccin Macchiato | `catppuccin-macchiato` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#24273a"/><rect width="20" height="14" x="20" fill="#cad3f5"/><rect width="20" height="14" x="40" fill="#b7bdf8"/><rect width="20" height="14" x="60" fill="#ed8796"/></svg> |
| Catppuccin | Catppuccin Mocha | `catppuccin-mocha` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1e1e2e"/><rect width="20" height="14" x="20" fill="#cdd6f4"/><rect width="20" height="14" x="40" fill="#b4befe"/><rect width="20" height="14" x="60" fill="#f38ba8"/></svg> |
| Solarized | Solarized Dark | `solarized-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#002b36"/><rect width="20" height="14" x="20" fill="#839496"/><rect width="20" height="14" x="40" fill="#6c71c4"/><rect width="20" height="14" x="60" fill="#ea6e60"/></svg> |
| Solarized | Solarized Light | `solarized-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#fdf6e3"/><rect width="20" height="14" x="20" fill="#3e4d52"/><rect width="20" height="14" x="40" fill="#6c71c4"/><rect width="20" height="14" x="60" fill="#a00d0d"/></svg> |
| Tokyo Night | Tokyo Night Dark | `tokyo-night-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1a1b26"/><rect width="20" height="14" x="20" fill="#c0caf5"/><rect width="20" height="14" x="40" fill="#bb9af7"/><rect width="20" height="14" x="60" fill="#f7768e"/></svg> |
| Tokyo Night | Tokyo Night Light | `tokyo-night-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#e1e2e7"/><rect width="20" height="14" x="20" fill="#3760bf"/><rect width="20" height="14" x="40" fill="#5a4a78"/><rect width="20" height="14" x="60" fill="#9f1f63"/></svg> |
| Rosé Pine | Rose Pine Dawn | `rose-pine-dawn` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#faf4ed"/><rect width="20" height="14" x="20" fill="#575279"/><rect width="20" height="14" x="40" fill="#907aa9"/><rect width="20" height="14" x="60" fill="#a72464"/></svg> |
| Rosé Pine | Rose Pine Main | `rose-pine-main` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#191724"/><rect width="20" height="14" x="20" fill="#e0def4"/><rect width="20" height="14" x="40" fill="#c4a7e7"/><rect width="20" height="14" x="60" fill="#eb6f92"/></svg> |
| Rosé Pine | Rose Pine Moon | `rose-pine-moon` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#232136"/><rect width="20" height="14" x="20" fill="#e0def4"/><rect width="20" height="14" x="40" fill="#c4a7e7"/><rect width="20" height="14" x="60" fill="#eb6f92"/></svg> |
| Kanagawa | Kanagawa Dragon | `kanagawa-dragon` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#181616"/><rect width="20" height="14" x="20" fill="#c5d0ff"/><rect width="20" height="14" x="40" fill="#8ba4b0"/><rect width="20" height="14" x="60" fill="#ff6666"/></svg> |
| Kanagawa | Kanagawa Lotus | `kanagawa-lotus` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#f2ecbc"/><rect width="20" height="14" x="20" fill="#545464"/><rect width="20" height="14" x="40" fill="#4d699b"/><rect width="20" height="14" x="60" fill="#8e1b32"/></svg> |
| Kanagawa | Kanagawa Wave | `kanagawa-wave` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1f1f28"/><rect width="20" height="14" x="20" fill="#c8d1d8"/><rect width="20" height="14" x="40" fill="#938aa9"/><rect width="20" height="14" x="60" fill="#ff6666"/></svg> |
| Everforest | Everforest Dark | `everforest-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1e2326"/><rect width="20" height="14" x="20" fill="#d3c6aa"/><rect width="20" height="14" x="40" fill="#a7c080"/><rect width="20" height="14" x="60" fill="#e67e80"/></svg> |
| Everforest | Everforest Light | `everforest-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#efebd4"/><rect width="20" height="14" x="20" fill="#5c6a72"/><rect width="20" height="14" x="40" fill="#8da101"/><rect width="20" height="14" x="60" fill="#9d1f1a"/></svg> |
| Dracula | Dracula | `dracula` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#282a36"/><rect width="20" height="14" x="20" fill="#f8f8f2"/><rect width="20" height="14" x="40" fill="#bd93f9"/><rect width="20" height="14" x="60" fill="#ff5555"/></svg> |
| Nord | Nord | `nord` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#2e3440"/><rect width="20" height="14" x="20" fill="#d8dee9"/><rect width="20" height="14" x="40" fill="#88c0d0"/><rect width="20" height="14" x="60" fill="#ff7777"/></svg> |
| Gruvbox | Gruvbox Dark | `gruvbox-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#282828"/><rect width="20" height="14" x="20" fill="#ebdbb2"/><rect width="20" height="14" x="40" fill="#fe8019"/><rect width="20" height="14" x="60" fill="#ff5555"/></svg> |
| Gruvbox | Gruvbox Light | `gruvbox-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#fbf1c7"/><rect width="20" height="14" x="20" fill="#3c3836"/><rect width="20" height="14" x="40" fill="#af3a03"/><rect width="20" height="14" x="60" fill="#9d0006"/></svg> |

<!-- THEME-GALLERY-END -->

</details>

## Development

Built with AI assistance, with every change reviewed and tested by a human before it lands.

## License

MIT.

## Credits

Built on top of great work from others:
[Ghostty](https://ghostty.org/) · [Kitty](https://sw.kovidgoyal.net/kitty/) · [Alacritty](https://github.com/alacritty/alacritty) · [Neovim](https://neovim.io/) · [Starship](https://github.com/starship/starship) · [bat](https://github.com/sharkdp/bat) · [delta](https://github.com/dandavison/delta) · [eza](https://github.com/eza-community/eza) · [lazygit](https://github.com/jesseduffield/lazygit) · [fastfetch](https://github.com/fastfetch-cli/fastfetch) · [tmux](https://github.com/tmux/tmux) · [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) · [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts).
