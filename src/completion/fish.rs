//! Fill clap_complete's static Fish positional-value gap from the same schema.

use clap::{Arg, Command};
use std::io::{Result, Write};

const HELPER: &str = r#"
# Offer a first positional value only when preceding tokens are exactly the
# command path after parsing its options. An option missing a required value
# must keep its own completion (e.g. list --appearance), not theme candidates.
function __fish_slate_positional
    set -l expected $argv[1]
    set -l conflicts (string split ' ' -- $argv[2])
    set -e argv[1..2]
    set -l specs $argv
    set -l words (commandline -opc)
    set -e words[1]
    argparse $specs -- $words 2>/dev/null
    or return 1
    for flag in $conflicts
        if test -n "$flag"; and set -q "_flag_$flag"
            return 1
        end
    end
    set -l actual (string join ' ' -- $argv)
    test "$actual" = "$expected"
end

"#;

pub(super) fn append_positionals(command: &Command, output: &mut Vec<u8>) -> Result<()> {
    output.write_all(HELPER.as_bytes())?;
    append_command(command, &[], output)
}

fn append_command(command: &Command, path: &[&str], output: &mut Vec<u8>) -> Result<()> {
    if let Some(positional) = command
        .get_positionals()
        .find(|arg| arg.get_index() == Some(1))
    {
        if let Some(values) = positional.get_value_parser().possible_values() {
            let values = values
                .filter(|value| !value.is_hide_set())
                .map(|value| value.get_name().to_owned())
                .collect::<Vec<_>>();
            if !values.is_empty() && positional.get_num_args().expect("built").max_values() == 1 {
                let specs = command
                    .get_arguments()
                    .filter(|arg| !arg.is_positional())
                    .filter_map(option_spec)
                    .map(|spec| quote(&spec))
                    .collect::<Vec<_>>()
                    .join(" ");
                let conflicts = command
                    .get_arg_conflicts_with(positional)
                    .into_iter()
                    .filter_map(|arg| {
                        arg.get_long()
                            .map(str::to_owned)
                            .or_else(|| arg.get_short().map(|short| short.to_string()))
                    })
                    .map(|name| name.replace('-', "_"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let condition = format!(
                    "__fish_slate_positional {} {} {specs}",
                    quote(&path.join(" ")),
                    quote(&conflicts)
                );
                writeln!(
                    output,
                    "complete -c slate -n {} -f -a {}",
                    quote(&condition),
                    quote(&values.join(" "))
                )?;
            }
        }
    }
    for child in command.get_subcommands() {
        let mut child_path = path.to_vec();
        child_path.push(child.get_name());
        append_command(child, &child_path, output)?;
    }
    Ok(())
}

fn option_spec(argument: &Arg) -> Option<String> {
    let mut spec = argument
        .get_short()
        .map(|short| short.to_string())
        .unwrap_or_default();
    if let Some(long) = argument.get_long() {
        if !spec.is_empty() {
            spec.push('/');
        }
        spec.push_str(long);
    }
    if spec.is_empty() {
        return None;
    }
    let range = argument.get_num_args().expect("built");
    if range.takes_values() {
        spec.push('=');
        if range.min_values() == 0 {
            spec.push('?');
        }
    }
    Some(spec)
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn completion_fish_positions_and_option_specs_follow_the_command_tree() {
        let mut command = super::super::completion_command(crate::Cli::command()).unwrap();
        command.build();
        let mut bytes = Vec::new();
        append_positionals(&command, &mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let rules: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("complete "))
            .collect();
        assert_eq!(rules.len(), 11); // includes prompt and tool subcommands
        for path in [
            "set",
            "theme",
            "theme set",
            "list",
            "completions",
            "doctor",
            "config get",
            "config set",
            "prompt",
            "tools info",
            "tools install",
        ] {
            let encoded_path = quote(&quote(path));
            let encoded_path = &encoded_path[1..encoded_path.len() - 1];
            assert!(rules.iter().any(|rule| rule.contains(encoded_path)));
        }
        assert!(text.contains("appearance="));
        assert!(text.contains("bash zsh fish"));
        assert_eq!(quote("a'b\\c"), "'a\\'b\\\\c'");
    }
}
