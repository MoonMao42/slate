//! Read-only setup preparation. Resolves choices before installers or writers
//! run; this is not a filesystem transaction or a package-manager sandbox.
use crate::adapter::ToolRegistry;
use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{ToolCatalog, ToolMetadata};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::shell::ShellBackend;
use crate::theme::ThemeVariant;

pub(crate) struct PreparedSetup {
    pub(super) env: SlateEnv,
    pub(super) tools_to_install: Vec<ToolMetadata>,
    pub(super) tools_to_configure: Vec<String>,
    pub(super) font: Option<String>,
    pub(super) theme: ThemeVariant,
    pub(super) shell: ShellBackend,
    pub(super) loader: super::shell_loader::PreparedShellLoader,
    pub(super) reviewed_installs: Option<crate::cli::tool_selection::InstallPlan>,
}

impl PreparedSetup {
    pub(crate) fn with_reviewed_installs(
        mut self,
        reviewed: crate::cli::tool_selection::InstallPlan,
        context: crate::platform::packages::InstallContext,
    ) -> Result<Self> {
        reviewed.verify_selection(&self.tools_to_install, &self.env)?;
        reviewed.verify_context(context)?;
        self.reviewed_installs = Some(reviewed);
        Ok(self)
    }
}

pub(crate) fn prepare_setup_with_env(
    installs: &[String],
    configure: &[String],
    font: Option<&str>,
    theme: Option<&str>,
    env: &SlateEnv,
) -> Result<PreparedSetup> {
    prepare_with_shell(
        installs,
        configure,
        font,
        theme,
        env,
        crate::platform::shell::detect_backend(),
    )
}

pub(super) fn prepare_with_shell(
    installs: &[String],
    configure: &[String],
    font: Option<&str>,
    theme: Option<&str>,
    env: &SlateEnv,
    shell: ShellBackend,
) -> Result<PreparedSetup> {
    let mut tools_to_install: Vec<ToolMetadata> = Vec::new();
    for id in installs {
        let tool = ToolCatalog::get_tool(id).ok_or_else(|| {
            SlateError::InvalidConfig(format!("Unknown setup tool '{}'", id.escape_default()))
        })?;
        if !tool.installable {
            return Err(SlateError::InvalidConfig(format!(
                "{} is detect-only, not installable through setup; select it for configuration instead.",
                tool.label
            )));
        }
        if !tools_to_install.iter().any(|item| item.id == tool.id) {
            tools_to_install.push(tool);
        }
    }

    // Configuration supports all registered adapters, including those not
    // offered for installation by the wizard. Never silently drop unknown IDs.
    let registry = ToolRegistry::default();
    let mut tools_to_configure = Vec::new();
    for id in configure {
        if registry.get_adapter(id).is_none() {
            return Err(SlateError::InvalidConfig(format!(
                "Unknown setup configuration target '{}'",
                id.escape_default()
            )));
        }
        if !tools_to_configure.contains(id) {
            tools_to_configure.push(id.clone());
        }
    }

    let font = font.map(normalize_font_request).transpose()?;
    super::integration::validate_shell(shell)?;
    // A malformed consent record is not permission to activate an editor.
    crate::config::ConfigManager::from_env_paths(env).is_editor_auto_activation_enabled()?;
    let theme = super::integration::resolve_selected_theme(theme, env)?;
    let loader = super::shell_loader::PreparedShellLoader::capture(env, shell)?;
    Ok(PreparedSetup {
        env: env.clone(),
        tools_to_install,
        tools_to_configure,
        font,
        theme,
        shell,
        loader,
        reviewed_installs: None,
    })
}

fn normalize_font_request(request: &str) -> Result<String> {
    crate::adapter::font_config::validate_family(request)?;
    // Keep literal installed families supported without probing system fonts
    // during preparation. Known display names use the same ID for every install
    // backend, instead of working through brew but failing the download fallback.
    Ok(FontCatalog::all_fonts()
        .into_iter()
        .find(|font| request == font.id || request == font.name)
        .map_or_else(|| request.to_owned(), |font| font.id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigManager;

    #[test]
    fn install_review_binding_rejects_changed_selection_route_or_profile_before_writes() {
        use crate::{
            cli::tool_selection::InstallPlan,
            platform::packages::{InstallContext, PackageManagerBackend},
        };
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let context = InstallContext {
            package_manager: PackageManagerBackend::Homebrew,
            supported_os: true,
        };
        let review = InstallPlan::capture(&["starship".into()], &env, context).unwrap();
        let prepare = |ids: &[String], selected: &SlateEnv| {
            prepare_with_shell(ids, &[], None, None, selected, ShellBackend::Bash).unwrap()
        };
        assert!(prepare(&["bat".into()], &env)
            .with_reviewed_installs(review.clone(), context)
            .is_err());
        assert!(prepare(&["starship".into()], &env)
            .with_reviewed_installs(
                review.clone(),
                InstallContext {
                    package_manager: PackageManagerBackend::Unsupported,
                    ..context
                }
            )
            .is_err());
        let other = SlateEnv::with_home(temp.path().join("other"));
        assert!(prepare(&["starship".into()], &other)
            .with_reviewed_installs(review.clone(), context)
            .is_err());
        assert!(prepare(&["starship".into()], &env)
            .with_reviewed_installs(review, context)
            .unwrap()
            .reviewed_installs
            .is_some());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn setup_plan_validates_and_deduplicates_before_any_installer() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let plan = prepare_with_shell(
            &["starship".into(), "bat".into(), "starship".into()],
            &[
                "ghostty".into(),
                "nvim".into(),
                "opencode".into(),
                "ghostty".into(),
            ],
            None,
            None,
            &env,
            ShellBackend::Bash,
        )
        .unwrap();
        assert_eq!(
            plan.tools_to_install
                .iter()
                .map(|tool| tool.id)
                .collect::<Vec<_>>(),
            ["starship", "bat"]
        );
        assert_eq!(plan.tools_to_configure, ["ghostty", "nvim", "opencode"]);
        assert_eq!(plan.theme.id, crate::theme::DEFAULT_THEME_ID);
        assert_eq!(plan.env.home(), env.home());
        assert_eq!(plan.shell, ShellBackend::Bash);
        let wizard_targets: Vec<_> = ToolCatalog::all_tools()
            .iter()
            .map(|tool| tool.id.into())
            .collect();
        assert!(
            prepare_with_shell(&[], &wizard_targets, None, None, &env, ShellBackend::Bash).is_ok()
        );
        assert!(std::fs::read_dir(td.path()).unwrap().next().is_none());
        for id in ["ghostty", "tmux", "unknown\u{1b}"] {
            let error = prepare_with_shell(
                &["starship".into(), id.into()],
                &[],
                None,
                None,
                &env,
                ShellBackend::Bash,
            )
            .err()
            .expect("invalid tool should be rejected");
            assert!(!error.to_string().contains('\u{1b}'));
        }
        assert!(std::fs::read_dir(td.path()).unwrap().next().is_none());
    }

    #[test]
    fn setup_plan_font_validation_is_pure_and_preserves_literal_families() {
        for font in FontCatalog::all_fonts() {
            for request in [font.id, font.name] {
                assert_eq!(normalize_font_request(request).unwrap(), font.id);
            }
        }
        // A bare family can be a distinct non-Nerd font, not a catalog alias.
        assert_eq!(normalize_font_request("Hack").unwrap(), "Hack");
        assert_eq!(
            normalize_font_request("Private \"Mono\" 字体").unwrap(),
            "Private \"Mono\" 字体"
        );
        for invalid in ["", "\u{1b}[31m", "Mono\n", "Mono\u{2028}", &"x".repeat(257)] {
            assert!(normalize_font_request(invalid).is_err());
        }
    }

    #[test]
    fn setup_plan_freezes_saved_theme_and_profile_without_writes() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("nord").unwrap();
        let plan = prepare_with_shell(&[], &[], None, None, &env, ShellBackend::Fish).unwrap();
        config.set_current_theme("kanagawa-wave").unwrap();
        assert_eq!(plan.theme.id, "nord");
        assert_eq!(plan.shell, ShellBackend::Fish);
        assert_eq!(plan.env.config_dir(), env.config_dir());
        config.set_current_theme("unknown\u{1b}").unwrap();
        let error = prepare_with_shell(&[], &[], None, None, &env, ShellBackend::Bash)
            .err()
            .expect("invalid saved theme should be rejected");
        assert!(!error.to_string().contains('\u{1b}'));
        // An explicit valid selection need not read broken saved state.
        assert!(prepare_with_shell(&[], &[], None, Some("nord"), &env, ShellBackend::Bash).is_ok());
    }

    #[test]
    fn setup_plan_execution_uses_prepared_theme_and_shell() {
        for shell in [ShellBackend::Bash, ShellBackend::Zsh, ShellBackend::Fish] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            config.set_current_theme("nord").unwrap();
            let plan = prepare_with_shell(&[], &[], None, None, &env, shell).unwrap();
            config.set_current_theme("catppuccin-mocha").unwrap();
            let summary = crate::cli::setup_executor::execute_prepared_setup(plan).unwrap();
            assert!(summary.is_successful());
            assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
            assert_eq!(
                env.bash_integration_path().is_file(),
                shell == ShellBackend::Bash
            );
            assert_eq!(env.zshrc_path().is_file(), shell == ShellBackend::Zsh);
            assert_eq!(
                env.fish_loader_path().is_file(),
                shell == ShellBackend::Fish
            );
        }
    }
}
