use clap::{Parser, Subcommand};
use color_eyre::Result;
use slate_cli::{brand, cli, env::SlateEnv, error};

#[derive(Parser)]
#[command(name = "slate")]
#[command(version)]
#[command(about = "✦ slate — terminal beautification kit for macOS and Linux")]
#[command(long_about = "Transform your terminal in 30 seconds across macOS and Linux")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Apply auto-resolved theme (system appearance) — also silences SFX
    #[arg(long, global = true)]
    auto: bool,

    /// Suppress output (for shell hook usage) — also silences SFX
    #[arg(long, global = true)]
    quiet: bool,
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
    /// Set or pick theme
    Theme {
        /// Theme name (optional; if omitted, launches picker)
        name: Option<String>,
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
    /// Screenshot your terminal with share code
    Share,
    /// Export current config as a shareable code
    Export,
    /// Import a shared config
    Import {
        /// Share code (e.g. slate://catppuccin-mocha/JetBrainsMono/frosted/s,h,f)
        uri: String,
    },
    /// Hidden easter egg
    #[command(hide = true)]
    Aura,
    /// Hidden auto-theme watcher entrypoint (Linux-only; macOS uses an embedded Swift helper)
    #[cfg(target_os = "linux")]
    #[command(hide = true, name = "__watch-auto-theme")]
    WatchAutoTheme,
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

    // seat cliclack's global theme before any command
    // handler runs. Brand events intentionally do NOT pre-seat the
    // default sink here — `dispatch()` self-initializes with NoopSink,
    // and leaving the slot untouched preserves chance to
    // register a real sink before the first dispatch.
    brand::cliclack_theme::init();

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
        Some(Commands::Set { theme }) => cli::set::handle(theme.as_deref(), cli.auto),
        Some(Commands::Theme { name }) => cli::theme::handle_theme(name, cli.auto, cli.quiet),
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
        Some(Commands::Share) => cli::share_screenshot::handle_share(),
        Some(Commands::Export) => cli::share::handle_export(),
        Some(Commands::Import { uri }) => cli::share::handle_import(&uri),
        Some(Commands::Aura) => cli::aura::handle(),
        #[cfg(target_os = "linux")]
        Some(Commands::WatchAutoTheme) => cli::watch::handle_auto_theme_watch(),
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
