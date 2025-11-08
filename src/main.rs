use clap::{Parser, Subcommand};
use color_eyre::Result;
use slate_cli::{cli, error};

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
    /// Restore previous configuration
    Restore {
        /// Backup point ID (optional; if omitted, shows list)
        backup_id: Option<String>,
    },
    /// Initialize shell integration
    Init {
        /// Shell type (zsh, bash, fish)
        shell: Option<String>,
    },
}

fn main() -> Result<()> {
    error::install_error_handler()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Setup { quick } => {
            // Dispatch to setup handler
            let args: Vec<&str> = if quick { vec!["--quick"] } else { vec![] };
            cli::setup::handle(&args)?;
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
        Commands::Restore { backup_id } => {
            let args: Vec<&str> = backup_id.as_ref().map(|id| vec![id.as_str()]).unwrap_or_default();
            cli::restore::handle(&args)?;
        }
        Commands::Init { shell } => {
            let args: Vec<&str> = shell.as_ref().map(|s| vec![s.as_str()]).unwrap_or_default();
            cli::init::handle(&args)?;
        }
    }

    Ok(())
}
