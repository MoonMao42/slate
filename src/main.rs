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
        /// List available themes grouped by family
        #[arg(long)]
        list: bool,
        /// Theme ID/display name, or `set <theme>` for compatibility with documented examples
        #[arg(value_name = "THEME", num_args = 0..=2)]
        args: Vec<String>,
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

    // register the EventSink seam with the real SoundSink
    // implementation. Must precede any brand-event dispatch (Pitfall 5).
    // `auto || quiet || config.sound=off || cache-unpack-fail` internally
    // degrades to NoopSink — this call never panics and never surfaces errors.
    brand::SoundSink::install(&env, cli.auto, cli.quiet);

    let result = match cli.command {
        None => {
            // Bare `slate` invocation routes to hub
            cli::hub::handle()
        }
        Some(Commands::Setup { quick, force, only }) => {
            cli::setup::handle_with_env(quick, force, only, &env)
        }
        Some(Commands::Set { theme }) => cli::set::handle(theme.as_deref(), cli.auto),
        Some(Commands::Theme { list, args }) => {
            handle_theme_command(list, args, cli.auto, cli.quiet)
        }
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
            brand::flush();
            let _ = cliclack::outro_cancel("");
            std::process::exit(130);
        }
        Err(error::SlateError::IOError(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
            brand::flush();
            let _ = cliclack::outro_cancel("");
            std::process::exit(130);
        }
        other => {
            let final_result = other.map(|_| ());
            brand::flush();
            Ok(final_result?)
        }
    }
}

fn handle_theme_command(
    list: bool,
    args: Vec<String>,
    auto: bool,
    quiet: bool,
) -> error::Result<()> {
    if list {
        if !args.is_empty() {
            return Err(error::SlateError::InvalidConfig(
                "`slate theme --list` does not accept a theme argument".to_string(),
            ));
        }
        return cli::list::handle(&[]);
    }

    match args.as_slice() {
        [] => cli::theme::handle_theme(None, auto, quiet),
        [name] => cli::theme::handle_theme(Some(name.clone()), auto, quiet),
        [verb, name] if verb == "set" => cli::theme::handle_theme(Some(name.clone()), auto, quiet),
        [verb, _] => Err(error::SlateError::InvalidConfig(format!(
            "unknown `slate theme {}` form. Use `slate theme <theme>`, `slate theme set <theme>`, or `slate theme --list`.",
            verb
        ))),
        _ => unreachable!("clap limits theme args to at most 2 values"),
    }
}
