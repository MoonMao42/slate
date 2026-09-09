use clap::{CommandFactory, Parser, Subcommand};
use color_eyre::Result;
use slate_cli::{brand, cli, env::SlateEnv, error};
use std::io::IsTerminal;

mod completion;

#[derive(Parser)]
#[command(name = "slate")]
#[command(version)]
#[command(long_version = cli::about::LONG_VERSION)]
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
    /// Show this binary's build, path and capabilities without reading settings
    About {
        /// Emit a versioned report of compiled capabilities, not live integrations
        #[arg(long)]
        json: bool,
    },
    /// Choose a prompt layout without changing theme, font or installed tools
    Prompt(cli::prompt::PromptOptions),
    /// Discover tools, review installations or sync selected adapter colors
    Tools(cli::tools::ToolsOptions),
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
    /// Set, pick, list or preview fonts
    #[command(group(clap::ArgGroup::new("font_inspection").args(["list", "dry_run"]).multiple(false)))]
    Font {
        /// Font name (optional; if omitted, launches picker)
        name: Option<String>,
        /// List discovered candidates and catalog choices without changing settings
        #[arg(long, conflicts_with = "name")]
        list: bool,
        /// Filter the font list by family or catalog ID (quote multiple words)
        #[arg(long, requires = "list", value_name = "QUERY")]
        search: Option<String>,
        /// Preview this font's configuration changes without downloading or writing
        #[arg(long, requires = "name")]
        dry_run: bool,
        /// Emit a versioned read-only list or change preview
        #[arg(long, requires = "font_inspection")]
        json: bool,
    },
    /// Configure slate settings (opacity, auto-theme, fastfetch, sound, editor)
    Config {
        /// Read, set, or configure theme pairing
        #[command(subcommand)]
        subcommand: ConfigSubcommand,
    },
    /// Show current configuration
    Status {
        /// Emit saved configuration and preview recovery as JSON
        #[arg(long)]
        json: bool,
    },
    /// Diagnose terminal, editor, or shell configuration issues
    Doctor {
        #[arg(help = format!("Diagnostic target (default: ghostty): {}", cli::doctor::TARGETS.join(", ")))]
        target: Option<String>,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// Also run a bounded `nvim --version` check (nvim target only)
        #[arg(long, requires = "target")]
        check_version: bool,
        /// Read Ghostty configuration without launching its native validator (Ghostty only)
        #[arg(long, conflicts_with = "check_version")]
        files_only: bool,
    },
    /// Search and list available themes
    List(cli::list::ListOptions),
    /// Generate a static shell completion script on stdout (does not install it)
    Completions {
        #[arg(value_enum)]
        shell: completion::Shell,
    },
    /// Clean up slate-managed configuration
    Clean {
        /// Preview file and directory cleanup without changing files or processes
        #[arg(long)]
        dry_run: bool,
        /// Emit a read-only clean preview as JSON
        #[arg(long, requires = "dry_run")]
        json: bool,
    },
    /// Recover files left by an interrupted live preview
    Recover {
        /// Inspect recovery without changing files
        #[arg(long, conflicts_with_all = ["yes", "discard", "export"])]
        dry_run: bool,
        /// Emit a read-only recovery plan as JSON
        #[arg(long, requires = "dry_run")]
        json: bool,
        /// Confirm recovery or discard without an interactive prompt
        #[arg(long)]
        yes: bool,
        /// Delete only the recovery record, keeping current config files
        #[arg(long, conflicts_with = "export")]
        discard: bool,
        /// Export original files to a new private directory, without restoring
        #[arg(long, value_name = "DIRECTORY", conflicts_with = "yes")]
        export: Option<std::path::PathBuf>,
    },
    /// Restore from a previous configuration snapshot
    #[command(group(clap::ArgGroup::new("restore_inspection").args(["list", "dry_run"]).multiple(false)))]
    Restore {
        /// Restore point ID (optional; if omitted, shows picker)
        #[arg(conflicts_with_all = ["list", "delete"])]
        id: Option<String>,
        /// List restore points without restoring
        #[arg(long, conflicts_with = "delete")]
        list: bool,
        /// Include undo checkpoints when listing restore points
        #[arg(long, requires = "list", conflicts_with_all = ["id", "delete", "dry_run"])]
        all: bool,
        /// Delete a specific restore point
        #[arg(long, value_name = "ID")]
        delete: Option<String>,
        /// Show which snapshot files would change without restoring them
        #[arg(long, requires = "id", conflicts_with_all = ["list", "delete"])]
        dry_run: bool,
        /// Emit a machine-readable restore list or dry-run preview
        #[arg(long, requires = "restore_inspection", conflicts_with = "delete")]
        json: bool,
    },
    /// Screenshot your terminal with share code
    Share,
    /// Export current config as a shareable code
    Export {
        /// Print only the share code, without styling or instructions
        #[arg(long)]
        raw: bool,
    },
    /// Import a shared config
    Import {
        /// Share code (e.g. slate://catppuccin-mocha/JetBrainsMono/frosted/s,h,f)
        uri: String,
        /// Preview requested settings without reading or changing your profile
        #[arg(long)]
        dry_run: bool,
        /// Emit a machine-readable import preview
        #[arg(long, requires = "dry_run")]
        json: bool,
    },
    /// Hidden easter egg
    #[command(hide = true)]
    Aura,
    /// Hidden profile-owned auto-theme watcher entrypoint
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
    /// Inspect pairing, or save selected slots without applying a theme
    Pairing(cli::config::pairing::PairingOptions),
    /// Read one resolved preference without writing files or starting tools
    Get {
        /// Key to inspect (opacity, auto-theme, fastfetch, sound, editor)
        key: String,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// List resolved preferences, including defaults and unreadable settings
    List {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Set a configuration value
    Set {
        /// Key to set (opacity, auto-theme, fastfetch, sound, editor)
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

fn needs_write_guard(command: Option<&Commands>, auto: bool) -> bool {
    match command {
        Some(
            Commands::Setup { .. }
            | Commands::Config {
                subcommand: ConfigSubcommand::Set { .. },
            }
            | Commands::Reset { .. },
        ) => true,
        Some(Commands::Font {
            name,
            list,
            dry_run,
            ..
        }) => name.is_some() && !list && !dry_run,
        Some(Commands::Import { dry_run, .. }) => !dry_run,
        Some(Commands::Clean { dry_run, .. }) => !dry_run,
        Some(Commands::Set { theme }) => auto || theme.is_some(),
        Some(Commands::Theme { list, args }) => !list && (auto || !args.is_empty()),
        Some(Commands::Restore {
            id,
            delete,
            list,
            dry_run,
            ..
        }) => (id.is_some() || delete.is_some()) && !list && !dry_run,
        _ => false, // Pickers own their journal lock; diagnostics stay read-only.
    }
}

fn run() -> Result<()> {
    error::install_error_handler()?;

    let cli = Cli::parse();
    // Keep build identification usable without HOME, even during recovery or
    // when profile paths/configuration are broken. No sound or native probes.
    if let Some(Commands::About { json }) = &cli.command {
        return Ok(cli::about::handle(*json)?);
    }
    if let Some(Commands::Prompt(options)) = &cli.command {
        if options.is_catalog() {
            return Ok(cli::prompt::print_catalog_options(options)?);
        }
    }
    // Completion generation is independent of HOME, config, locks and sound.
    if let Some(Commands::Completions { shell }) = &cli.command {
        return Ok(completion::print(*shell, Cli::command())?);
    }
    let prepared_import = if let Some(Commands::Import { uri, dry_run, json }) = &cli.command {
        if *dry_run {
            return Ok(cli::share::handle_import_preview(uri, *json)?);
        }
        Some(cli::share::prepare_import(uri)?)
    } else {
        None
    };
    // Invalid selection/retry/search input needs no profile paths, lock or sound.
    // Repeat this pure check in public handlers for callers bypassing the CLI.
    match &cli.command {
        Some(Commands::Tools(options)) => cli::tools::validate_options(options)?,
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Set { key, value },
        }) => {
            cli::config::validate_set(key, value)?;
            if key == "auto-theme" && value == "configure" {
                cli::auto_theme::require_interactive_configuration()?;
            }
        }
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Pairing(options),
        }) => options.validate()?,
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Get { key, .. },
        }) => cli::config::validate_key(key)?,
        Some(Commands::Setup { quick, only, .. }) => {
            cli::setup::validate_entry(*quick, only.as_deref())?;
        }
        Some(Commands::Set { theme }) => {
            cli::theme::validate_selection(theme.as_deref(), cli.auto)?;
        }
        Some(Commands::Theme { list, args }) => {
            let name = cli::theme::theme_name_argument(*list, args)?;
            if !list {
                cli::theme::validate_selection(name, cli.auto)?;
            }
        }
        Some(Commands::Font {
            search: Some(query),
            ..
        }) => {
            cli::font::validate_list_query(query)?;
        }
        _ => {}
    }

    // seat cliclack's global theme before any command
    // handler runs. Brand events intentionally do NOT pre-seat the
    // default sink here — `dispatch()` self-initializes with NoopSink,
    // and leaving the slot untouched preserves chance to
    // register a real sink before the first dispatch.
    brand::cliclack_theme::init();

    // Initialize SlateEnv from process environment early
    let env = SlateEnv::from_process()?;
    // First-run consent belongs only to the hub. Redirected and machine-readable
    // commands keep their existing output contract and need no locale preference.
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        cli::load_saved_ui_language(&env)?;
    }

    // Preference reads bypass writers, sound and native detection. Pairing owns
    // its single-file writer only when saving; inspection/preview stay read-only.
    match &cli.command {
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Set { key, value },
        }) if key == "auto-theme" && value == "configure" => {
            // Browsing pairing choices is read-only. The save operation owns
            // its writer; do not initialize config or sound before consent.
            return finish_command(cli::auto_theme::configure_auto_theme());
        }
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Pairing(options),
        }) => return Ok(cli::config::pairing::handle(&env, options)?),
        Some(Commands::Config {
            subcommand: ConfigSubcommand::Get { key, json },
        }) => return Ok(cli::config::handle_inspect(&env, Some(key), *json)?),
        Some(Commands::Config {
            subcommand: ConfigSubcommand::List { json },
        }) => return Ok(cli::config::handle_inspect(&env, None, *json)?),
        _ => {}
    }

    // A prepared font preview has no writer, recovery-journal or sound setup.
    // Keep it reachable even when mutation is blocked by another writer.
    if let Some(Commands::Font {
        name: Some(name),
        dry_run: true,
        json,
        ..
    }) = &cli.command
    {
        return Ok(cli::font::handle_preview(&env, name, *json)?);
    }

    if matches!(&cli.command, Some(Commands::Clean { dry_run: false, .. })) {
        cli::clean::validate_storage_paths(&env)?;
    }
    if prepared_import.is_some() {
        cli::share::validate_import_storage_paths(&env)?;
    }
    let font_mutation = matches!(
        &cli.command,
        Some(Commands::Font {
            list: false,
            dry_run: false,
            ..
        })
    );
    if font_mutation {
        cli::font::validate_storage_paths(&env)?;
    }

    // Acquire before sound unpacking, config initialization, or subprocesses.
    // Keep this guard alive for the entire direct mutation, including nested
    // coordinator calls and failure/rollback handling.
    let _write_guard = if needs_write_guard(cli.command.as_ref(), cli.auto) {
        Some(slate_cli::config::ConfigWriteGuard::acquire(&env)?)
    } else {
        None
    };

    // register the EventSink seam with the real SoundSink
    // implementation. Must precede any brand-event dispatch (Pitfall 5).
    // `auto || quiet || config.sound=off || cache-unpack-fail` internally
    // degrades to NoopSink — this call never panics and never surfaces errors.
    // Diagnostics and opening the hub must remain read-only, including sound
    // initialization. Merely viewing/canceling the menu creates no cache.
    let read_only = matches!(
        &cli.command,
        Some(Commands::Doctor { .. })
            | Some(Commands::List(_) | Commands::Export { .. } | Commands::Aura)
            | Some(Commands::Theme { list: true, .. })
            | Some(Commands::Font { list: true, .. })
            | Some(Commands::Status { .. })
            | Some(Commands::Tools(_))
            | Some(Commands::Prompt(_))
            | Some(Commands::Recover { .. })
            | Some(Commands::WatchAutoTheme)
            | Some(Commands::Clean { dry_run: true, .. })
            | Some(Commands::Restore { dry_run: true, .. })
            | Some(Commands::Restore { list: true, .. })
            | Some(Commands::Restore {
                id: None,
                delete: None,
                ..
            })
    ) || (!needs_write_guard(cli.command.as_ref(), cli.auto)
        && cli::recover::has_pending_preview(&env));
    // Import captures its recovery point before any sound preference/cache IO.
    if prepared_import.is_none() && !font_mutation && cli.command.is_some() {
        brand::SoundSink::install(&env, cli.auto, cli.quiet || read_only);
    }

    let result = match cli.command {
        None => {
            // Bare `slate` invocation routes to hub
            cli::hub::handle_with_options(cli.auto, cli.quiet)
        }
        Some(Commands::Setup { quick, force, only }) => {
            cli::setup::handle_with_env(quick, force, only, &env)
        }
        Some(Commands::Tools(options)) => cli::tools::handle(&env, &options),
        Some(Commands::Prompt(options)) => cli::prompt::handle(&env, &options),
        Some(Commands::Set { theme }) => cli::set::handle(theme.as_deref(), cli.auto, cli.quiet),
        Some(Commands::Theme { list, args }) => {
            handle_theme_command(list, args, cli.auto, cli.quiet)
        }
        Some(Commands::Font {
            name,
            list,
            json,
            search,
            ..
        }) => {
            if list {
                cli::font::handle_list_with_query(&env, json, search.as_deref())
            } else {
                cli::font::handle_font_with_env(name.as_deref(), &env, cli.auto, cli.quiet)
            }
        }
        Some(Commands::Config { subcommand }) => match subcommand {
            ConfigSubcommand::Set { key, value } => cli::config::handle_config_set(&key, &value),
            ConfigSubcommand::Get { .. }
            | ConfigSubcommand::List { .. }
            | ConfigSubcommand::Pairing(_) => {
                unreachable!("preference reads and pairing handled before shared initialization")
            }
        },
        Some(Commands::Status { json }) => cli::status::handle(json),
        Some(Commands::Doctor {
            target,
            json,
            check_version,
            files_only,
        }) => cli::doctor::handle_with_options(target.as_deref(), json, check_version, files_only),
        Some(Commands::List(options)) => cli::list::handle_with_options(&options),
        Some(Commands::Completions { .. } | Commands::About { .. }) => {
            unreachable!("handled before profile initialization")
        }
        Some(Commands::Clean { dry_run, json }) => {
            if dry_run {
                cli::clean::handle_preview(&env, json)
            } else {
                cli::clean::handle_clean()
            }
        }
        Some(Commands::Recover {
            dry_run,
            json,
            yes,
            discard,
            export,
        }) => cli::recover::handle(&env, dry_run, json, yes, discard, export.as_deref()),
        Some(Commands::Restore {
            id,
            list,
            all,
            delete,
            dry_run,
            json,
        }) => {
            if list {
                cli::restore::handle_list_with_options(json, all)
            } else if dry_run {
                cli::restore::handle_preview(id.as_deref().expect("clap requires an ID"), json)
            } else {
                cli::restore::handle(id.as_deref(), list, delete.as_deref())
            }
        }
        Some(Commands::Share) => cli::share_screenshot::handle_share(),
        Some(Commands::Export { raw }) => cli::share::handle_export_with_options(raw),
        Some(Commands::Import { .. }) => cli::share::handle_prepared_import_with_options(
            prepared_import.expect("imports are prepared before profile initialization"),
            cli.auto,
            cli.quiet,
        ),
        Some(Commands::Aura) => cli::aura::handle(),
        Some(Commands::WatchAutoTheme) => cli::watch::handle_auto_theme_watch(),
        Some(Commands::Reset { id }) => {
            // reset is now a compatibility alias that routes to restore
            println!("(i) Tip: 'slate reset' is transitioning to 'slate restore'. Use 'slate restore [id]' next time.");
            println!();
            cli::restore::handle(id.as_deref(), false, None)
        }
    };

    finish_command(result)
}

fn finish_command(result: error::Result<()>) -> Result<()> {
    // Unified cancellation handling, including read-only interactive routes.
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
    let name = cli::theme::theme_name_argument(list, &args)?;
    if list {
        return cli::list::handle(&[]);
    }
    cli::theme::handle_theme(name.map(str::to_owned), auto, quiet)
}
