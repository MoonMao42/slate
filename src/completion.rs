//! Static scripts derived from the actual CLI grammar and embedded catalog.
//! These scripts never invoke Slate during shell startup or Tab completion.

use clap::{builder::PossibleValuesParser, Command, ValueEnum, ValueHint};
use slate_cli::{error::Result, theme::ThemeRegistry};
use std::io::Write;

mod fish;

#[derive(Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

pub fn print(shell: Shell, command: Command) -> Result<()> {
    let mut command = completion_command(command)?;
    let generator = match shell {
        Shell::Bash => clap_complete::Shell::Bash,
        Shell::Zsh => clap_complete::Shell::Zsh,
        Shell::Fish => clap_complete::Shell::Fish,
    };
    // The generator panics on a failed writer. Buffer first so a normal early
    // consumer exit is handled here instead of becoming a panic or error dump.
    let mut output = Vec::new();
    clap_complete::generate(generator, &mut command, "slate", &mut output);
    if matches!(shell, Shell::Fish) {
        fish::append_positionals(&command, &mut output)?;
    }
    match std::io::stdout().lock().write_all(&output) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn completion_command(command: Command) -> Result<Command> {
    let registry = ThemeRegistry::new()?;
    let theme_ids = registry.list_ids();
    // Enrich only the completion copy. The real parser still accepts display
    // names and routes unknown input through the advisory suggestion preflight.
    let command = visible_command(&command).mut_subcommand("set", |cmd| {
        cmd.mut_arg("theme", |arg| {
            arg.value_parser(PossibleValuesParser::new(theme_ids.clone()))
                .conflicts_with("auto")
        })
    });
    // Represent the accepted compatibility form as a real completion branch,
    // so `theme set <ID>` does not suggest a second verb or another theme name.
    let compatibility = command
        .find_subcommand("set")
        .expect("set is a CLI alias")
        .clone()
        .mut_arg("theme", |arg| arg.required(true));
    let command = command
        .mut_subcommand("theme", |cmd| {
            cmd.mut_arg("args", |arg| {
                arg.num_args(0..=1)
                    .value_parser(PossibleValuesParser::new(theme_ids.clone()))
                    .conflicts_with_all(["auto", "list"])
            })
            .subcommand(compatibility)
        })
        .mut_subcommand("list", |cmd| {
            cmd.mut_arg("query", |arg| {
                arg.value_parser(PossibleValuesParser::new(theme_ids))
            })
        })
        .mut_subcommand("recover", |cmd| {
            cmd.mut_arg("export", |arg| arg.value_hint(ValueHint::DirPath))
        })
        .mut_subcommand("tools", |cmd| {
            cmd.mut_subcommand("install", |cmd| {
                cmd.mut_arg("tool", |arg| {
                    arg.value_parser(PossibleValuesParser::new(
                        slate_cli::cli::tools::installable_tools(),
                    ))
                })
            })
            .mut_subcommand("info", |cmd| {
                cmd.mut_arg("tool", |arg| {
                    arg.value_parser(PossibleValuesParser::new(
                        slate_cli::cli::tools::supported_tools(),
                    ))
                })
            })
            .mut_subcommand("sync", |cmd| {
                cmd.mut_arg("tools", |arg| {
                    arg.value_parser(PossibleValuesParser::new(
                        slate_cli::cli::tools::supported_tools(),
                    ))
                })
            })
        })
        .mut_subcommand("doctor", |cmd| {
            cmd.mut_arg("target", |arg| {
                arg.value_parser(PossibleValuesParser::new(slate_cli::cli::doctor::TARGETS))
            })
        })
        .mut_subcommand("config", |cmd| {
            let cmd = cmd.mut_subcommand("pairing", |cmd| {
                cmd.mut_args(|arg| {
                    let appearance = match arg.get_id().as_str() {
                        "dark" => Some(slate_cli::theme::ThemeAppearance::Dark),
                        "light" => Some(slate_cli::theme::ThemeAppearance::Light),
                        _ => None,
                    };
                    if let Some(appearance) = appearance {
                        arg.value_parser(PossibleValuesParser::new(
                            registry
                                .all()
                                .into_iter()
                                .filter(|theme| theme.appearance == appearance)
                                .map(|theme| theme.id.clone())
                                .collect::<Vec<_>>(),
                        ))
                    } else {
                        arg
                    }
                })
            });
            ["get", "set"].into_iter().fold(cmd, |cmd, name| {
                cmd.mut_subcommand(name, |cmd| {
                    // mut_arg removes and re-appends its argument. Preserve
                    // positional order: set must still be <key> <value>.
                    cmd.mut_args(|arg| {
                        if arg.get_id() == "key" {
                            arg.value_parser(PossibleValuesParser::new(
                                slate_cli::cli::config::SETTINGS
                                    .iter()
                                    .map(|setting| setting.key),
                            ))
                        } else {
                            arg
                        }
                    })
                })
            })
        });
    Ok(command)
}

/// Static generators currently enumerate hidden subcommands too. Build a public
/// completion view from the schema instead of copying names/flags into templates.
/// Preserve argument groups and aliases, and let clap add help/global flags.
fn visible_command(command: &Command) -> Command {
    let mut visible = Command::new(command.get_name().to_owned())
        .visible_aliases(command.get_visible_aliases().map(str::to_owned))
        .args(command.get_arguments().cloned())
        .groups(command.get_groups().cloned())
        .subcommands(
            command
                .get_subcommands()
                .filter(|cmd| !cmd.is_hide_set())
                .map(visible_command),
        );
    if let Some(about) = command.get_about() {
        visible = visible.about(about.clone());
    }
    if let Some(version) = command.get_version() {
        visible = visible.version(version.to_owned());
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn completion_schema_tracks_public_arguments_without_restricting_real_input() {
        let source = crate::Cli::command();
        let mut generated = completion_command(source.clone()).unwrap();
        generated.build();
        for subcommand in source.get_subcommands() {
            let copied = generated.find_subcommand(subcommand.get_name());
            if subcommand.is_hide_set() {
                assert!(copied.is_none());
                continue;
            }
            let copied = copied.unwrap();
            for argument in subcommand.get_arguments() {
                let actual = copied
                    .get_arguments()
                    .find(|arg| arg.get_id() == argument.get_id())
                    .unwrap();
                assert_eq!(actual.get_long(), argument.get_long());
                assert_eq!(actual.get_short(), argument.get_short());
            }
            assert!(copied
                .get_arguments()
                .any(|arg| arg.get_long() == Some("quiet")));
            assert!(copied
                .get_arguments()
                .any(|arg| arg.get_long() == Some("auto")));
        }
        let values = generated
            .find_subcommand("set")
            .unwrap()
            .get_arguments()
            .find(|arg| arg.get_id() == "theme")
            .unwrap()
            .get_value_parser()
            .possible_values()
            .unwrap()
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(values, ThemeRegistry::new().unwrap().list_ids());
        let set = generated
            .find_subcommand("config")
            .unwrap()
            .find_subcommand("set")
            .unwrap();
        let positions: Vec<_> = set
            .get_positionals()
            .map(|arg| (arg.get_id().as_str(), arg.get_index()))
            .collect();
        assert_eq!(positions, [("key", Some(1)), ("value", Some(2))]);
        let pairing = generated
            .find_subcommand("config")
            .unwrap()
            .find_subcommand("pairing")
            .unwrap();
        for name in ["clear_dark", "clear_light"] {
            assert!(pairing
                .get_arguments()
                .any(|argument| argument.get_id() == name));
        }
        assert!(crate::Cli::try_parse_from([
            "slate",
            "config",
            "pairing",
            "--clear-dark",
            "--light",
            "catppuccin-latte",
            "--dry-run"
        ])
        .is_ok());
        assert!(crate::Cli::try_parse_from([
            "slate",
            "config",
            "pairing",
            "--clear-dark",
            "--dark",
            "nord"
        ])
        .is_err());
        for (name, appearance) in [
            ("dark", slate_cli::theme::ThemeAppearance::Dark),
            ("light", slate_cli::theme::ThemeAppearance::Light),
        ] {
            let actual: Vec<_> = pairing
                .get_arguments()
                .find(|arg| arg.get_id() == name)
                .unwrap()
                .get_value_parser()
                .possible_values()
                .unwrap()
                .map(|value| value.get_name().to_owned())
                .collect();
            let expected: Vec<_> = ThemeRegistry::new()
                .unwrap()
                .all()
                .into_iter()
                .filter(|theme| theme.appearance == appearance)
                .map(|theme| theme.id.clone())
                .collect();
            assert_eq!(actual, expected);
        }
        let doctor_values = generated
            .find_subcommand("doctor")
            .unwrap()
            .get_arguments()
            .find(|arg| arg.get_id() == "target")
            .unwrap()
            .get_value_parser()
            .possible_values()
            .unwrap()
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(doctor_values, slate_cli::cli::doctor::TARGETS);
        for name in ["Rosé Pine Dawn", "nrod"] {
            assert!(crate::Cli::try_parse_from(["slate", "theme", name]).is_ok());
        }
    }
}
