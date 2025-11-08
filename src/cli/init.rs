use crate::config::ConfigManager;
use crate::error::Result;

/// Handle `slate init [shell]` command
/// Init subcommand scaffolded alongside setup/set/status/list/restore
pub fn handle(args: &[&str]) -> Result<()> {
    let shell = args.first().copied().unwrap_or("zsh");
    let config_manager = ConfigManager::new()?;
    print!("{}", config_manager.render_shell_init(shell)?);

    Ok(())
}
