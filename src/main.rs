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
    /// Set or pick theme
    Theme {
        /// Theme name (optional; if omitted, launches picker)
        name: Option<String>,
        /// Apply currently auto-resolved theme based on system appearance
        #[arg(long, conflicts_with = "name")]
        auto: bool,
        /// Suppress output (for shell hook usage)
        #[arg(long)]
        quiet: bool,
    },
    /// Set or pick font
    Font {
        /// Font name (optional; if omitted, launches picker)
        name: Option<String>,
    },
    /// Configure slate settings (opacity, auto-theme)
    Config {
        /// Subcommand (currently: set)
        #[command(subcommand)]
        subcommand: ConfigSubcommand,
    },
    /// Show current configuration
    Status,
    /// List available themes
    List,
    /// Clean up slate-managed configuration
    Clean,
    /// Restore from a previous configuration snapshot
    Restore {
        /// Restore point ID (optional; if omitted, shows picker)
        id: Option<String>,
        /// List restore points without restoring
        #[arg(long)]
        list: bool,
        /// Delete a specific restore point
        #[arg(long, value_name = "ID")]
        delete: Option<String>,
    },
    /// Deprecated: use 'slate restore' instead
    #[command(hide = true)]
    Reset {
        /// Backup ID (for compatibility)
        #[arg(value_name = "ID")]
        id: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigSubcommand {
    /// Set a configuration value
    Set {
        /// Key to set (opacity, auto-theme)
        key: String,
        /// Value to set
        value: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    error::install_error_handler()?;

    // Initialize SlateEnv from process environment early
    let env = SlateEnv::from_process()?;

    let cli = Cli::parse();

    let result = match cli.command {
        None => {
            // Bare `slate` invocation routes to hub
            cli::hub::handle()
        }
        Some(Commands::Setup { quick, force, only }) => {
            cli::setup::handle_with_env(quick, force, only, &env)
        }
        Some(Commands::Set { theme, auto }) => cli::set::handle(theme.as_deref(), auto),
        Some(Commands::Theme { name, auto, quiet }) => cli::theme::handle_theme(name, auto, quiet),
        Some(Commands::Font { name }) => cli::font::handle_font(name.as_deref()),
        Some(Commands::Config { subcommand }) => match subcommand {
            ConfigSubcommand::Set { key, value } => cli::config::handle_config_set(&key, &value),
        },
        Some(Commands::Status) => cli::status::handle(&[]),
        Some(Commands::List) => cli::list::handle(&[]),
        Some(Commands::Clean) => cli::clean::handle_clean(),
        Some(Commands::Restore { id, list, delete }) => {
            cli::restore::handle(id.as_deref(), list, delete.as_deref())
        }
        Some(Commands::Reset { id }) => {
            // reset is now a compatibility alias that routes to restore
            println!("(i) Tip: 'slate reset' is transitioning to 'slate restore'. Use 'slate restore [id]' next time.");
            println!();
            cli::restore::handle(id.as_deref(), false, None)
        }
    };

    // Unified cancellation handling — clean exit with no error dump
    match result {
        Err(error::SlateError::UserCancelled) => {
            let _ = cliclack::outro_cancel("");
            std::process::exit(130);
        }
        Err(error::SlateError::IOError(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
            let _ = cliclack::outro_cancel("");
            std::process::exit(130);
        }
        other => Ok(other?),
    }
}
