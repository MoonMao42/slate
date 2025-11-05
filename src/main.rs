use clap::{Parser, Subcommand};
use color_eyre::Result;

#[derive(Parser)]
#[command(name = "slate")]
#[command(about = "macOS terminal beautification kit", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
    Setup {
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
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Setup { quick } => {
            // Placeholder for 
            println!("Setup wizard (quick={}) — implemented in ", quick);
        }
        Commands::Set { theme } => {
            // Placeholder for 
            println!("Set theme: {:?} — implemented in ", theme);
        }
        Commands::Status => {
            // Placeholder for 
            println!("Status — implemented in ");
        }
        Commands::List => {
            // Placeholder for 
            println!("List themes — implemented in ");
        }
        Commands::Restore { backup_id } => {
            // Placeholder for 
            println!("Restore: {:?} — implemented in ", backup_id);
        }
        Commands::Init { shell } => {
            // Placeholder for 
            println!("Init shell: {:?} — implemented in ", shell);
        }
    }

    Ok(())
}
