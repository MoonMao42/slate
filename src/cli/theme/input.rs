use crate::error::{Result, SlateError};
use crate::theme::ThemeRegistry;

/// Shared syntax boundary for CLI preflight and compatibility dispatch.
pub fn theme_name_argument(list: bool, args: &[String]) -> Result<Option<&str>> {
    if list {
        return if args.is_empty() {
            Ok(None)
        } else {
            Err(SlateError::InvalidConfig(
                "`slate theme --list` does not accept a theme argument. Use `slate list <query>` to search.".into(),
            ))
        };
    }
    match args {
        [] => Ok(None),
        [verb] if verb == "set" => Err(SlateError::InvalidConfig(
            "Missing theme after `slate theme set`. Use `slate theme` for the picker or `slate theme set <theme>`.".into(),
        )),
        [name] => Ok(Some(name)),
        [verb, name] if verb == "set" => Ok(Some(name)),
        _ => Err(SlateError::InvalidConfig(
            "Use `slate theme <theme>`, `slate theme set <theme>`, or `slate theme --list`. Quote display names containing spaces.".into(),
        )),
    }
}

/// Validate without constructing paths, writing a lock, reading preferences,
/// initializing sound, or probing system appearance.
pub fn validate_selection(name: Option<&str>, auto: bool) -> Result<()> {
    if let Some(name) = name {
        if auto {
            return Err(SlateError::InvalidConfig(
                "A theme name cannot be combined with `--auto`. Choose `slate theme <theme>` or `slate theme --auto`; no theme was applied.".into(),
            ));
        }
        ThemeRegistry::new()?.require_by_id_or_name(name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_input_shared_validation_covers_selection_and_compatibility_syntax() {
        for args in [
            vec!["theme", "extra"],
            vec!["set"],
            vec!["set", "nord", "extra"],
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(theme_name_argument(false, &args).is_err());
        }
        assert!(theme_name_argument(true, &["nord".into()]).is_err());
        assert!(theme_name_argument(true, &[]).unwrap().is_none());
        assert_eq!(
            theme_name_argument(false, &["set".into(), "nord".into()]).unwrap(),
            Some("nord")
        );
        assert!(validate_selection(None, true).is_ok());
        assert!(validate_selection(Some("Rosé Pine Dawn"), false).is_ok());
        for auto in [false, true] {
            assert!(validate_selection(Some("nrod"), auto).is_err());
        }
    }
}
