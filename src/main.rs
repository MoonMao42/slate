use clap::{Parser, Subcommand};
use themectl::{
    cli::handle_list_command, cli::handle_set_command, cli::handle_status_command,
    cli::handle_restore_command, ThemeResult,
};

/// A zero-configuration terminal theme switcher
/// Apply your favorite theme to Ghostty, Starship, bat and other terminal tools
/// with a single command. No config needed.
#[derive(Parser)]
#[command(name = "themectl")]
#[command(version = "0.1.0")]
#[command(about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available themes
    #[command(about = "List available themes")]
    List,
    /// Apply a theme to all detected tools
    #[command(about = "Apply a theme")]
    Set {
        /// Name of the theme to apply (e.g., catppuccin-mocha, tokyo-night-dark)
        #[arg(value_name = "THEME")]
        theme: String,

        /// Show detection and modification details
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show current theme state of installed tools
    #[command(about = "Show current theme state")]
    Status {
        /// Show detection problems and warnings
        #[arg(short, long)]
        verbose: bool,
    },
    /// Restore a previous theme state from backups
    #[command(about = "Restore a previous theme state")]
    Restore {
        /// Restore point ID to restore (interactive selection in TTY if omitted)
        #[arg(value_name = "RESTORE_POINT_ID")]
        restore_point_id: Option<String>,

        /// List all available restore points
        #[arg(long)]
        list: bool,

        /// Delete a specific restore point
        #[arg(long, value_name = "RESTORE_POINT_ID")]
        cleanup: Option<String>,

        /// Delete all restore points
        #[arg(long)]
        clear_all: bool,
    },
}

fn main() -> ThemeResult<()> {
    // Initialize color-eyre for better error formatting
    let _ = color_eyre::install();

    let args = Args::parse();

    match args.command {
        Commands::List => match handle_list_command() {
            Ok(_) => {
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{}", themectl::cli::format_error(&e));
                std::process::exit(1);
            }
        },
        Commands::Set { theme, verbose } => {
            match handle_set_command(&theme, verbose) {
                Ok(_) => {
                    std::process::exit(0);
                }
                Err(themectl::ThemeError::PartialFailure(_)) => {
                    // Partial failure already printed
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{}", themectl::cli::format_error(&e));
                    std::process::exit(1);
                }
            }
        }
        Commands::Status { verbose } => match handle_status_command(verbose) {
            Ok(_) => {
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{}", themectl::cli::format_error(&e));
                std::process::exit(1);
            }
        },
        Commands::Restore {
            restore_point_id,
            list,
            cleanup,
            clear_all,
        } => match handle_restore_command(restore_point_id, list, cleanup, clear_all) {
            Ok(_) => {
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{}", themectl::cli::format_error(&e));
                std::process::exit(1);
            }
        },
    }
}
