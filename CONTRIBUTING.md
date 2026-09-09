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
bash scripts/build-dev.sh
```

This defaults to a locked, offline build using the disk-conscious `local-check`
profile. On a first checkout with missing dependencies, use
`bash scripts/build-dev.sh --online` to allow Cargo to download them without
updating the lockfile. The script works from any working directory and does not
install the binary, clean caches, or restart running tools. It preserves Cargo's
target directory, target architecture and toolchain environment. Use ordinary
`cargo build` only when you need the default development debugging symbols.

On macOS, `build.rs` compiles a small Swift helper (`dark-mode-notify`) via `swiftc`.
Its architecture follows Cargo's `TARGET`, not the build host. The helper targets
macOS 11.0 on Apple Silicon and 10.15 on Intel by default, using system Swift
runtime/overlay libraries. These are helper deployment floors, not an assertion
that the entire CLI has been runtime-tested on those OS versions; the Rust
executable's own deployment defaults are unchanged.

`MACOSX_DEPLOYMENT_TARGET` can select a newer deployment version for both compilers;
values below the helper's floor are rejected explicitly. `SDKROOT` is forwarded
to Swift as a single SDK path. Release/dist builds fail if Swift cannot compile
the helper or produces a missing, empty, or wrong-architecture artifact. Debug
builds retain a visible warning and an empty-helper fallback when compilation
fails; auto-theme is unavailable in that build. Linux skips Swift entirely.

The build watches the Swift source and toolchain-selection environment variables.
If developer tools are repaired in place without changing those values, force a
package rebuild with `cargo clean -p slate-cli`, then build again.

Focused build-policy checks use disposable compiler fixtures (no Xcode needed):

```bash
cargo test --locked --test watcher_build
```

These cover target/SDK arguments, failure/stale-output handling, deployment
validation, debug fallback, and the Linux skip path. They check the real build
script, not whether an old macOS system can load the resulting release helper.

## Testing

### Trying a local build in your terminal

Build and test the candidate first, then install that exact trusted executable:

```sh
bash scripts/build-dev.sh
python3 tests/support/menu_wrap_pty.py "$PWD/target/local-check/slate"
bash scripts/install-dev.sh "$PWD/target/local-check/slate"
command -v slate
slate --version
```

Check the developer build/install entry points without updating your real CLI:

```sh
bash tests/build-dev-smoke.sh
cargo test --locked --offline --profile local-check --test dev_install -- --test-threads=2
```

The shell smoke check uses a fake Cargo function and verifies that the documented
scripts are not Git-ignored. The native installer tests use trusted temporary
executables and private bin/backup directories: replacement preserves old bytes
and permissions, repeating the same install adds no backup, and failed version
checks, symlinks, a busy installer or an unusable backup directory leave existing
files unchanged. They do not test power-loss atomicity or execute an actual
Slate binary, download software, or restart a watcher.

Injected copy/rename failures also check installer feedback: an unverified
backup prevents replacement and is never advertised as recoverable; an
unconfirmed replacement points to the target and verified previous binary.
The rename-after-write fixture deliberately leaves the new binary installed
while returning failure, so the script must not claim the target stayed
unchanged or silently roll it back. No failed case prints `Installed:`.

`local-check` is a shared, disk-conscious profile for repeated local builds and
focused tests. It keeps development assertions and overflow checks, but omits
debugger symbols and disables incremental caches. A later edit may therefore
compile more slowly than an incremental dev build. Use normal `dev`/`test`
profiles when source-level debugger information is needed; release/dist settings
are unchanged. Reuse this one profile instead of inventing a new profile for
each iteration. For example:

```sh
cargo test --locked --offline --profile local-check --lib saved_font_selection
cargo test --locked --offline --profile local-check --test picker_exit font_
```

The first build needs space for its own dependencies; this does not delete old
profiles. Run `bash scripts/dev-space.sh` for a read-only size report of workspace
total size, Git history, build caches, the default installed CLI and binary recovery backups.
The workspace total includes its cache and Git rows: these are a breakdown, not
additional disk usage. Git history is not a disposable build cache. The report also
reports `CARGO_TARGET_DIR` when set (relative paths are resolved from the workspace,
as in the build wrapper). Locations may overlap; do not add them together. It does
not resolve Cargo configuration files or custom installer paths, follow a listed
symlink, compile, launch Slate, or delete anything.

Before any cleanup, inspect disk usage and resolve the exact target
directory. A full `cargo clean --target-dir /absolute/project/target` removes
all build profiles in that directory, not just the currently selected one. Never
delete source, personal configuration or binary recovery backups to make a test
pass. Avoid cleaning artifacts used by a running watcher or test process.

If Cargo uses a custom target directory or cross-compilation target, pass the
actual executable path instead. The installer defaults to `$HOME/.local/bin`;
optional second and third arguments override the absolute bin and backup
directories. It checks the staged binary's `--version`, preserves the previous
binary, replaces it on the same filesystem and compares bytes. It refuses target
symlinks and overlapping invocations of this script; it does not coordinate with
other installers. Matching binaries are a no-op. No downloads, shell edits or
watcher restarts occur, and already-running processes keep their existing image.
The supplied candidate is executed, so use only an artifact you trust.

```sh
python3 tests/support/dev_install_test.py
```

For prompt-menu navigation and confirmation without rebuilding the CLI, compile
only the library test harness:

```sh
cargo test --locked --lib cli::prompt::menu::tests --no-run
python3 tests/support/prompt_menu_pty.py /absolute/path/to/the/reported/test-executable
```

Use the `Executable unittests src/lib.rs (...)` path printed by Cargo, not the
`slate` CLI executable. The Python standard-library runner requires Unix PTYs;
it invokes only an explicit ignored fixture with a private HOME and empty PATH.
Optional scenario names are `browse`, `broken`, `decline`, `stale`, `apply`,
`check`, `check-broken`, and `escape`. The check cases exercise the read-only
Starship doctor handoff and return with valid or malformed preferences. `escape`
declines confirmation and returns to the style preview. CLI mode additionally
runs `interrupt` to verify Ctrl-C exits with status 130 without applying changes.
CLI mode also runs `noop`: it seeds a private profile through an explicit first
save, then confirms the same layout in the TUI. Contents, modes, file modification
times and the recovery-file inventory must remain unchanged on the second save.
The runner sets NO_COLOR for stable selection-row assertions.
Coverage includes saved-preference navigation, recoverable configuration errors,
default-No confirmation, and rejecting a plan after an external preference edit.
Read-only cases compare the full temporary tree before/after, including permissions.
The `apply` case confirms changing Classic to Focus and checks the saved preference,
both generated layouts, private file modes, unchanged theme, and absent shell startup
files. These checks do not validate live Starship rendering or the installed CLI.
A custom Cargo profile may be used to preserve existing binaries.

To check a separately built CLI rather than the library fixture, pass `--cli`:

```sh
python3 tests/support/prompt_menu_pty.py --cli /absolute/path/to/slate apply check-broken stale
```

This invokes the real `prompt` command with both HOME and SLATE_HOME isolated,
an empty PATH, and a private PTY. It does not install the executable or switch a
running watcher. Process exit is bounded in both runner modes.

For prompt display isolation, run `cargo test --locked --lib prompt_output_`, the
`picker_starship_fork_fixture` output-filter/normal-output cases, and the `picker_exit`
test `real_picker_full_preview_filters_terminal_controls_and_contains_styles`. Follow
with prompt override/render and timeout-fallback compatibility checks. The conservative
display filter emits only text, LF and complete numeric SGR: parameter text is capped at
128 bytes, individual nonempty fields at five digits with values at most 255. Tests
cover truecolor/indexed/colon styles, Unicode, C0/C1 controls, OSC/DCS/SOS/PM/APC payloads,
malformed and truncated sequences, exact bounds and mixed-input/idempotence invariants.
The composer filters every override and brackets it with resets, not just fork results.
Private executables/PTYs prove hidden control payloads, retained colors and successful
cancel; they do not touch the host clipboard or prove safety for every terminal emulator.
The sequence reference is [XTerm Control Sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).

For bounded full-preview prompts, run `cargo test --locked --test picker_starship_fork_fixture`,
`cargo test --locked --lib preview_starship_` and the focused `picker_exit` test
`real_picker_full_preview_timeout_falls_back_and_cancel_stays_usable`. Follow with prompt
cache and full-mode input compatibility checks. Private executables exercise successful
ANSI/zsh-wrapper output, argv/env/stdin contracts, hidden stderr, nonzero exits, exact
combined output limits, hanging children and inherited pipes. Config fixtures cover
resolved traversal/link escapes, regular-file/UTF-8/size checks, linked source compatibility,
bounded generated output and retention of the previous preview on failure. Failure-cache
tests cover suppression and resize retry; a real private PTY proves timeout fallback,
no repeated fork on Tab revisits, hidden partial/error output and usable cancellation.
The shared process-group capture starts its deadline after spawn; this is not a sandbox
or a hard deadline on filesystem IO/spawn/OS teardown, and target checks do not lock out
external editors. Fixtures use no real Starship binary, host config, fonts or watcher.

For picker input ordering, run `cargo test --locked --lib picker_input_batch_` and
`cargo test --locked --test picker_exit real_picker_queued_`, then focused Tab,
cancel/commit and killed-picker recovery checks. The production batch coordinator is
driven with ordered events, a bounded/infinite-ready source and a preview-call counter:
do not discard navigation, save, or exit actions when coalescing expensive preview IO.
The first exit stops further reads/actions, release events are inert, repeats remain
accepted, and at most 32 events are processed before yielding back to rendering. That
is an event-count bound, not a wall-clock guarantee. Save feedback requests a frame;
unchanged selections and UI-only actions do not publish another terminal preview.
Private PTYs verify the selected persisted row, first-exit behavior, visible auto-save
feedback, unchanged preview file/journal stamps after save, and retained explicit saves
after cancellation. No host terminal settings, fonts or watchers are used by these fixtures.
Tests dispatching picker brand events share the existing sink-test guard with the
navigation/Enter event-count tests, rather than racing their process-global counter.

For preview write attribution, run `cargo test --locked --lib preview_receipts_` and
`cargo test --locked --lib preview_write_state_`, then focused original-bytes/modes/links,
commit-lock, real-picker cancellation/commit and interrupted-recovery checks. Private
fixtures inject external edits to unwritten files, before a later write, and after a
recorded write; cleanup must preserve those edits and the journal must contain only
Slate's recorded bytes. Ignored adapter errors, redirected writer aliases, linked legacy
writes, per-file/aggregate output reservations, explicitly attached workers and expired
contexts have focused coverage. The real parallel adapter path must also work. Scope and
attachment guards are thread-bound; only their shared context is passed to joined workers.
Receipts are recorded after successful atomic publication, never inferred from readback.
Outside preview, legacy in-place writes keep their previous behavior. These tests do not
prove atomic exclusion of uncooperative editors, every crash window, or multi-file rollback.

For incomplete preview write state, run `cargo test --locked --lib preview_write_state_`,
then focused stable-panic-hook, original-bytes/modes/links, commit-lock-transfer and
real picker/recovery checks. The coordinator is driven with injected private adapter
writes, an oversized after-read target, and a separate thread's file edit. It must
preserve unrecorded edits, restore still-matching files, retain the journal on partial
cleanup, and block another preview until incomplete state is handled. Held/misshapen
expected state must not become permission to overwrite; overlapping operations must
fail promptly. A fully captured adapter error must still permit normal cleanup.
The panic-hook test uses the real hook under `catch_unwind` and checks file/record
outcomes during an in-flight operation and after unwind. This is not a release-abort
process or power-loss test. The unit-valued operation gate may be reacquired after
unwind; a poisoned expected-state mutex is never trusted. These are cooperative
exclusion and last-known-state checks, not an atomic lock against external editors.

For bounded preview inputs, run `cargo test --locked --lib preview_read_safety_`,
`cargo test --locked --lib shared_reader_`, `cargo test --locked --lib bounded_record_json_`,
and `cargo test --locked --test recover_read_safety`. Follow with focused recovery
cleanup, confirmation and killed-picker/original-mode checks. Tests cover real sparse
files exceeding per-file/aggregate limits, binary/full-mode compatibility, resolved
dotfile links, missing versus dangling parents, and budget accounting. A small encoder
limit exercises the same save path's exact-boundary and old-record retention behavior
without allocating a giant journal. CLI children use private profiles and deadlines
for FIFO/large-target checks, verifying blocked restore plus available export/discard.
The shared reader checks file identity before/open/after reads; these tests do not
simulate every possible external-editor race. A failed after-write journal save keeps
the previous record, not an automatically rolled-back profile. No host settings or
watchers are changed, and native results must not be described as Linux runtime tests.

For recovery cleanup boundaries, run `cargo test --locked --test recover_cleanup`,
then focused confirmation/output, original-bytes/modes/links, commit-lock-transfer,
and explicit-mode atomic-write checks. The new CLI tests use real private-directory
permissions to fail record removal (0500), directory opening for sync after removal
(0300), and the second file of a multi-file restoration. They check actual file
bytes/modes, retained or removed records, idempotent retries, and later-edit protection.
They explicitly skip under root, which bypasses these permission failures. This is
not power-loss injection or a claim of atomic multi-file recovery. The existing
explicit-mode writer sets permissions on its private temporary file before commit;
the mode checks inspect outcomes, not every transient observation by other readers.
No host configuration, watchers, fonts or packages are changed by these fixtures.

For recovery confirmation binding, run `cargo test --locked --test recover_confirmation`
and `cargo test --locked --lib prepared_recovery_`, plus the recovery-output checks
below and focused journal/picker recovery cases. Owned PTY children exercise record
replacement (including identical bytes in a new inode and same-session edits),
default cancellation, successful restoration/discard, target conflicts, and writer
exclusion while the prompt is open. Read-only inspection must remain available.
Nonregular/private-path cases use bounded child timeouts, not blocking FIFO reads.
Library checks cover stale records/locks before export as well as restore/discard,
metadata-only oversized discard, and record retention after completed restoration.
The last case splits restoration and cleanup directly; it does not simulate a
power-loss/crash-atomic transaction. These checks use private profiles, never host
configuration or watchers. A held cooperative lock does not prevent external
editors: revalidation detects common changes, not every possible check/write race.

For interrupted-preview recovery output, run `cargo test --locked --test recover_output`
and `cargo test --locked --lib recover_output_`, then the focused `restore_preview_output_`,
fresh-home recovery, recovery-status and closed-stdout history checks below.
`cli::file_output` now supplies the shared restore/recover text escaping and separates
read-only output (BrokenPipe tolerated) from required pre-action output (all errors
propagated). A dry-run must still return active/conflict/unreadable failures after a
consumer closes. Recovery, export and discard must not begin after plan-output failure,
including explicit discard of a corrupt record. Completed-action receipt errors must
retain the completed outcome, never claim rollback or that nothing happened.
Public recovery option combinations must be validated before profile IO as well as
by clap. Private record/held-lock fixtures cover absent, active, interrupted, conflicting
and corrupt states; real stdout descriptors cover errors, and private successful
recover/export/discard cases verify bytes/record retention. Completion-error wording is
unit-tested, not a claim of a simulated post-commit filesystem failure. These tests
neither start a host picker/watcher nor prove an atomic inspection/confirmation boundary.
JSON schema, journal validation and explicit-confirmation rules remain unchanged.

For restore-preview output, run the `restore_preview_output_` integration checks,
the `restore_preview_text_` library check, existing
`restore_preview_reports_byte_changes_without_mutation`, and
`confirmation_defaults_to_cancel_and_unchanged_inputs_restore_and_undo` from
`restore_confirmation`. Preview renders the captured plan without IO, uses history's
shared escaping/writer, and checks semantic blockers even after BrokenPipe. Other
write errors must propagate. Actual restore's pre-confirmation plan deliberately
does not swallow BrokenPipe: an unwritable plan must stop before confirmation and
restoring snapshot files; existing startup/lock metadata is outside that guarantee.
Keep JSON plan fields, backup validation, restore execution and revalidation intact.
The CLI path fixture uses permitted directional format characters; newline/ESC paths
are already rejected by manifests. Synthetic text-only tests cover all display fields
without claiming those synthetic records can be persisted. Private tests verify exact
file trees under writer/pending blockers and real descriptors, including unwritable
non-pipe stdout. Complement these with the existing focused pipe checks in
`completions`, `font_preview`, `font_list`, `restore_inventory` and `clean_preview`;
they now use `support/redirected_output.rs`, not assert_cmd's replacement capture pipe.

For automatic-theme diagnostic output, run `cargo test --locked --test auto_theme_doctor`,
`cargo test --locked --lib auto_theme_output_runtime_serialization`, and
`cargo test --locked --test config_inspect --test ghostty_doctor --test opencode_doctor closed_stdout`.
Render an already captured report, reuse doctor's terminal-path escaping and output
writer, and keep path lossiness explicit without changing native watcher path types.
Missing runtime paths have null lossiness flags; a lossy display is not a usable
round-trip path. Synthetic serializer paths cover non-UTF-8 even when the host
filesystem cannot create such filenames; they do not prove watcher/runtime support.
For descriptor-level tests, use `support/redirected_output.rs` with a std Command:
assert_cmd's Command::assert installs its own stdout capture pipe and invalidates
custom redirections. The helper preserves stdout, bounds/reaps its owned process,
and captures stderr in an anonymous temporary file, not the profile. Closed socket
consumers exercise BrokenPipe; unconnected datagram stdout exercises other write
errors (read-only descriptors are not reliable because Rust stdio may ignore EBADF).
Keep diagnostic reports successful when they contain issues, treat BrokenPipe as a
normal early reader exit, and still propagate other output errors without panicking.

For shared preference diagnostics, run
`cargo test --locked --test auto_theme_doctor --test config_inspect`, with the focused
`config_flag_distinguishes_absence_from_invalid_types_without_writing` and
`shared_reader_distinguishes_missing_paths_links_and_special_files` library tests and
`ordinary_reads_keep_linked_dotfiles_but_reject_broken_links_and_oversized_snapshots`
from `config_read_safety`. Doctor must use the same inspected auto-theme getter as
config queries, never infer false from ENOENT alone, and never create a configuration
manager's directories. The shared flag reader preserves one parser/default policy;
inspection validates profile boundaries and opens with Links::Reject, while ordinary
runtime reads retain Links::Follow. Sound inspection uses the same strict path too.
Private CLI cases compare JSON and human text for dangling parents, final links,
isolated escapes (including missing external targets), valid parent aliases, absent
defaults, dotted/inline tables, invalid types/UTF-8 and the exact document-size limit.
No source contents, configuration writes or watcher effects are permitted. Metadata
validation and bounded reads detect common changes; they are not external-editor locks.

For automatic-choice doctor integration, run
`cargo test --locked --test auto_theme_doctor --test auto_resolution`.
`cli::auto_theme_resolution` is the shared report/text layer for doctor and pairing;
keep selection policy in `config::auto_resolution`. Doctor adds the same JSON v1
`resolution` object without merging configuration and runtime evidence. Surface
selection errors in `issues` even for a disabled or ready watcher, but do not label
valid defaults, explicit overrides or self-pairs as failures. Unsafe current tracking
blocks only choices that need it; malformed pairing documents still block both.
Private fixtures compare both command reports, check text, bounded reads, FIFO/link
handling and unchanged file trees under writer/pending blockers. A held lifetime-lock
fixture simulates ready state with an invalid selection; it is not a real watcher or
evidence of native Linux behavior when run only on macOS. Neither diagnostic acquires
the configuration writer, queries the desktop, installs helpers or reads watcher
logs. Existing diagnostic exit-zero
and source-content omission contracts remain unchanged.

For pairing clears, run `cargo test --locked --lib pairing_`,
`cargo test --locked --test config_pairing pairing_clear_`, and the focused pairing
save/completion checks below. `SlotEdit` distinguishes keep/set/clear explicitly;
the old capture wrapper still means keep for an unspecified selection. CLI conflicts
must be rejected before profile setup and repeated in the public options validator.
Clear removes selected root keys only: never unlink the preference document or
change watcher enablement, shell files or current-theme state. Preserve original
absence for a no-op clear and identity for unchanged existing documents. A real
clear uses the same exact pre-config checkpoint, late-change checks, mode-preserving
publication and recovery path as a set. Retain removed assignments' prefix/inline
comment decoration in the document footer, not their string contents; reparse the
emitted TOML before checkpoint creation. Tests include quoted keys, multiline string
values containing '#', LF/CRLF comments, unrelated tables, mixed set/clear edits,
unknown ID removal, empty/absent documents, byte-exact recovery and actual conditional
fallback after clear. Non-string pairing fields and invalid syntax remain blocked;
this is not a generic damaged-document repair operation. Writer-lock setup can still
create its normal metadata on a no-op save, while dry-run remains write-free.

For shared automatic choice resolution, run `cargo test --locked --lib auto_resolution_`,
`cargo test --locked --test auto_resolution`, `cargo test --locked --test config_pairing`
and `cargo test --locked --test auto_theme_apply auto_theme_apply_preserves_pair_bytes_and_does_not_redetect_after_commit`.
`config::auto_resolution::choose` is the common policy for runtime and conditional
inspection: configured ID, matching current theme, catalog pair, then brand default.
Unknown selected IDs fail without echoing source strings. Preserve known manual
cross-appearance overrides and catalog self-pairs; report actual theme appearance.
Required inputs use the pairing inspection path contract and bounded regular reads;
current tracking is lazy and captured at most once for both inspection choices.
This is not a desktop query, atomic configuration snapshot or apply-readiness check.
Only inspect receipts gain the additive JSON v1 resolution field; unset saved slots
stay unset and remain distinct from a conditional fallback. Invalid pairing syntax
blocks both choices, while a valid explicit slot is independent of unsafe current
tracking. Inspect errors keep the diagnostic exit-zero convention. Tests cover all
current catalog entries, private command tripwires, locked/pending profiles, FIFO
reads, source-free errors and real automatic CLI rejection before theme-file writes.
Native successful application uses existing private adapter stubs, not real tools.

For pairing inspection/preview/save and interactive cancellation, run
`cargo test --locked --lib pairing_`, `cargo test --locked --test config_pairing`,
`cargo test --locked --bin slate completion_schema_tracks_public_arguments_without_restricting_real_input`
and `cargo test --locked --test completions bash_completion_offers_contextual_candidates_without_running_slate`.
The CLI validates exact matching-appearance IDs before HOME resolution; its handler
repeats validation, bypasses native/sound setup and owns the file writer only on
save. Read-only inspection/preview works under writer/pending-recovery blockers.
`PreparedPairing` captures bounded regular auto.toml bytes, comments, modes and
resolved-parent identity; write only that file after a pre-config checkpoint and
revalidation. No-op edits preserve quote spelling and inode identity. Existing
other-slot string values remain unchanged even when absent from the current catalog;
inspection reports unknown IDs without leaking stored content. Invalid field types
and malformed documents fail before writes. Do not discard unrelated document data
or turn a partial update into a watcher toggle, shell refresh or theme application.
Both the hub and configure menu use the file-only save; removing their old follow-up
refresh/restart is intentional. Native appearance events reread pairing; immediate
application is explicit via `slate theme --auto`. The PTY test confirms and declines
the real CLI prompt with a deliberately unsafe generated-shell target and untouched
watcher files, in private profiles with native-command tripwires. It is not a live
watcher event test. The core cases also exercise checkpoint recovery and late edits.

For staged shell preference toggles, use `cargo test --locked --lib shell_preference_`,
`cargo test --locked --lib shell_settings_`, and `cargo test --locked --test config_toggle`.
Fastfetch and auto-theme enable/disable render with in-memory overrides, capture
bounded regular inputs/outputs (4 KiB state, 256 KiB TOML, 8 MiB generated files),
create an exact `pre-config` checkpoint, then publish generated files before intent.
Preserve comments, custom fields, ordinary permissions and identical-file identity;
refuse final links, special files, unsafe parents and detected late edits. Operation
checkpoints must remain file-only in `RestorePoint::reapplies_theme`.
No-op Fastfetch skips checkpoint creation; auto-theme checkpoints include the helper
and launcher because native preparation/removal can still change them. Its native
callbacks are separate stages, not part of the captured generated-file publication.
Do not reset the boolean after a late lifecycle failure or overwrite external edits
with an automatic rollback. Report partial progress and the retained restore ID;
file recovery cannot restore running processes. This is not global/crash atomicity
or external-editor exclusion. Pairing/configure and other setters are outside this
workflow. Stage tests inject helper/lifecycle failures; CLI tests use isolated
profiles, native-command tripwires, FIFO timeouts and byte/mode/absence recovery,
not real native watcher starts or stops.

For preference inspection and parameter preflight, run
`cargo test --locked --test config_inspect`,
`cargo test --locked --bin slate completion_`, and
`cargo test --locked --test completions bash_completion_offers_contextual_candidates`.
The config catalog supplies get/list metadata, set validation and completion keys.
Get/list must return before writer/sound/native initialization and use path-only
ConfigManager construction. Invalid set/get arguments must be rejected before HOME
resolution or writer acquisition, including the public set handler. Only requested
preferences are read; list retains valid values when another preference fails.
JSON v1 exposes nullable values and ok/unset/error states with path-lossiness flags;
inspection issues are reported with a successful command exit, like doctor. An
early stdout consumer exit is normal. Do not echo TOML or state-file contents in
errors or assume an invalid flag is false. The shared Fastfetch getter now bounds
regular marker files to 4 KiB and rejects final links/special files instead of using
exists(); ordinary bounded payloads retain presence semantics. Tests exercise real
CLI defaults, locks/pending recovery, private command tripwires, partial failures,
unsafe paths/files, argument escaping, bytes/mode preservation and BrokenPipe.

For generated shell startup behavior, run `cargo test --locked --lib shell_startup_`
and `cargo test --locked --lib fish_paths_`. Add `--features has-fish` for native
Fish coverage in its existing Linux CI job; without Fish only compile that test
with `--no-run`. `src/config/shell_integration/startup_tests.rs` renders private
profiles with optional features enabled, sources twice in fresh Bash/Zsh/Fish
processes, and checks interactive versus non-interactive behavior. Every optional
command is a private stub; highlighting scripts only set test variables. The
script waits for its stub jobs before inspecting invocation records. Non-interactive
loads must produce no output, start no optional tools, preserve the caller's prompt
and return control while keeping exports, PATH and manual wrappers usable.
Interactive mode must still initialize enabled features or the disabled-Starship
minimal prompt. Re-sourcing interactively intentionally remains a refresh, not
once-per-session initialization. Bash/Zsh use the native `i` flag and Fish uses
`status is-interactive`; do not substitute TERM_PROGRAM, PS1 or terminal presence.
Fish cache creation uses a no-Slate control; native checks are not actual installed
Starship/plugin/watcher, SSH protocol or real-user-startup verification.

For Bash startup selection, run `cargo test --locked --lib bash_startup_`,
`cargo test --locked --test doctor_shell --test clean_preview bash_startup_`,
`cargo test --locked --test baseline_backup test_baseline_has_correct_metadata`
and `cargo test --locked --lib shell_loader_`.
`src/env/shell_startup.rs` centralizes macOS login-file precedence and the unchanged
Linux `.bashrc` convention. Only definite absence permits falling through: links,
special files and metadata errors remain selected for validation. macOS no longer
chooses `.bashrc` merely because it exists. Do not create a higher-priority login
entry over an existing profile or auto-chain user files. All supported Bash paths
must remain in standard snapshots, clean preflight, preview and marker removal.
New snapshot keys are `bash-login` and `shell-profile`; older snapshots are not
retroactively extended. The setup capture verifies selection and managed source
path as well as captured file state before installers and publication.
Tests cover all candidate-presence combinations on both selection policies,
unsafe candidates, public executor rejection, JSON diagnostics, backup bytes,
clean/restore symmetry and absent-file restoration. macOS also sources the actual
generated loader with a private payload in Bash and Zsh to check the shared-profile
guard; it does not run a system login shell, real setup installers or user scripts.

For shell diagnostics, run `cargo test --locked --lib doctor_shell_`,
`cargo test --locked --lib doctor_integrations`,
`cargo test --locked --test doctor_shell --test doctor_readonly`, and
`cargo test --locked --test completions bash_completion_offers_contextual_candidates`.
`src/cli/doctor/integrations/shell.rs` shares setup's startup paths, bounded byte
reads, recovery-path validation and marker validation, but never launches a shell
or repairs files. Tests cover literal spellings versus printing/assignment/comments,
bad markers, opaque bytes, unavailable path comparisons, unsafe files/links, parent
escapes, held writer locks, unchanged bytes/modes, JSON/text parity and PATH tripwires.
The literal-line check is deliberately not a shell parser: functions, heredocs,
multiline quoting, control flow and indirect references remain outside its scope.
It must never report actual execution or a working interactive terminal. APFS-invalid
path bytes are checked through the reporting helper in memory. CLI fixtures use
private profiles and deadlines; no native Bash/Zsh/Fish is needed by these diagnostics.

For font-file publication, run `cargo test --locked --lib font_install_files_`.
These private-directory fixtures cover complete-family installation, identical
retry/metadata preservation, existing-file/link/FIFO/directory conflicts, linked
or malformed sources, basename collisions, depth/count/byte limits, injected
mid-batch failures, late arrivals, source edits and directory replacement. They
exercise the shared copy helper used by Caskroom recovery and release downloads;
no real font, package manager, download, unzip or font-cache refresh is invoked.

For custom Linux user-font paths, run `cargo test --locked --lib font_paths_`.
These fixtures force the Fontconfig backend even on macOS and cover captured
defaults/relative paths/OS bytes, SLATE_HOME isolation, configuration snapshots,
external and missing data roots, explicit root aliases, linked managed suffixes,
alias retargeting during publication, private new directories, preserved existing
modes, identical retries, unchanged old fonts, and cache-command arguments.
Only private synthetic headers and a fixture command are used, not native font
registration. Linux cross-compilation is not a Linux runtime test.

The user-data rule follows the
[XDG Base Directory specification](https://specifications.freedesktop.org/basedir/latest/)
and Fontconfig's
[`dir prefix="xdg"` resolution](https://fontconfig.pages.freedesktop.org/fontconfig/fontconfig-user.html).
`SlateEnv` captures XDG_DATA_HOME once; only nonempty absolute overrides are used.
The shared installation resolver canonicalizes existing ancestors of an explicit
data root and appends missing normal components without creating directories.
Explicit root aliases are accepted; parent traversal, broken roots and linked
managed suffixes stop writes. Default HOME suffixes retain their stricter no-link
boundary. Publication creates missing directories at 0700 (subject to umask),
leaving existing modes unchanged, and rechecks target path/identity before each
file. Retargeting may retain an already-published file for manual review rather
than deleting through an uncertain path. No migration or old-font deletion runs.
The search list does not retain the old default root when an override is selected,
nor infer font directories from XDG_DATA_DIRS or arbitrary native configuration.

The helper stages on the target filesystem, syncs new files at 0644 and publishes
without replacement. Limits are 64 MiB/font, 2 GiB/family, 512 fonts, 10000 scanned
entries and 16 directory levels. Reads are per file, not a whole-family memory
buffer. Only [OpenType/SFNT/collection signatures](https://learn.microsoft.com/en-us/typography/opentype/spec/otff)
and legacy Apple SFNT signatures are checked, not tables, checksums, glyph rendering or OS activation. Rollback keeps
owned file descriptors alive and checks identity plus source/current bytes; changed
or unavailable sources conservatively retain the published copy. A changed private
staging-directory identity disables cleanup, avoiding recursive deletion through
an observed substituted path. Errors identify paths requiring review. This is not
a crash-safe multi-file transaction or exclusion of concurrent external writers;
directory changes can leave temporary files needing review. Publisher authenticity,
native Homebrew behavior and cache refresh remain separate gates.

For the download-to-publication pipeline, run `cargo test --locked --lib font_install_`.
This includes the copy regressions above plus private curl scripts and synthetic
Stored/Deflate/ZIP64 archives. It covers fixed HTTPS arguments, timeout/output/file
limits, failed or missing downloads, extraction cleanup, unsafe paths and special
files (including ignored non-font members), duplicate names, overlapping ranges,
CRC/expanded-size failures, and forged directory/count fields. No real font is
downloaded or installed, and no native font-cache command runs.

Direct downloads have a 300-second post-spawn capture budget, 64 KiB combined
stdout/stderr cap, and 512 MiB per-file write limit in the curl child. The first
`--disable` argument suppresses curlrc; protocol flags restrict initial and redirect
URLs to HTTPS. Child-only `RLIMIT_FSIZE` also bounds unknown-length responses on
[older curl versions](https://curl.se/docs/manpage.html#--max-filesize); parent
limits remain unchanged. Executable resolution respects actual PATH before the
normalized fallback, and the executable is trusted, not sandboxed. Filesystem,
spawn and OS termination are not hard-bounded by the capture deadline.

Before constructing `ZipArchive`, an outer-record guard bounds entry counts and
the central directory (10000 entries / 8 MiB). ZIP64 is supported; split volumes,
prefixed archives, encryption and codecs other than Stored/Deflate are rejected.
Metadata checks run before any extraction file is created. Only `.ttf`, `.otf`,
`.ttc` and `.otc` files are streamed into a private flat directory at 0600; paths from the
archive never create directories or links. Declared and actual byte counts must
agree and fonts are read to EOF for CRC validation. A 60-second cooperative budget
is checked between decode reads, not an OS-level hard deadline. Non-font contents
are not decompressed or CRC-validated. Complete extraction precedes file publication;
failed staging is dropped. Source, staging and installed copies may coexist, so the
per-family bound is not a total-disk bound or a free-space guarantee.

The 2 GiB expanded-family limit allows larger distributions: upstream's
[v3.5.1 assets](https://github.com/ryanoasis/nerd-fonts/releases/expanded_assets/v3.5.1)
list IosevkaTerm.zip at 384 MB, and its
[regular face](https://github.com/ryanoasis/nerd-fonts/blob/v3.5.1/patched-fonts/IosevkaTerm/IosevkaTermNerdFont-Regular.ttf)
alone is 13.8 MB. This sizing evidence is not a live real-font installation test.
HTTPS plus ZIP CRC detects transport/archive corruption, not a compromised
publisher; release pinning and authenticated integrity are separate work.

For post-install font-cache results, run:

```bash
cargo test --locked --lib font_cache_
cargo test --locked --test font_cache_outcomes
```

Private executable fixtures exercise the Linux backend even on macOS, with no
native font parsing or cache writes. They cover profile/argument handoff, macOS
skip, missing/unstartable/nonzero commands, timeout/output limits, unsafe directory
rejection, retained fonts and fallback chains that do not redownload after a cache
warning. The real CLI test selects a private non-catalog filename fixture and
checks that it neither runs a cache/installer command nor claims a refresh.

Refresh uses the same bounded process-group capture as other native helpers:
30 seconds after spawn and 64 KiB combined stdout/stderr. Raw native output is
omitted. It resolves actual PATH first, passes the selected HOME, XDG_CONFIG_HOME,
XDG_CACHE_HOME and XDG_DATA_HOME, removes Fontconfig debug-output variables and supplies exactly
the resolved user-font directory after `--`. The
[`--error-on-no-fonts` flag](https://manpages.debian.org/trixie/fontconfig/fc-cache.1.en.html)
prevents an empty font scan from succeeding. Command success is only the observed
exit status; it does not prove the selected family is matched or rendered.

The install APIs return the cache outcome after successful file publication;
failed file publication never invokes the refresh. Cache warnings are nonfatal
for file/configuration success and stay visible in setup notes or CLI warnings,
including before subsequent CLI config writes that may fail independently.
Already-installed font selection reports `NotRequested`, and macOS reports
`NotNeeded` without resolving or executing `fc-cache`.

The directory check rejects links below the selected root and missing paths
(explicit data-root aliases are resolved), but is not exclusion of
concurrent external renames. A directory argument narrows the requested scan; it
does not sandbox the trusted executable or
[native Fontconfig configuration/cache paths](https://fontconfig.pages.freedesktop.org/fontconfig/fontconfig-user.html).
Native cache writes may remain after a timeout/failure and are not rolled back;
filesystem, spawn and OS termination are outside a hard wall-clock guarantee.

For font discovery, run `cargo test --locked --lib font_discovery_` and
`cargo test --locked --test font_discovery`. Private synthetic files and real CLI
subprocesses cover nested/mixed-case/collection names, normal links and cycles,
directory/FIFO/socket/empty/text impostors, incomplete-scan decisions, escaped
diagnostics and entry/directory/depth limits. Unreadable-file coverage explicitly
does not claim a mode-000 denial when tests run as root. The shared extension and
signature rules also apply to release extraction and no-overwrite publication;
`font_install_` regressions cover `.OTC` through those paths.

For the shared candidate list/picker, run `cargo test --locked --lib font_list_`,
`cargo test --locked --test font_list`, and the single
`cargo test --locked --test picker_exit font_list_picker_preserves_literal_recommendation_suffix_on_real_selection`.
Pure fixtures cover every JetBrainsMono variant, exact-name preservation, stable
unique picker keys, ambiguous normalized matches, positive partial observations,
withheld unknown downloads and loss-aware serialization. Real listing commands
cover empty profiles, malformed settings, busy writers, pending recovery, XDG
isolation, invalid option combinations and a closed stdout. No saved settings or
native font tools are needed. A private PTY chooses a synthetic, non-catalog family
whose literal name ends with a display-like suffix, checking the saved family and
generated file. Its executable tripwires forbid native installers/cache/reloads;
this verifies interaction and data handoff, not a font engine.

`font --list --json` uses schema v1 with candidate family/kind/recommendation data,
separate catalog IDs/families/matching candidates and `download_offered`, scan
completeness, search roots and bounded issue paths. Catalog `presence` is
`candidate_found`, `not_observed` or `unknown`, not a native installation verdict.
Multiple normalized matches stay visible and require exact selection; they are
not collapsed to one family. Candidate labels are escaped/decorated separately
from the family passed to writers. The list returns success for a produced report,
including partial scans: consumers must inspect completeness before using absence.
Partial inventories never create new catalog download offers. Listing bypasses
writer/sound initialization; font selection still uses a writer guard and the
file checkpoint described below. The normal discovery budgets apply; OS filesystem stalls and concurrent
changes remain outside a hard deadline. Linux test cross-checks are compilation,
not execution against a native Linux font stack.

For list search, select `cargo test --locked --lib font_list_search_` and
`cargo test --locked --test font_list`. Pure filtering tests cover reordered terms,
case/separator handling, unchanged source evidence, catalog-only aliases, empty
versus symbol-only queries and safe display. Real CLI fixtures preserve the full
profile tree during busy writers, pending recovery, invalid settings and partial
font discovery; invalid options/oversized queries are rejected without HOME, and
closed stdout remains harmless. Query parsing does not consume a following flag;
leading-dash values use `--search=VALUE`. Queries are validated at the CLI boundary
and again in the public handler before scanning.

The optional schema-v1 `search` object contains `query`, `total_candidates`,
`matched_candidates`, `total_catalog_entries` and `matched_catalog_entries`.
Create `Choices` from the whole scan BEFORE retaining query matches; never
recompute presence/download offers or filter `matching_candidates` and scan issues
from the narrowed view. An empty view is not an empty scan. The full discovery
cost/budgets still apply, and the legacy `handle_list(env, json)` wrapper remains
available for library callers.

For read-only font previews, run `cargo test --locked --lib font_preview_` and
`cargo test --locked --test font_preview`. Injected scan reports cover catalog
aliases without native installation, partial negative/positive evidence,
ambiguous versus exact names, loss-aware JSON and escaped terminal paths. Real
CLI tests compare planned actions/byte counts with an isolated application of a
non-catalog fixture, including the no-op repeat. They verify no writes during
contention/pending recovery, unsafe/oversized files, malformed Alacritty input,
closed output and invalid CLI combinations. Keep subprocess tripwires active.

`font <name> --dry-run --json` is a schema-v1 report, not a mutation gate. Inspect
`file_plan_complete` and `blocker`, not just exit success; files are withheld when
preparation is incomplete. `execution_readiness_checked` stays false: preview
does not test writes, snapshot creation, writer/recovery gates, network or native
activation. Report intent only after the same `PreparedFont::capture` used by
application succeeds. Preserve ordered file actions and never expose captured
configuration bytes. This remains distinct from the settings-free font list.

For prepared font changes, run `cargo test --locked --lib prepared_font`,
`cargo test --locked --lib font_commit_failure`, and
`cargo test --locked --test font_commit`. Fault injection checks saved-choice and
notification ordering, late conflicts, input/output aliases, retargeted parents,
new optional files, no-op metadata and isolated-session gating. Private CLI
fixtures cover exact recovery (bytes/modes/absence), malformed TOML/preferences,
symlinks, FIFOs, oversized files, blocked backup storage and import checkpoint
reuse. Synthetic non-catalog font headers and executable tripwires keep these
tests away from host installers, native caches and running terminals.

For the shared direct/picker application lifecycle, run
`cargo test --locked --lib font_flow_`, `cargo test --locked --test font_commit`,
and `cargo test --locked --test picker_exit font_list_picker_`. The injectable
installer checks preflight/readiness/checkpoint ordering, failure propagation,
cache warnings and post-install conflicts using real private configuration writes
but no downloads/cache commands. CLI/PTY checks cover normal, quiet and auto
feedback and exact family selection; preferences disable sound. Quiet suppresses
font success/progress, not errors, cache warnings, recovery stderr or the picker
prompt. `src/cli/font/apply.rs` owns the shared lifecycle; only download presentation
differs between direct and interactive selection. Emit completion only after commit.

For advisory font-name hints, select `cargo test --locked --lib font_suggestions_`,
`cargo test --locked --test font_preview font_suggestions_`, and
`cargo test --locked --lib theme_input_` for the shared bounded distance helper in
`src/lookup.rs`. Suggestions never feed selection. Complete-scan unknown names
can show at most three exact-family hints; partial scans retain their existing
unknown/download gate. Exact names still beat normalized aliases, and ambiguous
matches list bounded exact choices plus a remaining count. Names are debug-escaped
for display, not shell-ready commands; catalog suggestions are labeled as possible
downloads, not verified absences. Very short/long queries omit fuzzy hints. At
most 4096 sorted unique observed names are scored, with a truncation notice when
needed; catalog alias suppression still checks the full captured inventory.
Private CLI tests prove that suggestions preserve both failed selection and
blocked JSON previews without preparing files, creating checkpoints or installing.

Font fallback policy lives in `src/cli/setup_executor/font_install/chain.rs` and
is used by setup and direct/picker/import selection. Its report records the
successful source, cache result and preceding known failures. Setup presentation
and availability tracking live in `font_stage.rs`; neither marks the choice saved
or proves rendering. A skipped choice does not scan or invoke any installer.
Incomplete discovery without positive evidence cannot start installation. Cache
warnings after successful publication stop the chain, while typed uncertain
errors pass through unchanged before presentation. Only ordinary all-path
failures are combined; never wrap uncertainty to retain earlier diagnostics.
Run `cargo test --locked --lib font_chain_` and
`cargo test --locked --lib font_stage_` for the shared reports and setup outcomes.
These inject native installation and discovery; private marker files verify that
partial/successful publication is retained without running real font installers.
The existing fallback tests below also exercise the shared implementation.

For Homebrew font capture/fallback changes, use
`cargo test --locked --lib font_brew_` and
`cargo test --locked --lib font_cache_fallback_chain_`. Private shell executables
check exact argv, closed stdin, successful/known failed exits, unstartable files,
signals, noisy output and inherited pipes after the leader exits. Test timeouts
are short; no real Homebrew, network, font engine or cache is run. Production
allows 600 seconds after startup and 512 KiB across both pipes, using the shared
owned-process-group capture. Filesystem/spawn and OS termination are not hard
bounded; detached descendants are not a sandboxed workload. `HomebrewInstallUncertain`
is a typed stop-fallback condition, shared by setup and direct/picker selection;
do not replace it with string matching or wrap it away before fallback decisions.
Font-phase setup records an issue and does not mark the selected font available; independent
configuration steps may continue and the overall setup remains incomplete.
Ordinary completed failures retain the existing fallback policy and can also
leave partial installer changes. Native output is omitted, including on errors.
The shared executor now lives in `src/platform/packages/homebrew.rs`; font and
tool call sites keep their distinct output/deadline limits. Tool installs use
1800 seconds and 2 MiB. An uncertain tool result returns immediately from setup;
the handler adds its existing recovery point and does not run follow-up setup.
The Starship permission fallback checks the typed gate BEFORE inspecting messages.
This does not sandbox detached subprocesses. The public formula package API and
wizard now share this same executor, eliminating the older unbounded duplicate.
`BrewKind` lives in the platform package layer and is re-exported by the catalog
to preserve its existing API. Homebrew policy fixtures remain in setup_executor.

For tool-path changes, run `cargo test --locked --lib homebrew_tool_` and
`cargo test --locked --lib test_local_starship_fallback_triggering`. Private
executables exercise formula/cask argv and classifier behavior; an injected tool
installer drives the real prepared-setup sequence, leaves one private partial
package marker, and proves the next installer/font/configuration never runs.
Font capture tests above continue to cover timeouts, noise, signals, failed spawn
and inherited pipes through the same executor. No real Homebrew or network is used.

For apt changes, use `cargo test --locked --lib apt_`. Private apt/sudo executable
fixtures inject effective-root and helper resolution, assert exact mapped package
argv, root's absence of sudo, fixed sudo environment assignments, closed stdin,
known error classification and typed capture uncertainty. Injected backend routing
proves Starship reaches local installation on apt, ordinary successes retain the
correct backend, and unknown backends/uncertain outcomes never invoke a fallback.
The actual prepared setup uses a private failed bat installer followed by a delta
tripwire, proving later tools/fonts/configuration do not run after apt uncertainty.
No actual apt, sudo, privilege change, repository or package database is used.
Linux cross-clippy is static validation, not a real Linux package installation.

For package-manager-free bootstrap, run `cargo test --locked --lib bootstrap_`,
`cargo test --locked --lib apt_tool_routes_`, `cargo test --locked --lib tool_inventory_`
and `cargo test --locked --lib setup_retry_`. The shared pure policy lives in
`src/platform/packages/routes.rs`: distinguish an unsupported OS from a supported
OS without Homebrew/apt. Starship gets the staged local route in the latter case;
other tools retain explicit missing-manager/mapping failures. Policy fixtures
drive actual dispatch with installer tripwires, preflight intent checks, candidate
filtering, inventory notes and final Quick selection validation. No installer or
network operation runs. Existing Homebrew inventory snapshots remain unchanged.
The retry collector's only checks are OS/architecture/selected route; it does not
run the full setup's unrelated font/tool inventory, DNS or config write probes.
Production still validates IDs before lock/profile initialization and keeps the
normal write guard. Catalog-only `compute_install_candidates` remains available;
the wizard uses its platform-aware wrapper. Installed configuration targets are
independent of missing-tool installation candidates.

Guided preflight makes the package-manager report advisory until choices exist;
Quick infers package needs from missing core tool routes, not font download need.
`src/cli/tool_selection/quick.rs` owns the shell-dependent core list shared with
both package and network preflight: Starship for Bash/Fish; Starship and
zsh-syntax-highlighting for Zsh. Detection uses the existing `SHELL` convention,
not the parent process or an executed shell probe. Unsupported shells are blocked
by shell preflight and Quick planning. Non-Zsh Quick defaults omit the Zsh-only
adapter even if detected; do not remove existing files, change preferences, or
filter explicit Manual/`--only` installation choices. Keep fallback-tier core and
current-terminal configuration, and deterministic ordering of detected defaults.
Run `cargo test --locked --lib quick_shell_` for policy, package/network intent,
manual availability and actual wizard selection/review (including force mode).
All shell/backend/discovery inputs are injected; no installers or DNS run.
Quick selected from a guided entry checks routes before review, and the handler
revalidates final choices before snapshots/preferences/installers, including with
`--force`. It does not silently discard required tools. Runtime helper/path/access
checks remain separate, and discovery is not atomic with installation. The legacy
generic `RetryInstall` preflight API stays conservative because it lacks a tool ID;
the CLI's exact retry uses `run_checks_for_retry`. Full-setup DNS text now describes
resolution evidence rather than claiming that release downloads are available.

For confirmation/execution consistency, run `cargo test --locked --lib install_review_`,
`cargo test --locked --lib review_receipt`, `cargo test --locked --lib setup_plan_`
and the focused `apt_tool_routes_` / `homebrew_tool_` policy regressions.

Setup loader preparation is in `src/cli/setup_executor/shell_loader.rs` and is
captured by `PreparedSetup` before snapshot/preferences/installers. The standalone
shell-integration helper also captures before theme writes. Reads and generated
content are bounded to 8 MiB; reject final links and special files, use the shared
recovery path contract, and reuse the byte-preserving marker transform for
Bash/Zsh. Fish remains an entirely generated Slate-owned conf.d entry. Compare
bytes, ordinary mode, file identity and resolved parent path before execution,
before theme work and at publication (including after creating missing parents).
Do not replace later user edits, and do not truncate through a hard link. Preserve
ordinary modes on atomic replacement, use 0600 for new loaders and skip identical
content. This does not pin all ancestor inode identities, exclude external writers,
roll back prior package/theme work or guarantee parent-directory fsync.
Run `cargo test --locked --lib shell_loader_`, `cargo test --locked --lib fish_paths_`,
`cargo test --locked --lib setup_plan_`, `cargo test --locked --test setup_plan`
and `cargo test --locked --test setup_outcomes setup_outcome_real_executor_`.
Private fixtures cover invalid sources, modes, hard links, no-op identity and
post-plan drift; the deadline-guarded public executor rejection test includes a
FIFO and broken markers. Configuration-only outcome tests now expect invalid
loaders to return before any theme changes, not an incomplete post-write summary.
No package/font installer or live terminal runs.

`src/cli/tool_selection/install_plan.rs` captures deduplicated catalog actions,
their routes and the user-local binary destination before the wizard asks for
confirmation. The real receipt renderer consumes that capture, including apt's
mapped package name, escaped local path, Starship fallback policy and file-only
backup scope. Repeated/failed review clears prior approval. Legacy catalog-only
receipt APIs/struct literals and their snapshots remain compatible; they do not
represent a bound execution plan.

The setup handler binds the confirmed plan to `PreparedSetup` before snapshot or
preference writes. Selection/order/metadata/destination and current route checks
reject drift; `execute_bound_plan` rechecks all routes up front and each route
before starting a tool. The guard is outside the ordinary installer-error/continue
branch, so even a previous known failed exit cannot let a changed next route run.
The installer consumes `PlannedToolInstall` rather than redetecting a backend.
Library callers without a wizard capture an execution plan once at entry; that
does not claim interactive confirmation. Tests inject all contexts and installers
for drift cases, leave only a private partial marker, and prove that no subsequent
installer/configuration runs. They do not invoke package managers, downloaded
binaries or network services. Route capture is not executable identity/version/
repository pinning or external-writer exclusion; fonts retain their separate
installation policy. Known Homebrew failures may still take the fallback disclosed
on the card, and existing file-recovery guidance does not roll back packages.

`src/platform/packages/apt.rs` is shared by wizard and public package entry points.
It uses 1800 seconds/2 MiB post-spawn capture; effective root bypasses sudo, while
non-root uses `sudo -n DEBIAN_FRONTEND=noninteractive LC_ALL=C -- <apt-get> ...`.
Assignments must precede `--`; sudo's
[argument parser](https://github.com/sudo-project/sudo/blob/main/src/parse_args.c)
ends environment-option parsing at that delimiter. Fixtures assert this ordering.
No privileged shell or `env` wrapper bypasses sudo policy; environment assignments
can be refused. Apt receives `install -y --no-remove -o DPkg::Use-Pty=0 -- <package>`.
Debconf defaults do not guarantee every package script/conffile operation is
noninteractive; failures are reported without an automatic force-conf, update or
repair retry. See the primary [sudo manual](https://manpages.debian.org/bookworm/sudo/sudo.8.en.html),
[apt-get manual](https://manpages.debian.org/trixie/apt/apt-get.8.en.html) and
[debconf manual](https://manpages.debian.org/bookworm/debconf-doc/debconf.7.en.html).
`AptInstallUncertain` joins the shared typed stop gate. Elevated processes can
outlive the owned capture group; never imply that timeout kills all package work,
rolls back dpkg or permits deleting lock files. Known failed exits retain setup's
ordinary issue/continue behavior and can also leave partial package state.

For user-local Starship changes, use `cargo test --locked --lib starship_local_`.
Private absolute-path curl and shell fixtures cover exact argv/closed stdin,
staging cleanup, atomic replacement and controlled permissions, blocked targets
before download, invalid/oversized/special staged files, content/mode/inode or
directory changes, HOME aliases, bounded download/installer failures and the real
setup early-stop sequence. No upstream installer or downloaded binary is executed.
`starship.rs` owns capture/staging; `starship/target.rs` owns optimistic target
checks and publication. The shared typed fallback gate lives in setup_executor;
`HomebrewInstallUncertain`, `AptInstallUncertain` and `StarshipInstallUncertain`
stop tool setup.
The latter includes publication errors conservatively, without promising that
rename did not occur. Download capture is 75 seconds/1 MiB, shell capture is
600 seconds/2 MiB, and regular-file validation is capped at 64 MiB. All native
output is omitted from errors. Script downloads disable curl configuration and
allow HTTPS-only redirects; the upstream script's own network/extraction behavior
is still trusted, not subject to a disk quota or sandbox. These are post-spawn
wait limits, not hard filesystem/spawn/OS termination deadlines. Staging does not
verify authenticity, architecture or runtime health, and file checkpoints do not
recover binaries or arbitrary script side effects. Directory fsync remains
best-effort through the shared atomic writer; checks do not exclude external writers.

For literal font-reference fixes, run `cargo test --locked --lib ghostty_literal_`,
`cargo test --locked --lib font_literal_`, and the single integration cases
`cargo test --locked --test font_commit font_literal_cli_` and
`cargo test --locked --test clean_preview ghostty_literal_clean_preview_`.
These exercise full values versus embedded paths, local list resets, optional
references, BOM preservation, legacy migration, Kitty continuations and unreadable
entries. The CLI tests compare preview/execution and restore exact user bytes.
Ghostty's shared parser lives in `src/adapter/ghostty/references.rs`; its existing
`ghostty_references_` tests still cover the pinned grammar and resolution rules.
File transforms deliberately do not inherit the recursive diagnostic's native
validation/line-limit checks. Receipts are literal-reference observations, not
proof of effective fonts, and never expose unreadable configuration contents.

`PreparedFont` prepares bytes before writing, checks bounded input/target identity,
and publishes `current-font` last. The CLI owns `pre-font` creation before catalog
installation; imports own their existing wider checkpoint. No-op selection skips
checkpoint creation, not final verification or a session-eligible reload request.
The saved choice describes the last completed configuration publication, not a
native font activation test. A late failure can leave earlier writes behind;
explicit recovery may also revert intervening user edits. Do not describe this
as automatic rollback, crash atomicity or exclusion of external writers. Raw
single-terminal helper APIs do not independently create recovery points.

For font diagnostics, run `cargo test --locked --lib font_doctor_` and
`cargo test --locked --test font_doctor`. The real CLI fixtures cover empty and
busy profiles, malformed recovery/general-config records, exact generated-byte
matches and drift, partial scan evidence, saved-family validation, XDG paths,
SSH scope, isolated path escapes, and FIFO/directory/link/size rejection.

For direct font-entry checks specifically, use
`cargo test --locked --lib font_doctor_references_` and
`cargo test --locked --test font_doctor font_doctor_references_`. Fixtures distinguish
correct generated bytes from actual direct references, inactive Alacritty imports,
Ghostty resets, Kitty continuations, ordinary/isolated links, partial positives,
invalid files and bounded reads. They preserve profile/outside trees and use native
tool tripwires. `src/adapter/font/references.rs` owns the shared receipt/diagnostic
observation model; it never reads referenced files or starts a font engine.

The optional schema-v1 `font_references` array appears only for the font target.
Its per-terminal state is `found` when any inspected entry contains a literal
reference, even if `inspection_complete` is false due to another entry. Otherwise
unreadable/path errors take precedence over `not_found` or `missing`; ordinary
missing candidates are not read errors. Completeness covers these bounded direct
checks only, not whole-file syntax, recursive/relative includes or native behavior.
Read-only link acceptance differs from the stricter recoverable mutation policy.
Private executable tripwires and before/after file snapshots ensure the report
does not launch an installer, cache builder, font engine or terminal, or rewrite
configuration. Tests use synthetic font headers, not rendering or registration.
Inventory unit fixtures distinguish positive evidence, unknown partial absence
and complete-but-limited search, including omitted issues and lossy path display.

The `font` target extends the schema-v1 integration report with optional
`font_inventory`: candidate source, backend, valid selected family or null,
scan completeness, Nerd/system family counts and retained/omitted issue counts.
Other targets omit this property. Issues use existing `checks` with stable codes
and per-path lossiness flags. Invalid state and generated content are not included;
valid selected names are included intentionally. Text output escapes terminal
controls and directional formatting. A zero exit status means report production,
not verified health. This target does not support `--check-version` or any native
probe. It reads at most 4 KiB for saved state and 8 MiB per terminal output, using
the shared recovery-compatible file-path guard without acquiring a writer lock.
Font discovery uses the same bounded scan as selection, including standard system
roots and ordinary font links even in an isolated profile; only configuration
reads are prevented from escaping SLATE_HOME. There is no whole-filesystem snapshot
or hard filesystem-call deadline. Candidate absence, template equality and a
configured search root do not prove native absence, runtime selection or writable
installation paths; repair suggestions require an explicit subsequent command.

Discovery scans at most 50000 directory entries, 4096 distinct directory identities
and 16 nested levels; it retains 32 issue paths and a count of further issues.
Readable file/directory links are followed in this read-only scanner, including
links outside the initial root; aliases/cycles deduplicate by device/inode.
Installation rejects links below its resolved HOME or explicitly selected data
root, while discovery may follow them read-only. Each candidate
is checked as a regular file and read for only a 12-byte prefix, with size,
identity and timestamp rechecks. No font renderer, cache command or complete
font-file read is needed. Filesystem calls have no hard wall-clock deadline.

The extension/signature set follows the
[OpenType file specification](https://learn.microsoft.com/en-us/typography/opentype/spec/otff)
and [Apple SFNT reference](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6.html).
This is deliberately candidate discovery: internal name tables, collection faces,
native font registration and Nerd Font glyph coverage are not parsed or proven.
System candidates use the platform's existing family whitelist. Invalid signatures
are excluded; unreadable or changed paths and scan limits produce incomplete-scan
evidence instead of silently returning an empty inventory.

Setup and explicit CLI selection may use a positively observed family in a partial
scan, but cannot infer missing catalog fonts from it. The picker shows known
candidates and hides downloads until the scan is complete. Preflight retains
unknown download needs as unknown; strict legacy vector-returning discovery APIs
return an error on incomplete scans rather than hiding it. The legacy decorated
recommendation list is presentation-only and is no longer used by setup's install
decision or family resolution.

```
cargo test
```

Integration tests use the `SLATE_HOME` environment variable to isolate runs from your real `~/.config`. No test touches your dotfiles.

For installer changes:

```bash
bash tests/install-script-smoke.sh
cargo test --locked --test install_script
```

The shell smoke checks dry-run platform/URL selection. The Rust integration tests
exercise actual archive reading and file replacement in temporary directories,
with offline download fixtures and failure-injected install/rename commands.
They cover conventional cargo-dist and root layouts, `./` prefixes, strict
checksums, duplicate/empty/linked/missing archive members, failed copies/renames,
SIGTERM cleanup, preserved old inodes, and protected destination symlinks/directories.
A failure observed after rename must not falsely report that the old file remains.
No downloaded executable is run; an unexpected sudo attempt fails via a tripwire.
CI runs these against the native BSD tar (macOS) and GNU tar (Linux). These tests
do not exercise real privilege escalation, SIGKILL cleanup, or power-loss durability.
Only the selected regular member is streamed to a private file; the archive tree
is never extracted. Supported layouts are `<asset-name>/slate` or root `slate`.
Release checksums detect corruption, not compromise of the release publisher.

For shared configuration IO changes:

```bash
cargo test --locked --test config_read_safety
cargo test --locked --lib shared_reader_
cargo test --locked --lib import_snapshot_source_
```

The CLI checks use private HOME/SLATE_HOME and short subprocess deadlines. They
cover FIFO preferences and normal theme snapshots, scalar sections (no panic),
content-free parse failures, UTF-8/size boundaries, exact comments/inline-table
preservation, one-sided auto-pair changes, and linked binary dotfile snapshots.
The common reader accepts regular files only; import/export refuse final links,
while ordinary getters/snapshot copies follow valid links. Missing parents differ
from dangling parents. Backup bytes and modes come from the same open descriptor;
size/identity checks detect common concurrent changes, not arbitrary external edits.
Complete snapshot builders publish the manifest last; this does not make the
legacy single-entry append API transactional. There is no filesystem IO timeout
or guarantee against a hostile directory owner racing the entire operation.
When changing this reader, also select the import/export and restore round-trip
regressions affected by the change; do not run the entire suite by default.

For font-name serialization and selection:

```bash
cargo test --locked --test font_names
# Optional native config validation; never opens/reloads a terminal window:
SLATE_TEST_GHOSTTY_BIN=/absolute/path/to/ghostty cargo test --locked --test font_names
```

The tests parse both generated Alacritty TOML files, exercise Kitty's shell-quote
boundary with private Bash arguments, reject multiline/control-bearing names
without file mutations, and verify exact/ambiguous font selection through the CLI
and import. The optional Ghostty validator loads an explicit fixture file, with a
separate disposable diagnostic cache. No real font is installed or rendered;
Kitty native font selection is not exercised by the Bash argument check.
Syntax references: [Ghostty 1.3.1 line parsing](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/cli/args.zig),
[Kitty 0.36 family specs and quoting](https://github.com/kovidgoyal/kitty/blob/v0.36.0/kitty/fonts/__init__.py),
and [Kitty font specification documentation](https://sw.kovidgoyal.net/kitty/kittens/choose-fonts/#the-font-specification-syntax).
Do not apply TOML/JSON backslash escapes to Ghostty's literal config values.

For setup startup hints, use `cargo test --locked --test startup_detection` and
`cargo test --locked --lib -- cli::font_detection::tests cli::wizard_core::tests`.
The hazardous-input probe runs `Wizard::new` only in a private child process with
a four-second test deadline; its ignored helper is invoked by the parent tests.
Direct `Wizard::with_env` checks cover custom XDG/captured profiles, adapter-selected
Alacritty candidates, inline/dotted TOML, safe names, and file/directory links inside
and outside isolated profiles. Exact 8 MiB config/4 KiB state boundaries must work;
over-limit sources must not yield hints parsed from a truncated prefix. Compare
whole private trees before/after construction. These are direct-file hints, not
native include-graph/font rendering validation; later interactive wizard actions
and setup writes are outside these read-only construction tests. Filesystem IO
has no hard wall-clock guarantee and external directory edits are not locked out.

For retry/profile plumbing and preflight probes, select `setup_retry`,
`preflight_write_probe` and `wizard_profile` lib filters, plus
`cargo test --locked --test setup_retry --test startup_detection`. Retry callbacks
assert exact profile identity, preflight-before-install ordering, failure propagation
and no initialization for invalid targets; they intentionally do not invoke package
managers or network checks. The real CLI test verifies exit 1, escaped errors and
unchanged private trees with and without HOME. Probe tests preserve existing bytes,
modes and links at the former fixed name and check that random probes leave no
residue. Wizard receipt/selection changes also need `cli::wizard_core::tests` and
`cli::wizard_support::tests`. These do not establish system installer isolation or
end-to-end package installation success; do not run a real installation as a test.

For full-setup outcome handling, use `cargo test --locked --lib -- setup_outcome
cli::failure_handler::tests` and `cargo test --locked --test setup_outcomes
--test wave1_events`. The `setup_outcomes` parent runs four configuration-only
executor cases (complete, rejected unknown theme, broken Alacritty TOML, blocked
shell loader) in private child profiles with cleared PATH, explicit Bash and
six-second deadlines. No tools or
fonts are requested; the ignored child helper is invoked by its parent. Unit
callbacks verify preference failure ordering, font persistence, Neovim follow-up
errors and exactly one matching final event without native installation or editor
launch. Marker checks cover non-UTF-8 bytes, oversized files and nonregular inputs.
`ExecutionSummary::is_successful()` is authoritative; `overall_success` is only a
refreshed compatibility mirror. The executor must not emit whole-setup milestones
before handler follow-up finishes. These checks do not exercise the full wizard,
real package managers, font downloads, Neovim activation or graphical appearance.
Setup recovery is captured-file recovery, not an automatic transaction/uninstall.

For setup-plan preparation, use `cargo test --locked --test setup_plan` and the
`setup_plan` lib filter. Private subprocesses verify rejection of unknown tool
IDs, invalid themes/font syntax, unsupported shells and bad saved-state sources
(unknown, nonregular, non-UTF-8, oversized and dangling link), comparing the entire
profile tree before/after. No valid tool or font installation is requested.
Plan-only unit tests check deduplication, catalog display names versus literal
families, captured theme/profile/shell and no directory initialization. A real
configuration-only run changes saved state between preparation and execution and
must still publish the planned theme/loader. Keep
`test_apply_does_not_commit_current_when_no_adapter_applied` green: only successful
setup shell activation adds the no-adapter theme publication. Planning performs
no font inventory/network probe and is not a write-permission or transaction
guarantee. The interactive wizard and real package/font installers are not tested.

For theme apply ordering and failure receipts, select
`cargo test --locked --test theme_commit --test theme_safety --test session_behavior`
plus `cargo test --locked --lib cli::apply::tests` and
`cargo test --locked --test restore_confirmation restore_reapplication_checks_shared_failure`.
Fixtures cover bad shared settings, a later shell-file write failing after earlier
writes, blocked current-theme tracking, no-target/no-snapshot application, quiet CLI
errors, successful retry, file restoration and restore-reapplication undo. They use
private profiles; native version/cache work uses fixture executables or explicitly
private paths. `failed_count()` counts adapters (including post-commit notifications)
only; callers must use `ensure_no_failures()` to check the full report, including
`commit_failure`. Shared shell writes must finish before current-theme tracking,
and either failure must skip auto-pair/Neovim advancement while retaining recovery
context. Do not claim these ordered writes form a multi-file atomic transaction.

For opacity publication and its checkpoint path contract, select
`cargo test --locked --test opacity_apply --test import_recovery --test baseline_backup`,
`cargo test --locked --test restore_confirmation opacity_checkpoint_cli_restore`,
and the lib filters `opacity_failure_`, `opacity_late_failure_`, `opacity_reload_`,
`test_apply_opacity_`, `cli::set::commit::tests`,
`restore_behavior_keeps_operation_checkpoints_file_only`, `opacity_prepared_`,
`opacity_noop_`.
Keep `opacity::MANAGED_FILES` aligned with the four actual adapter outputs; both
theme/picker snapshots and import checkpoints use it. Persisted opacity is written
last, never during preview. A deterministic post-checkpoint hook exercises later
I/O failures without background races; injected reload effects verify captured
SSH/isolation gates without controlling apps. CLI fixtures cover bytes/modes/absence,
unsafe sources, directory aliases/collisions, private backup modes, file-only restore
and undo. Ordinary preview uses its journal and import uses its existing checkpoint.
`opacity::managed` owns both output templates and prepared write/skip decisions;
adapter entrypoints must delegate there. No-op regressions compare file identity,
mtime and the entire backup tree, then corrupt/remove each output independently.
Same-byte replacements, permission changes, new files and hard-linked directory
redirects must invalidate a prepared write, including one classified unchanged.
An explicit local reload still runs on an unchanged disk configuration; test that
through injected effects, not a real terminal. Retain ordinary modes on changed files.
The shared `config::recovery_paths` metadata checks and bounded snapshot reader do
not provide a transaction or exclusion against unrelated editors.

For read-only opacity diagnostics, select:

```bash
cargo test --locked --test opacity_doctor --test doctor_readonly --test completions
cargo test --locked --test opencode_doctor integration_doctor
cargo test --locked --lib doctor_integrations
cargo test --locked --bin slate completion_
```

Keep comparisons tied to `opacity::MANAGED_FILES` and its shared writer templates;
do not infer a default preset or effective runtime settings. Private-profile fixtures
cover all presets, absent/invalid/noncanonical state, byte drift, missing outputs,
FIFOs, final links, oversize files, directory aliases/collisions, broken recovery
storage, XDG selection, isolation and SSH. Compare the complete fixture tree and
use tool-launch tripwires. Shared report regressions cover terminal-safe text,
lossy paths, held locks, pending previews and actual closed output pipes. A completed
report exits zero even with error checks; scripts inspect `checks[].status`.
Check codes can repeat across files and must be paired with `path`. Diagnostic read
failures must not imply that application was attempted. If changing shared
`config::recovery_paths` validation, also select `cargo test --locked --test import_recovery`.

For shared-code previews and import input validation:

```bash
cargo test --locked --test import_preview
cargo test --locked --test share_export
cargo test --locked --test import_recovery
```

These compare fixture bytes/modes before and after preview or rejected input,
including FIFO preferences, a held writer lock, pending recovery, missing HOME,
invalid profile environment, unavailable fonts, and early stdout closure.
Subprocess tripwires ensure previews cannot discover/download fonts or reload tools.
Preview reports requested settings only; font resolution, target writeability and
full-import rollback are explicitly outside its guarantee. Actual valid imports
still acquire the writer lock; validation now runs before lock/sound initialization.
A private application check verifies that `none` keeps the existing theme/font
while omitted tool flags are disabled and the requested opacity is written.

Import recovery tests run whole imports with private HOME/SLATE_HOME and harmless
tool stubs, checking both success and a later adapter failure after the font step.
They compare every changed file with the checkpoint contract, restore exact bytes,
modes and prior absence, and undo that restore. Unsafe/oversized sources must fail
before application or sound IO; failed captures clean up incomplete snapshots,
and interrupted captures must not publish a partial manifest. No real
font download, host cache build or app reload is performed. When adding an adapter
or a new import write path, update `src/cli/share/recovery.rs` and its coverage:
only tools selected before the checkpoint may be applied. Checkpoints are file-only;
external caches, font installations, empty directories and runtime state are excluded.

Export checks round-trip exact font text through a versioned UTF-8 font segment,
preserve legacy literal-percent semantics, exercise hostile/FIFO/symlink/oversized
settings, and execute the printed command with a private Shell argument-capture
stub. No real font is installed and no screenshot is captured. Tracking reads are
limited to 4 KiB, preferences to 256 KiB, decoded font names to 256 bytes and share
codes to 1024 bytes. File errors omit saved contents; concurrent multi-file writes
are not presented as a transactional snapshot. Missing tool flags use the same
defaults as ConfigManager; unset theme/font/opacity is exported as keep-current.

The v1 font segment uses [RFC 3986 percent-encoding](https://www.rfc-editor.org/rfc/rfc3986#section-2.1)
and keeps only unreserved ASCII bytes literal. Raw `none` is the keep-current token;
`%6Eone` denotes a literal font family named `none`. Decode exactly once and only in
v1; unknown versions, invalid UTF-8 and decoded controls are rejected.
The optional watermark doubles percent signs for [ImageMagick's property interpreter](https://imagemagick.org/escape/).
Its escaping unit check does not exercise screenshot capture or pixel layout.

For screenshot file safety, run `cargo test --locked --lib share_image_` and,
on macOS, `cargo test --locked --test share_capture`. Lib fixtures cover private
capture drafts, cancelled/absent/invalid files, unsafe/FIFO/sparse-oversized reads,
borrowed Portal URI sources, same-inode destinations, concurrent no-clobber saves,
private permissions, directory aliases, OS-byte argument passing and watermark
failure/timeout/output flooding. CLI fixtures shadow both screencapture and magick
with private scripts and isolate HOME/SLATE_HOME/PATH/TMPDIR. No test here captures
the screen, invokes real ImageMagick, changes desktop settings, or starts Portal.

Only the final, complete image bytes are published. A same-filesystem private
temporary file is synced, set to 0600 and persisted without replacement; collisions
select the next name, up to 10000 candidates. Directory fsync is best effort after
publication. The explicit-path capture API refuses existing targets rather than
silently picking another name. Final source links and special files are rejected
using the shared bounded reader; a [Portal screenshot URI](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html)
is treated as borrowed access, never authorization to unlink the source.

Watermarking receives a copy and a separate PNG output path. Keep the captured
bytes immutable until a successful result is read; a failed/invalid optional
result warns and falls back to those original bytes. The 10-second/64 KiB limits
use the owned process-group capture helper. Reads cap encoded files at 64 MiB and
check PNG signature only, not full decoding, CRCs, pixel appearance or decompressed
memory use. Limits do not hard-bound OS/filesystem work or all external-tool disk
usage. Directory resolution occurs after interaction, but is not a transaction
against concurrent directory renames. These file tests do not certify Portal
request/response ordering, real screenshot permissions or actual watermark rendering.

For screenshot request ordering, run `cargo test --locked --lib portal_screenshot_`
and check Linux lib/bin compilation. The private bus fixture sends real zbus method
and signal messages over an unnamed socket pair; its recorded AddMatch rules assert
that subscription precedes Screenshot. Both predicted and legacy handles emit a
Response before the method reply. Cases cover late responses, v1/v2+ interactive
options, wrong sender/path/interface/member, forged owner loss, cancellation,
malformed/oversized responses, early-response flooding, method failure after an
early signal, setup/query deadlines, simulated service activation and disconnects.
An explicit round trip inside result consumption verifies that the transport is
still live, followed by observed EOF even when consumption fails. Re-run
`cargo test --locked --lib portal_watch_` when changing the shared bus fixture.

Follow the [Portal Request ordering contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html):
subscribe before calling Screenshot and accept the actual returned handle. Slate
uses a dedicated connection and temporarily matches all Request Response paths
from the resolved unique owner, then filters by the exact returned handle. This
also retains an early legacy response without a resubscription gap. While the
method reply is pending, actively drain the stream into at most 16 messages, with
each body at most 64 KiB; overflow is an error, not silent data loss or an unbounded
buffer. Signals for other returned paths are ignored. These are application-level
limits, not caps on zbus's prior wire allocation or all bus traffic. Handle tokens
contain 32 OS-random bytes using getrandom, already present transitively in the
lockfile; there is no PID/time-based fallback on entropy failure.

Screenshot setup and version-only probes each have a 2-second asynchronous budget.
Setup covers connecting, owner subscription/lookup (and activation if needed),
the lowercase version query and Response registration. Owner changes are checked
before calling and throughout interaction. There is no short deadline on the
Screenshot method reply or the user's interaction. Borrowed URI copying happens
before closing the exclusively owned connection; shutdown has its own 1-second
best-effort budget and failure cannot turn a saved image into an apparent failure.
Tests do not launch a real session bus/Portal, capture a screen or change desktop
settings; bus routing/policy, service activation machinery, Documents portal access
and actual Linux desktop integration are not certified by this fixture.

For CLI discovery and shell completion changes, use the focused checks:

```bash
cargo test --bin slate completion_
cargo test --test completions
cargo test --test theme_catalog
```

Completion tests cover deterministic, profile-independent output (including locks,
pending recovery, FIFO preferences and early stdout closure), native Bash candidate
matching, and native Zsh syntax/registration on macOS. Fish positional rules and
catalog coverage also have structural checks.

For native Fish candidate-engine checks, install Fish in your development/test
environment or point `SLATE_TEST_FISH` to an existing absolute executable path:

```bash
cargo test --locked --features has-fish --test completions
```

The feature requires a real Fish executable instead of silently skipping checks.
Linux CI installs the distro Fish package and enables `has-fish`; macOS contributors
may use a separately extracted official app binary without installing it globally.
The test uses `complete --do-complete` for root/nested commands, global flags,
option values, consumed positionals and conflicting flags. Test HOME/XDG paths are
temporary; Fish's own initial directory/config seeding is established in a control
run before asserting that Slate completion neither launches Slate nor changes files.
This tests the native completion engine, not interactive terminal key handling.

Shell source quoting is separate from completion generation. Fish single-quoted
words must escape both backslashes and apostrophes; POSIX single-quoted words do
not follow that rule. `platform::shell::fish_quote` is used by both managed Fish
rendering and the setup loader. Do not feed POSIX-quoted model values into Fish.
See [Fish's quoting contract](https://fishshell.com/docs/current/language.html#quotes).
Use `cargo test --locked --lib fish_paths_` for literal/renderer/loader checks and
native Bash/Zsh round trips, and
`cargo test --locked --lib --features has-fish fish_paths_` for the required native
Fish check in Linux CI. `SLATE_TEST_FISH` also applies here. The fixture runs actual
configuration-only setup, sources its loader twice in a fresh non-interactive
shell, and checks exact exported paths and fastfetch wrapper argv. Autorun,
Starship initialization, Zsh plugins and the watcher are disabled. Only a private
fastfetch stub runs, after verifying the first PATH entry; no real tool or font
installation occurs. Fish's own initial cache writes get a control baseline.
Without Fish, compile with `--features has-fish --no-run` but do not report native
Fish runtime coverage. UTF-8 literal quoting is not a non-UTF-8 path conversion or
a fix for delimiters interpreted separately by external tool formats.

For focused session checks:

```bash
cargo test --lib session_context
cargo test --test session_behavior
```

For picker file recovery and real keyboard-driven cancel/commit checks:

```
cargo test --lib preview_snapshot
cargo test --locked --offline --profile local-check --test picker_exit -- --test-threads=2
```

The latter uses a private PTY and `SLATE_HOME`; it does not drive your visible
terminal windows. It kills only its own child to check forced-exit recovery,
live-session locking, preservation of later edits, and recovery-menu confirmation.
Keep this subprocess-heavy suite's concurrency bounded: the external prompt
preview has a deliberate short runtime budget and can fall back under heavy
parallel load. Do not increase that production timeout merely to stabilize QA.
Unit tests also check
incomplete-write records and invalid recovery targets. Power-loss behavior is not tested.

For read-only status and recovery discovery:

```
cargo test --test status_readonly
```

These checks compare fixture file contents and modes before/after text and JSON
inspection, including pending, active, conflicting, and malformed recovery state.

The empty `preview-session.lock` file intentionally remains after cleanup. A live
kernel lock, not the presence of this file or a saved PID, determines whether a
preview/recovery or ordinary configuration-writing operation is still active.
Never unlink it while an operation may own it: a new inode would bypass exclusion.

For focused writer exclusion checks:

```
cargo test --lib write_guard_
cargo test --test write_exclusion
```

The CLI test holds a real lock and checks write rejection plus read-only access.
It clears child PATH and uses invalid setup/font targets as extra safeguards
against real process control or package installation if a guard regresses.
The preview tests also verify lock ownership transfer across commit. Advisory
locking coordinates only cooperating Slate operations using the same cache root.

For cleanup and file-restore safety:

```
cargo test --test clean_safety
cargo test --locked --test clean_preview
cargo test --test restore_preview
cargo test --locked --test restore_record_safety
cargo test --locked --test restore_inventory
cargo test --locked --test restore_confirmation
cargo test --locked --lib -- backup::time::tests pre_restore_uses_prepared_targets
cargo test --locked --lib late_restore_change_keeps_checkpoint
```

Cleanup tests use private `SLATE_HOME` trees and a PATH containing harmless process
spies, never real pkill or application control. They cover complete file snapshots,
permission restoration, legacy manifests, redirected targets, backup failure, and
partial-clean recovery receipts. Writer tests seed the permanent empty lock before
their full-tree no-mutation assertions; read-only tests must still work without it.
Do not run clean against your real configuration as a test.

Clean preview tests compare the projected per-file actions with a real isolated
clean, including every Ghostty/tmux candidate, all parser-based integrations,
binary managed files, empty directories and a preserved user-tier link. They then
restore the checkpoint and compare bytes/modes. They also verify no-write full
trees for fresh profiles, a held writer lock, FIFO recovery metadata, malformed
preferences/configs, unsafe paths, bounded sources/target scans, custom HOME/XDG/
ZDOTDIR/NVIM_APPNAME, SSH session policy, parser conflicts and a closed stdout.
Run static completion tests when changing these flags. Preview does not execute
the cleanup functions against a temporary clone: shared pure transforms produce
the file projection directly. Read/parse errors must never leak source contents.

`scan_complete` means target enumeration finished, not that cleanup can run.
File blockers have `action: blocked`; fatal discovery issues clear `scan_complete`.
Both return nonzero, while parser skips are explicit unchanged-file warnings.
No writer, recovery or destination-write readiness check is implied. Keep the
mandatory snapshot and execution-time preflight even after a successful preview;
JSON is not a persisted execution plan. Source limits are shared with snapshots,
and target file/directory limits are shared with formal cleanup. Use
`cargo test --locked --lib clean_preview_display_paths` for lossy/control-path
formatting (APFS cannot create every non-UTF-8 filename used by the pure check).

For narrow include cleanup changes, use
`cargo test --locked --lib -- cli::clean::edits::tests adapter::kitty::tests::test_ensure_ cli::clean::tests::remove_kitty_`
plus `cargo test --locked --test clean_preview --test clean_safety`.
The Alacritty projection uses immutable parser spans, not a rebuilt array: remove
only owned literal values and their following delimiter commas, skipping commas
in comments. Preserve every other byte, including CRLF, empty sections, comments
attached to removed entries, and non-string user values. Test both import locations,
dotted/inline forms, every removal position, escaped paths and repeat no-ops.
Kitty setup and cleanup share literal directive/continuation inspection; never
use substring matching for include paths, option names or socket names. Its
[upstream parser](https://github.com/kovidgoyal/kitty/blob/master/kitty/conf/utils.py)
joins continuation lines before recognizing keys and treats include values as
whole paths, not shell argument lists. Tests include prefix collisions, user socket
names, Unicode indentation and binary surrounding bytes; they do not execute Kitty
or expand arbitrary environment, generated or glob includes. Real clean fixtures
check read-only preview, no-op inode/mtime preservation, ordinary permissions and
exact pre-clean recovery, entirely inside disposable profiles.

For Alacritty application and import-precedence changes, select
`cargo test --locked --test alacritty_integration --test theme_safety --test doctor_readonly`
and `cargo test --locked --test font_names alacritty_font_names_round_trip`.
The narrow lib filters are `adapter::alacritty::integration::tests`,
`adapter::alacritty::tests::integration_`, `adapter::alacritty::tests::invalid_integration_`,
`adapter::alacritty::tests::test_integration_`, `cli::font::tests::font_apply_report_`
and `cli::doctor::integrations::tests`.
Pin fonts in real adapter tests: an isolated home can still discover system fonts.
The no-managed-font branch is covered directly by the prepared-editor unit test.
Preserve root-over-general precedence without migration/merging, keep user import
order, support dotted/inline tables and only remove the overridden font family.
Changing entries still uses TOML AST serialization (unlike the byte-spliced clean
editor); no-op entries must preserve bytes, inode, mtime and ordinary permissions.
The prepared edit validates before managed output and rechecks source bytes,
identity and ordinary permissions before publishing the entry, but does not lock
out external editors or roll back partial managed writes after later failures.
Input and output share an 8 MiB limit. The invalid-input adapter test uses a
deadline-limited subprocess for FIFO/link/oversize cases; the probe alone is a
no-op without its private fixture environment. Doctor CLI regressions cover
effective imports, source-free parse errors and all four integration FIFO reads.
Diagnostics permit ordinary linked reads; writers reject final links. These
checks do not launch Alacritty or prove live GUI reload behavior.

For Alacritty entry-path changes, select
`cargo test --locked --test alacritty_paths --test baseline_backup --test clean_preview`
and the lib filters `alacritty_resolution_`, `setup_reuses_alacritty_`,
`setup_does_not_initialize_`, `preview_snapshot_covers_alacritty_`.
The real coordinator/doctor/recovery fixtures cover each user-level entry with
default and custom XDG roots, inactive lower-priority files, original permissions,
absent higher-priority files and ordinary directory aliases. Keep all candidates
in baseline, clean and live-preview capture; update the snapshot key list/count
alongside any added locations. Resolver aliases are deduplicated by resolved
parent plus filename, without following final file links. Do not infer runtime
--config paths or silently bypass an obstructed candidate. The live-preview check
creates a higher-priority user file between preview steps, verifies the next step
stops, and retains that file while recovering the original alternate. Setup
checks exercise existing alternates and dangling links; its create-new primitive
also prevents clobbering a final entry that appears after inspection. None of
these fixtures modifies system configuration or runs a terminal process.

For OpenCode JSONC changes, select `cargo test --locked --lib opencode` and
`cargo test --locked --test opencode_integration --test clean_preview --test theme_safety`.
The parser disables JSON5 extensions and also validates literal syntax; the token
editor must retain every unrelated byte and every comment, including comments
inside the removed theme property. Keep new text before trailing line comments,
check every root comma position, and reparse edits before publication. Do not use
serde value serialization for editing. Application prepares before adapter backup,
rejects unsafe/oversize sources, and checks bytes, identity and mode again before
writing. The already-system adapter path must leave file identity and backup tree
unchanged. Deadline child probes cover FIFOs, nesting limits, links, bad inputs,
and host override isolation; CLI tests compare clean preview with real cleanup and
file-only recovery, all in temporary profiles. These checks do not launch OpenCode
or prove live rendering/reload behavior. Cleanup's theme=system convention is not
proof that Slate originally authored that setting.

For captured OpenCode path changes, select
`cargo test --locked --test opencode_paths --test opencode_doctor --test opencode_integration --test alacritty_paths`
and the lib filters `opencode_override_`, `opencode_candidates_`,
`opencode_config_evidence_`. The cwd/env mutation probe is a separate child process;
never change cwd or process variables in the parallel parent test runner. Fixtures
exercise existing and absent relative targets, dot/parent syntax, directory aliases,
mandatory baseline and pre-clean checkpoints, and restores after selecting a new
override. Keep final file symlinks unresolved. The shared directory-alias helper is
only an identity hint: ordinary source validation still applies. A bad override is
captured as data so doctor can describe it; writers and ordinary snapshots explicitly
validate it, while exact file-only restores must not depend on the current override.

Version probes and Ghostty validation share `platform::process_output`. Use
`cargo test --locked --lib -- platform::version_check::tests
cli::doctor::ghostty_validation::tests` and
`cargo test --locked --test version_detection` for focused checks. Fake executables
exercise argument/stdin handling, exact combined limits, invalid UTF-8, nonzero
exits, noisy stderr, omitted private diagnostics and rejection of partial version
prefixes. The public version entry is tested against both a sleeping leader and
an exited leader with inherited pipes under a separate seven-second test deadline.
No real editor, font installer or terminal validator is invoked. Production probe
deadlines start after spawn; OS calls and deliberately detached descendants are
not hard-bounded.

Version headers use the existing `semver` crate, with complete three-part numbers,
optional `v`, and preserved prerelease/build identifiers. Only the first non-empty
line is considered, and known tool basenames must match its name. Other executable
paths accept a bare version or `tool [version] v?X.Y.Z`; no later numeric token or
dependency/compiler line can repair a malformed candidate. Unexpected header
control characters and attached invalid suffixes fail without echoing output.
The policy is a SemVer lower bound (`cmp_precedence`, not a `VersionReq` stable-only
filter): floor prereleases fail, higher prereleases can pass, and build metadata
does not change precedence. Thresholds remain unchanged. Unit fixtures follow
[SemVer](https://semver.org/spec/v2.0.0.html), the
[Neovim header](https://github.com/neovim/neovim/blob/master/src/nvim/version.c), and
[Ghostty header](https://github.com/ghostty-org/ghostty/blob/main/src/cli/version.zig);
they do not assert runtime API compatibility for every release or custom build.

For the broader Ghostty diagnostic surface:

```bash
cargo test --locked --test ghostty_doctor
cargo test --locked --lib cli::doctor::tests
cargo test --locked --lib cli::doctor::ghostty_validation::tests
cargo test --locked --lib ghostty_references
cargo test --locked --test integration_tests test_doctor_ghostty
cargo test --locked --test opencode_doctor integration_doctor
```

Keep the existing candidate/duplicate/cycle JSON fields. `scan_complete` is separate
from `cycle_risk`: lack of findings in an incomplete scan is not a healthy result.
Fixtures cover unsafe entry and nested files, missing suffixes under directory aliases
(including macOS /var), linked cycles, isolation escapes, graph/read limits, terminal
controls, lossy paths, pending recovery and closed output pipes. Native validation
tests use only private scripts with explicit arguments and null stdin. A test-only
readiness hook avoids interpreter-startup races when checking short deadlines; verify
leader reaping and inherited-pipe descendant shutdown, not just a timeout message.
The process group belongs only to that invocation; do not use name-based process
termination or signal a group after reaping its leader. Test combined stdout/stderr
limits, the exact boundary, zero/nonzero exits, launch failure and output escaping.
Native output may contain user values even though scan errors omit file contents.
These checks do not validate real Ghostty syntax/rendering, supervise descendants
that deliberately escape the process group, impose filesystem IO deadlines or lock
out hostile concurrent directory changes. Do not launch the host terminal in tests.

For reference-only changes, use `ghostty_references`, `ghostty_doctor`, the parent
doctor unit tests and the two `test_doctor_ghostty` CLI regressions; native process
tests need not be repeated when that code is unchanged. Keep the two parsing stages
aligned with the pinned Ghostty 1.3.1 `LineIterator` and `Path.parse`, rather than
inferring file syntax from the path parser alone. Fixtures cover full literal paths,
BOM, optional/required absence in both orders, unsafe optional files, alias-specific
relative bases, resets and the 4094/4095-byte line boundary. Preserve load spelling
separately from physical cycle identity. Do not assume Linux and macOS realpath
behavior is identical for invalid prefixes followed by `..`; absolute paths must
still be read as supplied, and failed relative resolution must not become a guessed
target. The reference baseline is not a runtime version probe or entry-selection
contract. Cross-file resets remain explicitly unmodeled, not silently declared safe.

For entry-order changes, select the `ghostty_entry_order` and
`ghostty_config_evidence` unit tests, `test_detect_current_font_prefers`,
`test_ghostty_integration_config_path`, `cli::doctor::tests`, the `ghostty_doctor`
integration target, and `test_baseline_snapshots_all_existing_ghostty_candidates`
in `baseline_backup`. The latter checks distinct key/path/byte/mode mappings and
theme/font apply → restore → undo using private profiles only. The CLI matrix
covers all candidate-presence combinations (16 on macOS, 4 elsewhere); the pure
descriptor test covers both platform lists on either host. Keep labels and backup
keys bound to paths, never zipped to a separately ordered key list. The
[1.3.1 default-file loader](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/config/Config.zig)
loads legacy before current in each root; native
[preferred-file checks](https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/config/file_load.zig)
can skip an empty/unreadable App Support current file. Slate's last-existing write
policy is not that full runtime loader; its empty-profile fallback stays XDG.

For OpenCode diagnostics and shared integration report changes, select
`cargo test --locked --test opencode_doctor --test doctor_readonly --test completions`,
`cargo test --locked --lib doctor_integrations`, and
`cargo test --locked --bin slate completion_`.
Reports must select the adapter's file without claiming OpenCode runtime precedence,
read only bounded non-linked sources, and omit private values even on parse errors.
Keep the selected-path and theme-state check codes stable, preserve JSON target/check
fields, and disclose path lossiness and unchecked runtime scope. Test alternate,
custom-XDG, explicit/relative override and isolated paths, active locks and pending
recovery, no process launch, unsafe input deadlines, and closed stdout. APFS rejects
non-UTF-8 filenames: every platform tests lossy path serialization without filesystem
writes; Linux additionally runs the real filename case. Bash completion is exercised
natively; static Zsh/Fish scripts must include the public target list, with native
Fish checks remaining opt-in. These tests never use a real user configuration.

For the shared marker editor, select
`cargo test --locked --lib -- adapter::marker_block::tests apply_activation_choice_a_ build_marker_block_for_init_`
and the two render checks `test_render_delta_config` / `test_render_tmux_block`.
String and byte APIs share one line-range validator: zero markers or exactly one
ordered pair, complete marker lines, and matching Hash/Lua/Vim wrappers. File APIs
reject malformed original/replacement blocks before writing; legacy String-returning
helpers keep malformed inputs unchanged. Replacement padding is excluded so repeated
upserts remain idempotent. Preserve binary user bytes, CRLF, ordinary modes and
unchanged file identity. The clean CLI regressions catch reversed-marker truncation
and comment prefixes accidentally disabling the next user Lua/Vim line, retaining
the pre-clean checkpoint when cleanup fails. This is structural marker validation,
not a full Shell/Lua/Vim parser or automatic repair of old orphan comments.

Record safety tests use short CLI deadlines for FIFO/Unicode regressions and
private corrupted manifests alongside healthy history. They check ID, source
containment, aliases, metadata controls, schema/size limits and non-mutating JSON
previews, plus blocked current targets and rejection before undo creation. Sparse
oversize fixtures are compared by size/mode (not hashed); ordinary fixture contents
are compared byte-for-byte. The prepared-target unit check edits a manifest between
loading the selected point and creating its undo checkpoint; it is not a concurrent
filesystem race test. Keep legacy/default-field and cross-profile restore coverage
when changing this path. Checks do not authenticate user-editable backup contents,
coordinate external editors, or time out an unresponsive filesystem. Listing is
bounded per record, not paginated across an unlimited number of history directories.

Inventory checks exercise real text/JSON list modes with healthy, malformed,
FIFO, linked and manifest-less entries, hidden undo-only history, deterministic
timestamp ties, legacy folders, a live writer lock, pending recovery, malformed
preferences and a closed stdout consumer. They verify exact no-write fixture
trees and parser conflicts before profile initialization. APFS refuses non-UTF-8
filenames at creation: the on-disk case runs on Linux; a pure inventory unit check
covers lossy display/ID handling on both platforms. JSON success is scan success,
not restore readiness; use the existing dry-run plan for current-file comparison.
Select the static completion checks when changing these flags.

Restore confirmation tests use a real private pseudo-terminal with isolated
HOME/SLATE_HOME, an empty child PATH, a seeded writer lock and bounded waits.
They edit a target while the prompt is visible, then verify refusal without
overwriting it or creating an undo point. Positive checks cover default cancel,
binary bytes, ordinary permissions, successful restore and undo. Library cases
replace target/backup contents or inodes, alter permissions or manifest fields,
redirect a parent alias, replace the record directory, and contend with another
writer. Full-tree comparisons ensure rejected plans do not change fixture files;
errors, Debug and JSON must not expose saved/current contents.

Keep the CLI undo/redo round trip, not just the library restore check: theme
regeneration happens after the library returns. The fixture retains a valid tracked
theme alongside binary, hand-edited shell files, then compares all non-directory
profile files after each real confirmation, excluding only recovery storage and
the permanent lock. It also checks default cancellation of an undo, read-only JSON,
and no initialization of an absent Slate config directory. Named-theme CLI restores
have a positive regeneration control. Both kinds run with auto-theme disabled in
private profiles; no host tools are reloaded. The behavior unit matrix includes
legacy pre-clean/pre-restore labels; run it with
`cargo test --locked --lib restore_behavior_keeps_operation_checkpoints_file_only`.
Undo coverage is the selected target set, not all additional regeneration effects.

Use `prepare_restore_with_env`, display its `plan()`, then consume it with
`execute_prepared_restore` when implementing a confirmation UI. `PreparedRestore`
is opaque, non-cloneable and in-memory only; preparation is read-only and retains
up to 64 MiB each of backup/current bytes plus metadata. Revalidation compares
one file pair at a time, not a second full retained plan. On Unix, identities use
device/inode; atomic replacement is conservatively stale even with equal bytes.
Manifest comparison is semantic: comments/formatting alone do not change a plan.
The existing immediate-execution APIs still prepare a fresh plan under the writer
guard. Exported dry-run JSON cannot be replayed as a prepared authorization.

Execution revalidates before and after creating the undo checkpoint. The late-change
unit test mutates a target through an inter-stage test hook and verifies that the
checkpoint is retained but no restoration starts; this is deterministic coverage,
not a proof against concurrent filesystem races. Checks are not an external-editor
transaction, backup authentication, or coverage of post-restore theme regeneration.

For the managed auto-theme runtime:

```
cargo test --lib watcher_
cargo test --lib write_guard_busy_or_pending
cargo test --test auto_theme_doctor
```

Lifecycle tests spawn only their own bounded test processes in private config/cache
roots, including two profiles, duplicate launch, deferred events, stop requests,
and forced-exit/stale-record handling. The ignored fixture is invoked by these
tests; do not run it standalone. On macOS a private helper copy is exercised in
event-only mode (no theme command). Tests never enumerate or signal host watchers.
Linux cross-compilation checks the code paths but does not validate a real portal
or GNOME desktop session. Old process-name-based watchers are not auto-migrated.
Doctor tests compare the complete private fixture tree before/after text and JSON
inspection, including legacy/changed launchers, held locks, exit receipts, invalid
records, links and FIFOs. Log files are never opened and instance tokens are never
included in the report. Simulated control records do not replace the separate
real subprocess lifecycle tests.

For the managed event reader, select `cargo test --locked --lib watcher_event_`.
Private `/bin/sh`/`sleep` fixtures cover backend-specific records, invalid UTF-8,
split/unterminated records, overlong records, idle cancellation and an exited
leader whose child retains stdout. Queue tests flood 100,000 invalidations and
require one pending wakeup, with a terminal error taking priority. Producers must
not block on a full queue: the consumer rereads current appearance rather than
replaying historical values. Native readers wait on stdout and an unnamed private
cancellation socket, not a polling timer. Cleanup holds the unreaped leader's PID
until its owned process group is signalled, then reaps and joins; OS cleanup is
not a hard real-time guarantee. No source stderr or malformed record is echoed.
Native cleanup does not cover the legacy standalone GNOME
`desktop::watch_appearance_changes` implementation; managed Portal ownership is
covered separately below.

On macOS, `cargo test --locked --lib watcher_apply_callback_` exercises the real
background apply callback in bounded private-profile subprocesses. Every adapter
program and `defaults` is stubbed; a Dark-then-Light probe catches accidental
post-commit redetection. Existing pair bytes, absent pair files and retained
editor warnings are checked. It never starts a live event source or watcher.
Keep `.preserving_auto_pair()` and warning reporting on this callback as well as
the CLI automatic path. Compile Linux lib/bin targets separately; these fixtures
do not certify a running Linux Portal/GNOME session.

For Portal watcher ownership, select `cargo test --locked --lib portal_watch_`
and compile Linux lib/bin targets. The managed source owns a cancellation socket
and joins its thread on Drop; the protocol future shares one setup deadline across
connection and subscriptions, explicitly closes its dedicated transport on exit,
and has no idle timeout. Polling/yield points must allow cancellation to win even
while messages remain queued. Keep startup confirmation after subscription and
owner-race checks, before the runtime's ready receipt. Cancellation cannot interrupt
arbitrary synchronous user callbacks or promise hard-bounded OS cleanup.

The private bus fixture uses an unnamed socket pair and handles real Hello,
AddMatch, GetNameOwner and RemoveMatch messages. It does not replace environment
variables, start dbus-daemon, or connect to a real session/system bus. Cases cover
valid/unknown enum values, unrelated settings, forged senders, malformed relevant
signals, denied/missing/stalled startup, owner loss during subscription, disconnect,
idle cancellation, and cancelling an actual unfinished authentication handshake.
Managed-source tests additionally check startup failure cleanup and terminal
failure delivery amid a notification burst. EOF is checked on the fixture peer;
a finished callback alone is not connection-cleanup evidence.

Service lifecycle follows the [D-Bus NameOwnerChanged contract](https://dbus.freedesktop.org/doc/dbus-specification.html#bus-messages-name-owner-changed);
Settings values follow the [SettingChanged contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html#org-freedesktop-portal-settings-settingchanged).
The reserved daemon sender is checked explicitly, and Settings messages are pinned
to the resolved unique owner. On owner loss/replacement, exit with an error and let
the existing launcher start a fresh instance later; no hidden reconnect loop is
implemented. Each stream requests a 16-message queue, not a byte-size cap on D-Bus
frames. The fixture does not certify a production bus's multi-client routing,
policy enforcement, service activation or a real Linux desktop session.

The real tmux reload test is opt-in and requires `tmux` on `PATH`:

```
cargo test --features has-tmux --test tmux_session
```

It uses temporary config roots and explicitly addressed private sockets, and stops
only its own fixture servers. It does not connect to your existing tmux sessions.

For focused Neovim loader checks (requires `nvim` on `PATH`):

```
cargo test --features has-nvim --test nvim_lifecycle --test nvim_custom_paths
```

These run headless with temporary config/cache/data roots. The lifecycle checks
cover handle ownership and queued callbacks with deterministic libuv doubles,
then exercise real atomic replacements, state-file recreation, debounce, and
stop/restart. Each lifecycle subprocess has a ten-second timeout.

For Neovim activation preferences, run `cargo test --locked --test editor_preferences`,
`cargo test --locked --test baseline_backup test_baseline_has_correct_metadata`,
and lib filters `editor_preference`, `config_editor`, `setup_outcome_neovim`.
The CLI test uses a private silent profile and no editor process; it verifies
disable/enable, retained user bytes and shims, read-only doctor/clean previews,
baseline restore and undo of both hook and consent. Unit callbacks prove disabled
or unreadable preferences cannot invoke native activation. Custom XDG/appname
profiles are independent; missing records default to allowed without writing.
The bounded regular-file record lives at the active profile's
`lua/slate/auto-activation.disabled`; malformed/symlink/nonregular records must not
be silently overwritten. The `nvim-auto-activation` snapshot key is stable. Older
snapshots lack this entry; they do not retroactively gain consent recovery. These
tests do not stop an existing editor/watch loop or validate native rendering.

For Neovim availability, use `cargo test --locked --test nvim_detection` and lib
filters `nvim_availability`, `nvim_activation_state_detects_existing_marker`,
`format_nvim_consent_receipt` and `setup_outcome_neovim`. Disposable scripts in
deadline-guarded child profiles verify one native probe per activation stage,
idempotent hooks, disabled preferences, old versions, invalid/nonzero output and
timeout errors before Neovim writes. The adapter test supplies a different HOME
and a home-local executable absent from PATH, and verifies coordinator failures
instead of false missing-tool skips. The filesystem fixture uses Unicode/spaces
on macOS and non-UTF-8 bytes on Linux; a pure callback checks byte-preserving
resolution on both. Neither test launches a real editor
or runs package/font installers. Default `doctor nvim` stays probe-free.

For the opt-in `doctor nvim --check-version` path, use
`cargo test --locked --test nvim_version_doctor`, lib filters `nvim_version_report`
and `adapter::nvim::availability::tests`, plus `cargo test --locked --test
doctor_readonly --test completions` and `cargo test --locked --bin slate completion_`.
Temporary executables cover valid prereleases, below-floor versions, invalid
headers with dependency versions, failed exits, timeouts and output overflow.
The CLI test holds a real writer lock, preserves disabled consent, compares the
full private profile before/after, and keeps its native-call counter outside that
profile. It also verifies no native launch without the flag, target validation,
fallback/Unicode paths and text/JSON parity. Missing and non-UTF-8 display states
have pure report fixtures, avoiding host fallback probes. No real editor runs.
The optional schema-v1 object is absent by default; doctor exit success means
report production, so automation must inspect its status. Native diagnostics are
explicitly not a sandbox for the invoked executable.

For executable discovery, select `cargo test --locked --lib executable_lookup`,
`cargo test --locked --test command_detection --test nvim_detection --test
nvim_version_doctor`, and the existing `test_command_path_with_env_finds_user_local_bin`
lib test. Pure directory fixtures cover non-executable files, directories, FIFOs,
broken/looping links, link-path identity, effective owner permissions and missing
candidate evidence. Public lookup runs in a private child PATH without launching
any candidate, checking PATH/fallback tiers and `batcat` alias handling. The Neovim
doctor fixture places a non-executable `nvim` before its valid home-local fallback.
Its adapter fixture separately checks a permission-approved script whose
interpreter cannot start; no actual editor is involved. Do not use a rejected
`nvim` as the only private candidate in native tests: normal fallback discovery
may correctly find a host installation. Rejected-only states have pure fixtures.
Lookup uses regular-file metadata plus libc `faccessat(X_OK, AT_EACCESS)` (see the
[system-call contract](https://man7.org/linux/man-pages/man2/access.2.html)); it
does not open executable contents or constitute a race-free authorization check.
Legacy libc/filesystem access-check limitations still apply. Cross-compilation
does not replace runtime coverage on the target platform.

For bat apply, select `cargo test --locked --lib bat_`,
`cargo test --locked --lib test_write_tmtheme_files_writes_one_per_theme`, and
`cargo test --locked --test bat_integration`. Private, deadline-guarded child
profiles verify the exact executable before launching it: use a private `bat`
for fallback tests, and an actual-PATH `batcat` to test alias precedence over a
fallback `bat`. A fallback-only `batcat` is unsafe as a fixture because normal
discovery can correctly prefer a host `bat`. No fixture should rebuild a host cache.
The tests check child environment bytes, separate config/assets/cache paths,
cwd changes after capture, Unicode (and Linux raw-byte) directories, idempotence,
missing binaries, invalid asset targets/IDs and preserved current-theme state
after failed or known-unsupported cache builds. Native stderr/stdout must not be
replayed in errors. Unit cache failures use a two-second test budget; production
uses 30 seconds and 256 KiB combined output through the shared owned-process-group
capture. Neither is a sandbox or a hard bound on OS calls/detached processes.

`SlateEnv` captures `BAT_CONFIG_DIR` and `BAT_CACHE_PATH` as native paths; like
upstream, `BAT_CONFIG_PATH` only accepts UTF-8. Empty overrides denote cwd,
relative paths are anchored once, and isolated profiles discard all three
overrides. Import's matching regression is `cargo test --locked --test
import_recovery isolated_import_ignores_external_bat_overrides_and_recovers_local_assets`:
it restores only the private asset files, not the external compiled cache.
Upstream's [directory code](https://raw.githubusercontent.com/sharkdp/bat/master/src/bin/bat/directories.rs)
and [cache subcommand](https://raw.githubusercontent.com/sharkdp/bat/master/src/bin/bat/main.rs)
define the separate paths and the known zero-exit no-build-assets response.
An ordinary successful native exit is not proof of actual theme rendering.

For ordinary-theme checkpoints, use `cargo test --locked --test theme_checkpoint
--test theme_safety --test theme_commit`, plus lib filters `prepared_theme_apply`,
`theme_write_paths`, `theme_checkpoint_picker` and
`restore_behavior_keeps_operation_checkpoints_file_only`. The fixture first
reproduced missing generated Alacritty colors in ordinary snapshots. It now
checks exact binary bytes/modes, original absence, selected-target scope, restore
and undo, file-only CLI preview metadata, and early rejection of oversized,
FIFO and dangling-link sources. The picker unit includes a real private opacity
write after the theme stage and restores both stages from one checkpoint.

`adapter::write_paths` is the shared theme/import contract: update it whenever an
adapter's potential writes change. `PreparedThemeApply` retains one availability
result per selected themeable adapter; tests prove no re-probe between capture
and execution, preserve missing/probe-error results and exclude installer-only
adapters. Native availability checks are not filesystem isolation or runtime
validation. A `pre-theme` record is never a pre-install baseline and never triggers
theme regeneration; old named-theme records still do. Quiet auto-follow and
explicit `SnapshotPolicy::Skip` workflows keep their existing ownership rules.
Generated files and shared state are covered; tool caches, native process effects,
empty directories and unrelated setup hooks are not.

For Neovim notification ordering, select `cargo test --locked --test nvim_commit
--test theme_commit --test nvim_detection` and lib filter `nvim_notification_`.
The private version script checks normal, unsupported and invalid probes; public
coordinator cases cover adapter/shared-shell/current-record failure, Neovim-only
success and notification failure after commit. `SnapshotPolicy::Skip` is used only
in late-write-error fixtures so strict checkpoint validation does not intercept
the injected fault first. Every case asserts one native version call, preserves
unrelated link targets, and launches no actual editor. The quiet/non-quiet CLI
regression now uses a supported private Neovim, so it verifies the formerly early
notification does not change state on shared-shell failure.

Pure adapter spies assert that current theme and shared Shell files already exist
at notification time, and detect duplicate fallback writes/retries with distinctive
bytes and call counters (not unstable filesystem event counts). Ready post-commit
notifications are split only after their potential files were captured. Failed
availability remains in the pre-commit phase; `ThemeNotCommitted` skips retain the
actual prerequisite failure elsewhere in the report. Selected notification errors
are failed adapter results with explicit already-saved context; the unselected
shared hook remains a retained warning. This is ordered publication, not rollback,
and does not prove a live editor received or rendered the change. Direct adapter
and low-level registry calls intentionally retain their immediate semantics.

For post-commit auto pairing, select `cargo test --locked --test auto_theme_apply`
and lib filter `auto_pair_`. The CLI fixtures use private HOME/SLATE_HOME/PATH and
stub every default adapter program; no real editor, bat cache or watcher is run.
On macOS a private `defaults` script returns Dark once and Light on any later
read, asserting only one read during both quiet and ordinary automatic application.
Cross-platform cases assert exact pair bytes/absence, successful theme and editor
publication despite a malformed pairing, content-free warnings, and quiet stderr
visibility after redirection ends. The shared editor failure uses a private final
symlink and quiet auto's existing snapshot-skip policy to reach the late fault.
Pure policy tests inject appearance, check both manual slots, preserve comments
and unrelated keys, and ensure disabled/preserve policies do not probe the desktop.
The pre-commit shell failure case retains the old pair/editor state and recovery
hint, without attempting a desktop read or issuing an already-saved warning.
These checks do not exercise a live appearance watcher or claim transactional
rollback; `reload_warnings` includes nonfatal post-commit preference updates.

For picker commit recovery, select `cargo test --locked --lib cli::set::commit::tests`
and `cargo test --locked --test picker_commit`. The unit seam selects only file-based
Alacritty/ls_colors and injects opacity-stage faults before/after real private file
writes. Compare every checkpoint entry's bytes, ordinary mode and absence, including
custom generated files and auto pairs; verify unrelated cache effects remain and the
undo point restores the pre-recovery partial selection. Missing backup files appear
as inventory issues, not usable points: preserve the bad record and its diagnostic ID.
Blocked targets, wrong-kind/missing checkpoints and synthetic partial/empty receipts
must never produce a successful-recovery claim. The public API subprocess fixtures
shadow all default adapter programs, check exact bat/Neovim resolution, and inject
a post-checkpoint directory fault from the fake bat build. Call counters prove that
automatic recovery never reruns native probes or the theme/cache stage. These are
commit-path tests, not full interactive picker or live-window verification. Preview
cleanup remains separately covered by `picker_exit` and the preview journal tests.

For short appearance queries, select lib filters `appearance_probe_`,
`appearance_resolution_`, `appearance_portal_`, and `test_parse_gnome_color_scheme_output`,
plus `cargo test --locked --test auto_theme_apply`. Private command fixtures exercise
complete values, both recognized macOS missing-key diagnostics, malformed/non-UTF-8
output, nonzero exits, output flooding, inherited pipes and timeout. CLI cases on
macOS assert failed automatic resolution leaves theme/pair/editor files and snapshots
alone, even in quiet mode; manual post-commit query errors retain the theme and warn.
Pure futures verify completion, deadline and cancellation/drop; typed error fixtures
distinguish absent Portal backends from denied or invalid requests without D-Bus I/O.
Linux builds must also compile the async Settings proxy path. These tests do not
validate a live Linux Portal, GNOME desktop or watcher; avoid substituting a real
desktop probe for them. The compatibility infallible platform API still defaults
to Light on errors; mutating CLI paths use `detect_system_appearance_checked`.

Portal enum behavior follows the [Settings specification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html):
1 is dark, 2 is light, and 0/unknown values mean no preference (Slate uses Light).
Only absent/unimplemented service/settings errors allow missing-backend fallback.
Settings reads use `ReadOne` with property caching disabled and an async deadline;
only its exact unknown-method error triggers one deprecated `Read` fallback.
The same deadline includes connection and both requests. `ReadOne` must contain
a uint32; `Read` must contain precisely one additional variant around that uint32.
Do not guess types or retry legacy access after denied/malformed/timed-out reads.
This is not a D-Bus response-size cap or a hard bound on synchronous OS work.
The direct async-io/futures-lite dependencies were already present transitively in
the lockfile; do not implement a timeout by abandoning a blocking worker thread.

For Portal protocol compatibility, run `cargo test --locked --lib portal_settings_`
and compile Linux lib/bin targets. These tests use zbus p2p over unnamed, private
Unix socket pairs; only the dev dependency enables p2p, with no new daemon, bus
environment override, filesystem socket or desktop service. The production query
future and proxy are shared with the fixture. A v1-only service truly lacks
`ReadOne`, and the v2 fixture records reads/properties to catch unintended version,
GetAll or legacy queries. Settings and Screenshot proxies explicitly name their
lowercase `version` property as required by their specifications (including the
[Screenshot specification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html)).
Fixtures expose that spelling independently and reject the incorrect `Version`
alias; this version-only Screenshot fixture has no screenshot method (the separate
`portal_screenshot_` fixtures above exercise the request/response lifecycle).
Wire replies cover valid/no-preference/unknown enum values,
extra variant layers, wrong types, denied requests and unavailable methods. Timed
services verify a shared fallback deadline and no fallback after timeout; owned
transports close even on assertion unwind. This is D-Bus protocol coverage, not
certification against a running Linux desktop, a session bus's routing/Hello
handshake or the long-running Settings signal watcher.

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
