use clap::{Parser, Subcommand};
use color_eyre::Result;
use slate_cli::{cli, env::SlateEnv, error};

#[derive(Parser)]
#[command(name = "slate")]
#[command(about = "✦ slate — macOS terminal beautification kit")]
#[command(long_about = "Transform your terminal in 30 seconds")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
    Setup {
        /// Skip questions, use all defaults
        #[arg(long)]
        quick: bool,
        /// Ignore current state and run as fresh install
        #[arg(long)]
        force: bool,
        /// Retry only a specific tool (skip wizard, install just this tool)
        #[arg(long, value_name = "TOOL")]
        only: Option<String>,
    },
    /// Switch to a theme
    Set {
        /// Theme name (optional; if omitted, launches picker)
        theme: Option<String>,
        /// Auto-follow macOS system appearance (light/dark)
        #[arg(long, conflicts_with = "theme")]
        auto: bool,
    },
    /// Show current configuration
    Status,
    /// List available themes
    List,
    /// Reset to previous configuration
    Reset {
        /// Backup point ID (optional; if omitted, shows list)
        backup_id: Option<String>,
    },
}

fn main() -> Result<()> {
    error::install_error_handler()?;

    // Initialize SlateEnv from process environment early
    let env = SlateEnv::from_process()?;

    let cli = Cli::parse();

    match cli.command {
        None => {
            // Bare `slate` invocation routes to hub
            cli::hub::handle()?;
        }
        Some(Commands::Setup { quick, force, only }) => {
            // Dispatch to setup handler with env
            match cli::setup::handle_with_env(quick, force, only, &env) {
                Err(error::SlateError::UserCancelled) => {
                    let _ = cliclack::outro_cancel("✦ Setup cancelled.");
                    std::process::exit(130);
                }
                Err(error::SlateError::IOError(ref e))
                    if e.kind() == std::io::ErrorKind::Interrupted =>
                {
                    // Fallback: IO interrupted that slipped past handle_cliclack_error
                    let _ = cliclack::outro_cancel("✦ Setup cancelled.");
                    std::process::exit(130);
                }
                other => other?,
            }
        }
        Some(Commands::Set { theme, auto }) => {
            // Dispatch to set handler
            let mut args: Vec<&str> = Vec::new();
            if auto {
                args.push("--auto");
            } else if let Some(t) = theme.as_ref() {
                args.push(t.as_str());
            }
            cli::set::handle(&args)?;
        }
        Some(Commands::Status) => {
            cli::status::handle(&[])?;
        }
        Some(Commands::List) => {
            cli::list::handle(&[])?;
        }
        Some(Commands::Reset { backup_id }) => {
            let args: Vec<&str> = backup_id
                .as_ref()
                .map(|id| vec![id.as_str()])
                .unwrap_or_default();
            cli::restore::handle(&args)?;
        }
    }

    Ok(())
}
