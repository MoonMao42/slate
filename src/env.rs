use crate::error::Result;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

mod bat;
mod lazygit;
mod opencode;
mod shell_startup;
mod zellij;

/// SlateEnv encapsulates environment paths for config and home directory.
/// This abstraction enables:
/// - Dependency injection: all path resolution goes through SlateEnv
/// - Test isolation: tests can inject a tempdir via with_home()
/// - Single source of truth: all adapters and config code use SlateEnv methods
/// `Clone` is derived so `RollbackGuard` (and the companion
/// `install_rollback_panic_hook` closure) can own an env snapshot
/// independent of the caller's borrow. All fields are owned `PathBuf`s,
/// so cloning is O(paths) with no shared state — safe regardless of call
/// site.
#[derive(Clone)]
pub struct SlateEnv {
    home: PathBuf,
    xdg_config_home: PathBuf,
    xdg_cache_home: PathBuf,
    xdg_data_home: PathBuf,
    xdg_data_home_overridden: bool,
    slate_config_dir: PathBuf,
    slate_cache_dir: PathBuf,
    zsh_config_home: PathBuf,
    nvim_config_dir: PathBuf,
    bat_paths: bat::BatPaths,
    lazygit_paths: lazygit::Paths,
    opencode_tui_config: Option<opencode::TuiConfig>,
    starship_config_override: Option<PathBuf>,
    yazi_config_home: PathBuf,
    eza_config_home: PathBuf,
    eza_color_values: [Option<std::ffi::OsString>; 2],
    zellij_paths: zellij::Paths,
    session: crate::session::SessionContext,
}

impl SlateEnv {
    pub(crate) fn zellij_paths(&self) -> Result<(&Path, &Path)> {
        self.zellij_paths.resolve()
    }

    pub(crate) fn verify_zellij_destinations(&self, paths: [(&Path, &Path); 2]) -> Result<()> {
        self.zellij_paths.verify_destinations(paths)
    }

    /// Initialize from process environment
    /// Reads HOME and XDG config/cache/data homes from std::env.
    /// Prefers $XDG_CONFIG_HOME if set, otherwise uses $HOME/.config.
    /// Prefers $XDG_CACHE_HOME if set, otherwise uses $HOME/.cache.
    /// Prefers absolute $XDG_DATA_HOME, otherwise uses $HOME/.local/share.
    pub fn from_process() -> Result<Self> {
        Self::from_vars(|name| std::env::var_os(name))
    }

    /// Resolve environment variables without changing the process.
    /// `SLATE_HOME` isolates all paths, including tool-specific overrides.
    pub fn from_vars(vars: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        // SLATE_HOME overrides HOME for full isolation (used by integration tests).
        // When SLATE_HOME is set, we also force XDG config/cache/data homes to land
        // inside SLATE_HOME so a host-level XDG override can't leak tests outside the
        // sandbox. Without this, GHA Ubuntu runners (which set XDG_CONFIG_HOME) had slate
        // write to /home/runner/.config/slate while tests looked in the tempdir.
        let nonempty = |name| vars(name).filter(|value| !value.is_empty());
        let slate_home_override = nonempty("SLATE_HOME").map(PathBuf::from);
        let home = slate_home_override
            .clone()
            .or_else(|| nonempty("HOME").map(PathBuf::from))
            .ok_or_else(|| crate::error::SlateError::Internal("HOME not set".to_string()))?;

        let xdg_config_home = if slate_home_override.is_some() {
            home.join(".config")
        } else {
            nonempty("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".config"))
        };

        let xdg_cache_home = if slate_home_override.is_some() {
            home.join(".cache")
        } else {
            nonempty("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".cache"))
        };

        let data_override = if slate_home_override.is_some() {
            None
        } else {
            nonempty("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let xdg_data_home_overridden = data_override.is_some();
        let xdg_data_home = data_override.unwrap_or_else(|| home.join(".local/share"));

        let slate_config_dir = xdg_config_home.join("slate");
        let slate_cache_dir = xdg_cache_home.join("slate");

        let zsh_config_home = if slate_home_override.is_some() {
            home.clone()
        } else {
            nonempty("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.clone())
        };
        let nvim_appname = if slate_home_override.is_some() {
            PathBuf::from("nvim")
        } else {
            nonempty("NVIM_APPNAME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("nvim"))
        };
        if !nvim_appname
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(crate::error::SlateError::InvalidConfig(
                "NVIM_APPNAME must be a relative directory name without '.' or '..' components"
                    .into(),
            ));
        }
        let nvim_config_dir = xdg_config_home.join(nvim_appname);
        let opencode_tui_config = opencode::TuiConfig::capture(
            nonempty("OPENCODE_TUI_CONFIG"),
            slate_home_override.is_some(),
        );
        let bat_paths = bat::BatPaths::capture(
            &vars,
            slate_home_override.is_some(),
            &xdg_config_home,
            &xdg_cache_home,
        );
        let yazi_config_home = if slate_home_override.is_some() {
            xdg_config_home.join("yazi")
        } else {
            nonempty("YAZI_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| xdg_config_home.join("yazi"))
        };

        let eza_config_home = if slate_home_override.is_some() {
            xdg_config_home.join("eza")
        } else {
            // Keep native path bytes, including relative or empty values. This
            // records selection only; it does not authorize a write there.
            vars("EZA_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| xdg_config_home.join("eza"))
        };

        Ok(SlateEnv {
            lazygit_paths: lazygit::Paths::capture(
                &home,
                &xdg_config_home,
                &vars,
                slate_home_override.is_some(),
            ),
            zellij_paths: zellij::Paths::capture(
                &home,
                &xdg_config_home,
                &vars,
                slate_home_override.is_some(),
            ),
            session: crate::session::SessionContext::from_vars(&vars),
            home,
            xdg_config_home,
            xdg_cache_home,
            xdg_data_home,
            xdg_data_home_overridden,
            slate_config_dir,
            slate_cache_dir,
            zsh_config_home,
            nvim_config_dir,
            bat_paths,
            opencode_tui_config,
            yazi_config_home,
            eza_config_home,
            eza_color_values: if slate_home_override.is_none() {
                [nonempty("EZA_COLORS"), nonempty("LS_COLORS")]
            } else {
                [None, None]
            },
            starship_config_override: if slate_home_override.is_some() {
                None
            } else {
                nonempty("STARSHIP_CONFIG").map(PathBuf::from)
            },
        })
    }

    /// Create with injected home path (for testing)
    /// Useful for sandboxing tests: SlateEnv::with_home(tempdir.path().to_path_buf())
    /// will ensure all config and cache file writes go to tempdir instead of developer's home.
    pub fn with_home(home: PathBuf) -> Self {
        let xdg_config_home = home.join(".config");
        let xdg_cache_home = home.join(".cache");
        let xdg_data_home = home.join(".local/share");
        let slate_config_dir = xdg_config_home.join("slate");
        let slate_cache_dir = xdg_cache_home.join("slate");
        SlateEnv {
            lazygit_paths: lazygit::Paths::capture(&home, &xdg_config_home, &|_| None, true),
            zellij_paths: zellij::Paths::capture(&home, &xdg_config_home, &|_| None, true),
            session: crate::session::SessionContext::isolated(),
            zsh_config_home: home.clone(),
            nvim_config_dir: xdg_config_home.join("nvim"),
            bat_paths: bat::BatPaths::capture(&|_| None, true, &xdg_config_home, &xdg_cache_home),
            opencode_tui_config: None,
            yazi_config_home: xdg_config_home.join("yazi"),
            eza_config_home: xdg_config_home.join("eza"),
            eza_color_values: [None, None],
            starship_config_override: None,
            home,
            xdg_config_home,
            xdg_cache_home,
            xdg_data_home,
            xdg_data_home_overridden: false,
            slate_config_dir,
            slate_cache_dir,
        }
    }

    /// Get home directory path
    pub fn home(&self) -> &Path {
        &self.home
    }

    pub(crate) fn lazygit_default_config(&self) -> &Path {
        &self.lazygit_paths.default
    }

    pub(crate) fn lazygit_primary_config(&self) -> &Path {
        &self.lazygit_paths.primary
    }

    pub(crate) fn lazygit_config_selection(&self) -> Option<&std::ffi::OsStr> {
        self.lazygit_paths.selection.as_deref()
    }

    pub fn session(&self) -> &crate::session::SessionContext {
        &self.session
    }

    /// Captured Yazi profile, isolated by SLATE_HOME. Invalid relative overrides
    /// are retained so the adapter rejects them rather than editing a fallback.
    pub fn yazi_config_home(&self) -> &Path {
        &self.yazi_config_home
    }

    /// Captured eza directory selection, not a Slate-managed write destination.
    pub fn eza_config_home(&self) -> &Path {
        &self.eza_config_home
    }

    /// Presence only; raw color expressions are not retained or evaluated.
    pub(crate) fn eza_color_overrides(&self) -> bool {
        self.eza_color_values.iter().any(Option::is_some)
    }

    /// Exact comparison only: do not interpret or expose personal color strings.
    pub(crate) fn eza_colors_match(&self, eza: &str, ls: &str) -> bool {
        self.eza_color_overrides()
            && self
                .eza_color_values
                .iter()
                .zip([eza, ls])
                .all(|(actual, expected)| {
                    actual
                        .as_deref()
                        .is_none_or(|value| value == std::ffi::OsStr::new(expected))
                })
    }

    /// Read-only diagnostic selection, not an adapter write target. Relative
    /// overrides remain unresolved; an isolated profile ignores host overrides.
    pub(crate) fn starship_config_override(&self) -> Option<&Path> {
        self.starship_config_override.as_deref()
    }

    /// bat's captured assets root, independent of its config-file override.
    pub fn bat_config_dir(&self) -> &Path {
        &self.bat_paths.config_dir
    }

    pub fn bat_config_path(&self) -> &Path {
        &self.bat_paths.config_file
    }

    pub fn bat_cache_dir(&self) -> &Path {
        &self.bat_paths.cache_dir
    }

    /// Validate captured bat paths before writing or launching its cache builder.
    pub fn validate_bat_paths(&self) -> Result<()> {
        self.bat_paths.validate()
    }

    /// Captured once and absent under SLATE_HOME or with_home isolation. Valid
    /// overrides are absolute; an unresolved override retains its supplied path
    /// for diagnostics. Writers must call validate_opencode_tui_config first.
    pub fn opencode_tui_config(&self) -> Option<&Path> {
        self.opencode_tui_config
            .as_ref()
            .map(|config| config.path.as_path())
    }

    pub(crate) fn opencode_tui_config_was_relative(&self) -> bool {
        self.opencode_tui_config
            .as_ref()
            .is_some_and(|config| config.was_relative)
    }

    pub(crate) fn opencode_tui_config_error(&self) -> Option<&str> {
        self.opencode_tui_config
            .as_ref()
            .and_then(|config| config.error.as_deref())
    }

    pub fn validate_opencode_tui_config(&self) -> Result<()> {
        match self.opencode_tui_config_error() {
            Some(error) => Err(crate::error::SlateError::InvalidConfig(error.to_owned())),
            None => Ok(()),
        }
    }

    /// tmux uses the first existing user config, with the legacy path first.
    pub fn tmux_config_candidates(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.home.join(".tmux.conf"),
            self.xdg_config_home.join("tmux/tmux.conf"),
        ];
        let fallback = self.home.join(".config/tmux/tmux.conf");
        if !paths.contains(&fallback) {
            paths.push(fallback);
        }
        paths
    }

    pub fn tmux_config_path(&self) -> PathBuf {
        self.tmux_config_candidates()
            .into_iter()
            .find(|path| path.exists())
            .unwrap_or_else(|| self.home.join(".tmux.conf"))
    }

    /// Get XDG config home (~/.config or $XDG_CONFIG_HOME)
    pub fn xdg_config_home(&self) -> &Path {
        &self.xdg_config_home
    }

    /// Captured once; relative/empty values fall back to HOME/.local/share.
    /// SLATE_HOME and with_home always use the isolated default.
    pub fn xdg_data_home(&self) -> &Path {
        &self.xdg_data_home
    }

    pub(crate) fn xdg_data_home_overridden(&self) -> bool {
        self.xdg_data_home_overridden
    }

    /// Get slate config directory path (~/.config/slate or $XDG_CONFIG_HOME/slate)
    pub fn config_dir(&self) -> &Path {
        &self.slate_config_dir
    }

    /// Get XDG cache home (~/.cache or $XDG_CACHE_HOME)
    pub fn cache_dir(&self) -> &Path {
        &self.xdg_cache_home
    }

    /// Get slate cache directory path (~/.cache/slate or $XDG_CACHE_HOME/slate)
    pub fn slate_cache_dir(&self) -> &Path {
        &self.slate_cache_dir
    }

    /// Get .zshrc path (for shell integration marker block)
    pub fn zshrc_path(&self) -> PathBuf {
        self.zsh_config_home.join(".zshrc")
    }

    /// Neovim's active profile, honoring XDG_CONFIG_HOME and NVIM_APPNAME.
    pub fn nvim_config_dir(&self) -> &Path {
        &self.nvim_config_dir
    }

    /// Profile-local opt-out, within Slate's existing owned loader directory.
    pub fn nvim_auto_activation_path(&self) -> PathBuf {
        self.nvim_config_dir
            .join("lua/slate/auto-activation.disabled")
    }

    /// Prefer the existing init file; do not create init.lua beside init.vim.
    pub fn nvim_init_path(&self) -> PathBuf {
        let lua = self.nvim_config_dir.join("init.lua");
        let vim = self.nvim_config_dir.join("init.vim");
        if lua.exists() || !vim.exists() {
            lua
        } else {
            vim
        }
    }

    /// Get .bashrc path (raw accessor for backup/snapshot code).
    pub fn bashrc_path(&self) -> PathBuf {
        self.home.join(".bashrc")
    }

    /// Get .bash_profile path (raw accessor).
    pub fn bash_profile_path(&self) -> PathBuf {
        self.home.join(".bash_profile")
    }

    /// Get fish conf.d directory for managed loader files.
    pub fn fish_conf_d_dir(&self) -> PathBuf {
        self.xdg_config_home.join("fish").join("conf.d")
    }

    /// Get the path to the Slate-managed fish loader file.
    pub fn fish_loader_path(&self) -> PathBuf {
        self.fish_conf_d_dir().join("slate.fish")
    }

    /// Get the per-user local bin directory (~/.local/bin).
    pub fn user_local_bin(&self) -> PathBuf {
        self.home.join(".local").join("bin")
    }

    /// Get path to a managed config file (e.g., current, current-font, auto.toml)
    pub fn managed_file(&self, filename: &str) -> PathBuf {
        self.slate_config_dir.join(filename)
    }

    /// Get path to a managed subdirectory (e.g., managed/, user/, shell/)
    pub fn managed_subdir(&self, subdir: &str) -> PathBuf {
        self.slate_config_dir.join(subdir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn custom_paths_resolve_profiles_and_ignore_invalid_xdg_roots() {
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some("/tmp/slate-user".into()),
            "XDG_CONFIG_HOME" => Some("/tmp/slate-config".into()),
            "XDG_CACHE_HOME" => Some("relative-cache".into()),
            "NVIM_APPNAME" => Some("profiles/work".into()),
            "ZDOTDIR" => Some("/tmp/shell config".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            env.nvim_config_dir(),
            Path::new("/tmp/slate-config/profiles/work")
        );
        assert_eq!(env.zshrc_path(), Path::new("/tmp/shell config/.zshrc"));
        assert_eq!(env.cache_dir(), Path::new("/tmp/slate-user/.cache"));
        for invalid in ["../other", "/other", "."] {
            assert!(SlateEnv::from_vars(|key| match key {
                "HOME" => Some("/tmp/slate-user".into()),
                "NVIM_APPNAME" => Some(invalid.into()),
                _ => None,
            })
            .is_err());
        }
    }

    #[test]
    fn custom_paths_isolation_ignores_host_overrides() {
        let env = SlateEnv::from_vars(|key| match key {
            "SLATE_HOME" => Some("/tmp/isolated".into()),
            "HOME" | "XDG_CONFIG_HOME" | "XDG_CACHE_HOME" | "ZDOTDIR" | "NVIM_APPNAME" => {
                Some("/host/untouched".into())
            }
            _ => None,
        })
        .unwrap();
        assert_eq!(env.zshrc_path(), Path::new("/tmp/isolated/.zshrc"));
        assert_eq!(
            env.nvim_config_dir(),
            Path::new("/tmp/isolated/.config/nvim")
        );
        assert_eq!(
            env.slate_cache_dir(),
            Path::new("/tmp/isolated/.cache/slate")
        );
        let defaults = SlateEnv::from_vars(|key| match key {
            "HOME" => Some("/tmp/isolated".into()),
            _ => Some("".into()),
        })
        .unwrap();
        assert_eq!(defaults.nvim_config_dir(), env.nvim_config_dir());
        assert_eq!(defaults.zshrc_path(), env.zshrc_path());
    }

    #[test]
    fn test_from_process_reads_home() {
        // This test only checks that from_process can be called
        // We don't assert on the actual result since we can't control HOME in tests
        // without isolation. Real validation happens in integration tests.
        let _result = SlateEnv::from_process();
        // If it doesn't panic, the test passes
    }

    #[test]
    fn test_with_home_creates_valid_env() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert_eq!(env.home(), tempdir.path());
        assert!(env.xdg_config_home().ends_with(".config"));
        assert!(env.config_dir().ends_with(".config/slate"));
        assert!(env.cache_dir().ends_with(".cache"));
        assert!(env.slate_cache_dir().ends_with(".cache/slate"));
    }

    #[test]
    fn test_zshrc_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let zshrc = env.zshrc_path();

        assert!(zshrc.ends_with(".zshrc"));
    }

    #[test]
    fn test_bashrc_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let bashrc = env.bashrc_path();

        assert!(bashrc.ends_with(".bashrc"));
    }

    #[test]
    fn test_fish_loader_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.fish_conf_d_dir().ends_with(".config/fish/conf.d"));
        assert!(env
            .fish_loader_path()
            .ends_with(".config/fish/conf.d/slate.fish"));
    }

    #[test]
    fn test_user_local_bin_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.user_local_bin().ends_with(".local/bin"));
    }

    #[test]
    fn test_managed_file() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config_file = env.managed_file("current");

        assert!(config_file.ends_with(".config/slate/current"));
    }

    #[test]
    fn test_managed_subdir() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let managed_dir = env.managed_subdir("managed");

        assert!(managed_dir.ends_with(".config/slate/managed"));
    }

    #[test]
    fn test_cache_dir() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.cache_dir().ends_with(".cache"));
        assert!(env.slate_cache_dir().ends_with(".cache/slate"));
    }
}

#[cfg(test)]
mod slate_cache_dir_tests {
    //! Task 1: dedicated coverage for `SlateEnv::slate_cache_dir`.
    //! These tests prove the accessor is XDG-aware (via the `from_process`
    //! / `with_home` constructors) and stable across calls — without
    //! mutating `std::env::set_var` (per user preference `feedback_no_tech_debt`,
    //! pure-function testing, no global env mutation).
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn slate_cache_dir_honors_injected_home() {
        // Constructor injection (`with_home`) is the pure-function-friendly
        // test hook: no XDG_CACHE_HOME mutation required. The resolved
        // path must sit under `<injected-home>/.cache/slate`, regardless
        // of whatever `XDG_CACHE_HOME` happens to be set to at runtime.
        let tempdir = TempDir::new().expect("create tempdir");
        let injected_home: PathBuf = tempdir.path().to_path_buf();
        let env = SlateEnv::with_home(injected_home.clone());

        let dir = env.slate_cache_dir();
        assert!(
            dir.starts_with(&injected_home),
            "expected slate_cache_dir under injected home {:?}, got {:?}",
            injected_home,
            dir
        );
        assert!(
            dir.ends_with(".cache/slate"),
            "expected path ending with '.cache/slate', got {:?}",
            dir
        );
    }

    #[test]
    fn slate_cache_dir_is_stable_across_calls() {
        let tempdir = TempDir::new().expect("create tempdir");
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        assert_eq!(
            env.slate_cache_dir(),
            env.slate_cache_dir(),
            "slate_cache_dir must be deterministic across calls"
        );
    }
}
