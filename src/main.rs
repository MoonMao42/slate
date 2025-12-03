use clap::{Parser, Subcommand};
use color_eyre::Result;
use slate_cli::{cli, env::SlateEnv, error};

#[derive(Parser)]
#[command(name = "slate")]
#[command(about = "✦ slate — macOS terminal beautification kit")]
#[command(long_about = "Transform your terminal in 30 seconds")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
    let _env = SlateEnv::from_process()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Setup { quick, force, only } => {
            // Dispatch to setup handler
            match cli::setup::handle(quick, force, only) {
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
        Commands::Set { theme } => {
            // Dispatch to set handler
            let args: Vec<&str> = theme.as_ref().map(|t| vec![t.as_str()]).unwrap_or_default();
            cli::set::handle(&args)?;
        }
        Commands::Status => {
            cli::status::handle(&[])?;
        }
        Commands::List => {
            cli::list::handle(&[])?;
        }
        Commands::Reset { backup_id } => {
            let args: Vec<&str> = backup_id
                .as_ref()
                .map(|id| vec![id.as_str()])
                .unwrap_or_default();
            cli::restore::handle(&args)?;
        }
    }

    Ok(())
}
