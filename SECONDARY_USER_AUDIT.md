# Complete Secondary User Scenario Audit

## Scenario Context
Secondary user (testuser) on shared macOS machine with:
- Access to: /opt/homebrew/bin tools (shared), /Applications/Ghostty.app (shared)
- No access to: /opt/homebrew (permission denied), primary user's ~/.config/*, ~/Library/Fonts/*
- Network: May be unreliable

---

## CRITICAL ISSUES FOUND

### Issue 1: Empty Config Files Prevent Theme Application (BLOCKING)
**Location**: `src/cli/setup_executor.rs:107` in `ensure_tool_configs()`

**Problem**: 
The function creates empty config files for detected tools, but later adapters SKIP if files exist but are empty. This creates a catch-22:
- `ensure_tool_configs()` creates empty files: `ghostty/config.ghostty`, `starship.toml`, `bat/config`
- Adapters check `if !config_path.exists()` ONLY (line 114 in `src/adapter/ghostty.rs`, line 114 in `src/adapter/starship.rs`)
- If the file exists but is EMPTY, adapters proceed to parse/write
- EMPTY files cause TOML parse failures or create minimal invalid configs

**Secondary User Impact**:
When testuser runs `slate setup`, the created empty files are valid in `ensure_tool_configs()`, but then:
1. Ghostty adapter creates `theme.conf` successfully (write doesn't require valid config at integration point)
2. Starship adapter tries to parse EMPTY `starship.toml` as valid TOML and creates minimal config
3. Later `slate set` runs find empty files and try to parse them, potentially failing

**Root Cause**: 
The logic assumes "if file exists → user has configured it" but setup explicitly creates empty files for secondary users who have never configured anything.

**Fix**:
In `src/cli/setup_executor.rs:192-221`, either:
1. Don't create empty files; let adapters handle missing files gracefully during setup, OR
2. Create files with minimal valid content (empty TOML table, empty comments), OR
3. Mark these files as "slate-managed-empty" and have adapters skip them

**Recommendation**: Option 2 — create files with a comment marker like:
```toml
# Slate configuration — managed by ~/.config/slate/managed/{tool}/
```

---

### Issue 2: Starship Adapter Fails on Empty File Parse
**Location**: `src/adapter/starship.rs:112-127`

**Problem**:
When `starship.toml` is empty (0 bytes):
1. Line 123: `fs::read_to_string(&config_path)` reads empty string `""`
2. Line 127: `content.parse()` calls `"".parse::<DocumentMut>()`
3. Empty TOML IS valid (DocumentMut parses to empty table), so this succeeds
4. BUT line 130: `doc["palette"] = toml_edit::value("slate")` on empty doc creates minimal structure
5. If later the file had user config, that minimal structure overwrites it

**Secondary User Impact**:
First `slate setup` creates empty `starship.toml`. When adapters apply, Starship gets:
```toml
palette = "slate"

[palettes]
[palettes.slate]
# ...full palette data
```

But the file had no prior `format` or other Starship config, so this is correct for fresh setup. However, **no idempotency guarantee** — if file already has content, minimal structure risk exists.

**Fix**: 
Add explicit empty-file handling in Starship adapter:
```rust
// Check if file is effectively empty (0 bytes or whitespace-only)
if content.trim().is_empty() {
    // For fresh setup: create minimal valid config
    let mut doc = DocumentMut::new();
    // ... proceed to populate
} else {
    // For existing config: parse carefully
    let mut doc = content.parse()?;
    // ... proceed
}
```

---

### Issue 3: Ghostty Config Creation Returns `Ok()` Even When File Doesn't Exist
**Location**: `src/adapter/ghostty.rs:69-133` in `ensure_integration_includes_managed()`

**Problem**:
Lines 73-77: If integration file doesn't exist, function returns `Ok(())` silently:
```rust
if !integration_path.exists() {
    return Ok(());  // ← SILENTLY SUCCEEDS
}
```

During setup:
1. `ensure_tool_configs()` creates EMPTY `~/.config/ghostty/config.ghostty`
2. Ghostty adapter's `apply_theme_with_env()` writes to managed files successfully
3. BUT then tries to call `ensure_integration_includes_managed(&integration_path, ...)` on the file
4. File exists (was created empty), so the include check runs
5. **BUT**: If file is empty, the logic to append the `config-file = "..."` line depends on file content parsing

**Secondary User Impact**:
Secondary user gets `~/.config/ghostty/config.ghostty` created empty (0 bytes). When `ensure_integration_includes_managed()` runs:
1. File exists ✓
2. Line 80: `fs::read_to_string(integration_path)` reads empty string
3. Lines 84-107: Iteration over empty lines finds no `config-file` or `include` directives
4. Line 124: Appends `config-file = "..."` to empty content
5. Writes: `config-file = "..."\n`

**Problem**: This is CORRECT behavior, but Ghostty will see a config file with ONLY the include line and no other settings. Ghostty's default settings should handle this, but it's a minimal config.

**Root Cause**: No distinction between "file doesn't exist yet" vs. "file exists but is empty".

**Fix**: Create files with minimal valid Ghostty config:
```
# Slate-managed Ghostty configuration
# User overrides: add settings before the include
```

---

### Issue 4: Theme Apply Fails If applied_count == 0 (NO RECOVERY)
**Location**: `src/cli/theme_apply.rs:52-74`

**Problem**:
In `ThemeApplyCoordinator::apply()`:
```rust
if report.applied_count() == 0 {
    return Err(self.no_success_error(&report));  // ← FATAL
}
```

The function ABORTS if NO adapters succeeded. For secondary user:
1. All tools are installed in /opt/homebrew (shared, readable)
2. But adapters SKIP because integration files don't exist
3. Each adapter returns `SkipReason::MissingIntegrationConfig`
4. `applied_count == 0`
5. Setup FAILS with error instead of proceeding

**Secondary User Impact**:
During `slate setup`:
1. `ensure_tool_configs()` creates files ✓
2. Theme is applied via `theme_apply::apply_theme_selection_with_env()`
3. Adapters detect tools installed in /opt/homebrew ✓
4. But each adapter tries to apply and finds integration file DOESN'T exist yet (created empty but AFTER apply runs? timing?)
5. `applied_count == 0` → FATAL ERROR
6. Setup ABORTS before shell integration is written

**Root Cause**: The timing of `ensure_tool_configs()` vs. `apply_theme_selection_with_env()` and the order of checks in `apply_theme()`.

**Critical Detail**: In `src/cli/setup_executor.rs`:
- Line 107: `ensure_tool_configs(env)` creates empty files
- Line 111: `setup_shell_integration_with_env(theme, env)` calls `theme_apply::apply_theme_selection_with_env()`

So files ARE created before theme apply. But **adapters check `if !config_path.exists()`** which will be true since files were just created. So why would they skip?

**Correction**: Let me re-examine the adapter `apply_theme()` logic:
- Ghostty line 266-269: `apply_theme_with_env()` checks `if !integration_path.exists()` 
- But `integration_config_path_with_env()` at line 356-368 returns a DEFAULT path even if it doesn't exist!
- So `apply_theme_with_env()` will find the default path does NOT exist (because `ensure_tool_configs()` created it but adapter checks a DIFFERENT path?)

**Actual Root Cause**: 
`integration_config_path_with_env()` returns FIRST EXISTING path OR the upstream default. But `ensure_tool_configs()` creates the default path. So the adapter SHOULD find the file exists.

Let me trace this more carefully:
1. `ensure_tool_configs()` line 210: calls `touch(&xdg.join("ghostty/config.ghostty"))`
2. This creates: `~/.config/ghostty/config.ghostty` (empty)
3. Later, `GhosttyAdapter::integration_config_path_with_env()` line 356-368:
   - Builds `candidate_paths()` which includes `xdg.join("config.ghostty")`
   - Calls `first_existing_path()` which returns the first path in candidates that exists
   - IF the file exists, it returns that path
   - IF no file exists, it returns the DEFAULT which is `xdg.join("config.ghostty")`
4. So `apply_theme_with_env()` line 266 will find the path exists (or equals the default)
5. Then line 267: `if !integration_path.exists()` checks if it ACTUALLY exists on disk

**WAIT** — there's a logical issue here:
- If no files exist, `first_existing_path()` returns `None`
- Then adapter returns the DEFAULT path (line 367)
- That DEFAULT path probably DOESN'T exist on disk
- So `if !integration_path.exists()` is true
- Adapter returns `Skipped(MissingIntegrationConfig)`
- `applied_count` stays 0

**So the actual problem is**: `integration_config_path_with_env()` returns a path that MAY NOT EXIST, and `apply_theme()` checks existence after the fact, causing skips.

**Secondary User Impact**: 
CONFIRMED: For secondary user, `ensure_tool_configs()` creates files, BUT:
- If Ghostty adapter's `integration_config_path_with_env()` is called BEFORE files are created, it returns a DEFAULT path
- That default path doesn't exist yet
- When `apply_theme()` runs later, it checks `if !integration_path.exists()` on the stale path
- Adapter skips

OR: Files ARE created, but the adapter gets called with a different path resolution.

**FIX**: Ensure files are created BEFORE adapters are instantiated, OR change skip logic to be more lenient.

---

### Issue 5: Apply Theme Fails Silently with `applied_count == 0` During Setup
**Location**: `src/cli/theme_apply.rs:59-60` in `ThemeApplyCoordinator::apply()`

**Problem**:
```rust
if report.applied_count() == 0 {
    return Err(self.no_success_error(&report));
}
```

This is a FATAL error that aborts the entire setup. For secondary user who has all tools installed (in /opt/homebrew), if ALL adapters skip due to missing integration files, setup FAILS.

**Secondary User Impact**:
`slate setup` runs:
1. Wizard completes, user confirms
2. Tools install successfully (via brew install in /opt/homebrew)
3. Font install succeeds
4. `ensure_tool_configs()` creates empty files
5. `setup_shell_integration_with_env()` calls `theme_apply::apply_theme_selection_with_env()`
6. Adapter registry tries to apply theme
7. ALL adapters skip: `SkipReason::MissingIntegrationConfig`
8. `applied_count == 0`
9. **Setup ABORTS** with error message

BUT: `ensure_tool_configs()` already created the files! So why would adapters skip?

**Root Cause Deep Dive**:
Looking at `src/cli/setup_executor.rs:192-221` carefully:
```rust
fn ensure_tool_configs(env: &SlateEnv) {
    let installed = detect_installed_tools();
    if installed.get("ghostty").copied().unwrap_or(false) {
        touch(&xdg.join("ghostty/config.ghostty"));
    }
}
```

This creates files IF tool is detected as installed. But `detect_installed_tools()` is called **without checking if the binary actually exists in PATH**. It checks various detection heuristics.

So: If Ghostty is in /Applications (secondary user can see it), it's detected as installed. File is created.

Then when adapter runs:
- `GhosttyAdapter::is_installed()` line 218: checks `which::which("ghostty")` — FAILS for secondary user (no write perms to /opt/homebrew)
- OR `integration_config_path()` is called and returns a path
- Then `apply_theme_with_env()` checks `if !integration_path.exists()` — should be TRUE since file was created

**WAIT**: I need to check the ACTUAL execution order in `setup_executor.rs`:
```rust
// Line 107: ensure_tool_configs(env);
// Line 111: setup_shell_integration_with_env(theme, env);
```

So files ARE created before theme apply. Then inside `setup_shell_integration_with_env()`:
```rust
// Line 161
fn setup_shell_integration_with_env(theme: Option<&str>, env: &SlateEnv) -> Result<ThemeVariant> {
    marker_block::upsert_managed_block_file(&zshrc_path, &marker_content)?;
    
    let selected_theme = resolve_selected_theme(theme, env)?;
    theme_apply::apply_theme_selection_with_env(&selected_theme, env)?;  // ← HERE
    
    Ok(selected_theme)
}
```

Inside `apply_theme_selection_with_env()`:
```rust
let report = ThemeApplyCoordinator::new(env).apply(theme)?;
```

Inside `apply()`:
```rust
let registry = ToolRegistry::default();
let results = registry.apply_theme_to_all(theme);
let report = ThemeApplyReport { results };

if report.applied_count() == 0 {
    return Err(self.no_success_error(&report));  // ← FATAL
}
```

Inside `apply_theme_to_all()` in `src/adapter/registry.rs:70-89`:
```rust
self.adapters
    .par_iter()
    .filter(|adapter| adapter.apply_strategy() != ApplyStrategy::DetectAndInstall)
    .map(|adapter| {
        let tool_name = adapter.tool_name().to_string();
        let status = match adapter.is_installed() {  // ← CHECK 1
            Ok(false) => ToolApplyStatus::Skipped(SkipReason::NotInstalled),
            Ok(true) => match adapter.apply_theme(theme) {  // ← CHECK 2
                Ok(ApplyOutcome::Applied) => ToolApplyStatus::Applied,
                Ok(ApplyOutcome::Skipped(reason)) => ToolApplyStatus::Skipped(reason),
                Err(err) => ToolApplyStatus::Failed(err),
            },
            Err(err) => ToolApplyStatus::Failed(err),
        };
        ToolApplyResult { tool_name, status }
    })
    .collect()
```

So for each adapter:
1. **CHECK 1**: `adapter.is_installed()` — For secondary user, this might return FALSE if tool detection fails
2. **CHECK 2**: If installed, call `adapter.apply_theme(theme)`

For Ghostty in secondary user scenario:
- `/Applications/Ghostty.app` exists ✓
- `/opt/homebrew/bin/ghostty` exists ✓ (symlink from Caskroom)
- `which::which("ghostty")` should succeed
- But `is_installed()` also checks config: `integration_config_path()` and `integration_config_path().exists()`

In `src/adapter/ghostty.rs:218-227`:
```rust
fn is_installed(&self) -> Result<bool> {
    let binary_exists = which::which("ghostty").is_ok();
    
    let config_exists = match self.integration_config_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    };
    
    Ok(binary_exists || config_exists)
}
```

So:
- `binary_exists = true` (ghostty is in PATH)
- `config_exists`: calls `integration_config_path()` which checks candidates and returns default
- If file was created by `ensure_tool_configs()`, this should be `true`

So `is_installed()` should return `true`.

Then in `apply_theme()` line 266-269 of `ghostty.rs`:
```rust
fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
    let integration_path = self.integration_config_path_with_env(env)?;
    if !integration_path.exists() {
        return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
    }
    // ...proceed to apply
}
```

If file was created, `integration_path.exists()` should be true, and apply should proceed.

**So why would applied_count be 0?**

**Possibility 1**: `ensure_tool_configs()` is detecting FEWER tools than the adapter registry. Let me check:

In `src/cli/setup_executor.rs:196`:
```rust
let installed = detect_installed_tools();
```

This calls the wizard's `detect_installed_tools()` from `src/cli/tool_selection.rs`.

But adapters use their own `is_installed()` method from `ToolAdapter` trait.

These might DISAGREE on which tools are installed!

For example:
- Wizard detection might check: "is starship in /opt/homebrew?" → TRUE
- Adapter.is_installed() might check: "is binary in PATH AND does integration config exist?" → Might be FALSE if PATH is different

**Root Cause Found**: Tool detection inconsistency between `detect_installed_tools()` (used by setup wizard) and adapter-specific `is_installed()` methods.

**Secondary User Impact**:
- Setup wizard detects Ghostty installed → creates empty config
- But adapter's `is_installed()` checks both binary and config separately
- If adapter uses a different PATH resolution, it might not find the binary
- Returns `Skipped(NotInstalled)` instead of applying

**FIX**: Either:
1. Use the same detection logic everywhere, OR
2. Make adapters not skip on missing config if binary is found, OR
3. Have `ensure_tool_configs()` run AFTER tools are verified installed, not before

---

### Issue 6: Font Installation Might Fail with Permission Denied on Secondary User
**Location**: `src/cli/setup_executor.rs:285-321` in `copy_font_from_caskroom()`

**Problem**:
Lines 310-319:
```rust
let home = std::env::var("HOME")?;
let font_target = std::path::PathBuf::from(home).join("Library/Fonts");
fs::create_dir_all(&font_target)?;

for src in &ttf_files {
    if let Some(filename) = src.file_name() {
        let dest = font_target.join(filename);
        fs::copy(src, &dest)?;  // ← Can fail if copy fails mid-way
    }
}
```

If copy fails for ANY font file, the function returns error and font installation FAILS.

**Secondary User Impact**:
Secondary user's ~/Library/Fonts might have permission issues or might be full. If copy fails:
- Font installation aborts
- Summary marks `font_applied = false`
- Setup continues but completion message reports font failure

**Root Cause**: No error recovery for partial font copy failures.

**FIX**: Track which fonts succeeded, skip failed ones, return partial success:
```rust
let mut copy_count = 0;
for src in &ttf_files {
    if let Some(filename) = src.file_name() {
        let dest = font_target.join(filename);
        if fs::copy(src, &dest).is_ok() {
            copy_count += 1;
        }
        // Continue even if one fails
    }
}

if copy_count == 0 {
    return Err(...);  // All failed
} else {
    Ok(())  // Partial success
}
```

---

### Issue 7: Shell Integration Missing Command Checks for Critical Tools
**Location**: `src/config/shell_integration.rs:45-79`

**Problem**:
Lines 76-79:
```rust
// Initialize starship prompt if available
content.push_str("\nif command -v starship &> /dev/null; then\n");
content.push_str("  eval \"$(starship init zsh)\"\n");
content.push_str("fi\n");
```

This is correct, BUT the earlier content (lines 18-48) assumes tools are available WITHOUT checks:

```rust
content.push_str(&format!(
    "export BAT_THEME=\"{}\"\n",
    theme.tool_refs.get("bat")...
));
```

If bat is NOT installed, exporting `BAT_THEME` is harmless but unnecessary.

**Secondary User Impact**:
For secondary user who selects only a SUBSET of tools:
- Setup creates env.zsh with exports for ALL tools (bat, eza, lazygit, fastfetch, etc.)
- But secondary user only installed Ghostty and Starship
- Shell sources env.zsh with exports for non-existent tools
- No harm (they're just env vars), but cleanup would be better

**Root Cause**: Shell integration doesn't filter exports based on actually installed tools.

**FIX**: Filter exports based on tools selected during setup or tools actually installed:
```rust
if let Some(bat_theme) = theme.tool_refs.get("bat") {
    content.push_str(&format!("export BAT_THEME=\"{}\"\n", bat_theme));
}
```

---

### Issue 8: No Validation of write_shell_integration_file Success
**Location**: `src/cli/setup_executor.rs:110-123`

**Problem**:
```rust
spinner.start("Setting up shell integration...");
match setup_shell_integration_with_env(theme, env) {
    Ok(selected_theme) => {
        summary.theme_applied = true;
        spinner.stop(format!(
            "✓ Shell integration configured for {}",
            selected_theme.name
        ));
    }
    Err(e) => {
        spinner.error(format!("✗ Shell integration setup failed: {}", e));
        return Err(e);  // ← ABORTS SETUP
    }
}
```

If `write_shell_integration_file()` fails (e.g., permission denied on ~/.config), setup ABORTS.

For secondary user:
- ~/.config/slate should be writable (user's home)
- But if permissions are weird, write fails
- Setup terminates without partial credit

**Root Cause**: No fallback for shell integration failure.

**FIX**: Mark as partially applied, continue:
```rust
Err(e) => {
    spinner.error(format!("⚠ Shell integration setup had issues: {}", e));
    // Don't abort; mark as having issues but continue
    eprintln!("(Note: shell integration may need manual setup)");
}
```

---

### Issue 9: Completion Message Doesn't Account for applied_count == 0 Case
**Location**: `src/cli/failure_handler.rs:74-131` in `format_completion_message()`

**Problem**:
Lines 82-85:
```rust
if !self.overall_success {
    output.push_str("✦ Setup finished with issues.\n\n");
} else {
    output.push_str("✦ Setup Complete!\n\n");
}
```

But `overall_success` is set in `setup_executor.rs:127-128`:
```rust
let font_ok = font.is_none() || summary.font_applied;
summary.overall_success =
    (summary.failure_count() == 0 || summary.success_count() > 0) && font_ok;
```

The logic is: success if (no failures OR at least one success) AND font is ok.

**But**: What if:
- ALL tools are already installed (skipped)
- Font is not requested (skipped)
- Theme apply skips all adapters (applied_count == 0)
- `summary.tool_results` is EMPTY
- `success_count()` returns 0
- `failure_count()` returns 0
- `overall_success = (0 == 0 || 0 > 0) && true = true && true = TRUE`

So empty summary would incorrectly report success!

**Secondary User Impact**:
If all tools are pre-installed in /opt/homebrew and secondary user only runs setup to configure them, setup might skip all tool installations, leading to confusing success message even though adapters skipped applying themes.

**Root Cause**: `overall_success` logic doesn't account for skips.

**FIX**: Change logic to require at least one applied or success:
```rust
summary.overall_success =
    (summary.success_count() > 0 || summary.applied_themes_count > 0) && font_ok;
```

---

### Issue 10: ensure_tool_configs() Doesn't Verify Write Success
**Location**: `src/cli/setup_executor.rs:197-204`

**Problem**:
The `touch` function silently ignores errors:
```rust
let touch = |path: &Path| {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);  // ← Ignores errors
        }
        let _ = fs::File::create(path);  // ← Ignores errors
    }
};
```

If create_dir_all fails (e.g., /opt/homebrew permission denied), it's silently ignored.
If File::create fails, it's silently ignored.

For secondary user with permission issues, config files might not be created, but setup continues.

**Secondary User Impact**:
- `ensure_tool_configs()` tries to create ~/.config/ghostty/config.ghostty
- If ~/.config is read-only (weird edge case), creation fails silently
- Adapters later find file doesn't exist and skip
- applied_count == 0
- Setup FAILS (fatal error in theme_apply.rs:59)

**Root Cause**: No error reporting for file creation failures.

**FIX**: Return a Result and report errors:
```rust
fn ensure_tool_configs(env: &SlateEnv) -> Result<()> {
    let touch = |path: &Path| -> Result<()> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::File::create(path)?;
        }
        Ok(())
    };
    
    // Handle errors
    for config_path in [...] {
        touch(&config_path).ok();  // Soft fail, continue
    }
    Ok(())
}
```

---

### Issue 11: Delta Adapter Assumes .gitconfig Exists
**Location**: `src/adapter/delta.rs:91-100`

**Problem**:
The `apply_theme()` method doesn't check if ~/.gitconfig exists. It goes straight to marker block insertion:
```rust
pub fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
    // No check for integration_config_path().exists()
    
    // Validate theme...
    let delta_colors = Self::render_delta_colors(theme);
    
    let config_mgr = ConfigManager::new()?;
    config_mgr.write_managed_file("delta", "colors", &delta_colors)?;
    
    let integration_path = Self::gitconfig_path()?;
    marker_block::upsert_managed_block_file(&integration_path, &marker_content)?;
    // ← Will fail if .gitconfig doesn't exist
}
```

Unlike Ghostty/Starship adapters, delta doesn't skip if .gitconfig doesn't exist. It tries to insert marker blocks.

**Secondary User Impact**:
If secondary user has never used git on this machine, ~/.gitconfig doesn't exist. When delta adapter tries to insert marker block, `upsert_managed_block_file()` might fail or create a minimal .gitconfig.

**Root Cause**: Delta adapter doesn't skip on missing integration config, while other adapters do.

**FIX**: Add missing file check:
```rust
pub fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
    let integration_path = Self::gitconfig_path()?;
    if !integration_path.exists() {
        return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
    }
    // ...rest of apply logic
}
```

---

### Issue 12: Alacritty Adapter Not Examined (Potential Gap)
**Location**: `src/adapter/alacritty.rs`

**Problem**:
Haven't fully reviewed alacritty adapter in this audit. It might have similar issues to Ghostty or Starship.

**Recommendation**: Check:
1. Does `is_installed()` work for secondary user?
2. Does `apply_theme()` handle missing config files?
3. Does it skip properly or abort?

---

### Issue 13: Preflight Check Might Mislead About Tool Availability
**Location**: `src/cli/preflight.rs:95-104`

**Problem**:
The "Tools" check reports:
```
Optional: Tools — 8 of 11 tools already installed
```

But this detection might not match what actually happens during setup. For example:
- Preflight detects Ghostty via `/Applications/Ghostty.app` ✓
- Setup detects Ghostty same way ✓
- But adapter detects Ghostty differently (e.g., via binary in PATH)
- Adapter returns Skipped instead of Applied

**Secondary User Impact**:
User sees "8 of 11 tools installed" in preflight and thinks setup will configure them all. But if adapter detection differs, some might be skipped.

**Root Cause**: Detection inconsistency between preflight and adapters.

**FIX**: Use the same detection method everywhere or document the difference.

---

## Summary Table

| Issue # | File | Line | Severity | Secondary User Impact | Fix Complexity |
|---------|------|------|----------|----------------------|-----------------|
| 1 | setup_executor.rs | 107-221 | **CRITICAL** | Empty files prevent theme apply | Medium |
| 2 | starship.rs | 112-127 | Medium | Empty TOML parsing edge case | Low |
| 3 | ghostty.rs | 69-133 | Medium | Silent success on missing file | Low |
| 4 | ghostty.rs | 356-368 | Medium | Path resolution ambiguity | Low |
| 5 | theme_apply.rs | 59-60 | **CRITICAL** | Setup aborts if no adapters apply | High |
| 6 | setup_executor.rs | 285-321 | Medium | Font copy failures abort setup | Low |
| 7 | shell_integration.rs | 45-79 | Low | Unnecessary exports clutter env.zsh | Low |
| 8 | setup_executor.rs | 110-123 | Medium | Shell integration failure aborts setup | Low |
| 9 | failure_handler.rs | 74-131 | Low | Misleading success message | Low |
| 10 | setup_executor.rs | 197-204 | Medium | Silent file creation failures | Low |
| 11 | delta.rs | 91-100 | Medium | No missing file check | Low |
| 12 | alacritty.rs | TBD | Unknown | Unchecked potential gaps | TBD |
| 13 | preflight.rs | 95-104 | Low | Detection inconsistency | Low |

---

## Recommended Fix Priority

** (Must Fix - Blocking)**: 
- Issue 1: Empty config file handling
- Issue 5: applied_count == 0 fatal error

** (Should Fix - Data Integrity)**:
- Issue 6: Font copy error recovery
- Issue 10: File creation error reporting
- Issue 11: Delta missing config check

** (Nice to Have - UX/Clarity)**:
- Issue 7: Filter exports by installed tools
- Issue 9: Accurate success reporting
- Issue 13: Consistent detection methods

---

## Testing Recommendations

After fixes, test secondary user scenario:
```bash
# Primary user setup:
slate setup --quick

# Switch to secondary user:
su - testuser
cd /Users/maokaiyue/Projects/slate
cargo build
./target/debug/slate setup --quick

# Expected: Setup completes successfully with proper configuration
# Check: ~/.config/ghostty/config.ghostty exists and has proper content
# Check: ~/.config/starship.toml exists and has proper content
# Check: ~/.config/slate/managed/* populated correctly
# Check: ~/.zshrc has marker block for slate
# Check: New shell session sources env.zsh properly
```
