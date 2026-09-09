use super::{normalize_theme_lookup, ThemeRegistry, ThemeVariant};
use crate::error::{Result, SlateError};
use crate::lookup::close_distance;

impl ThemeRegistry {
    /// Resolve only a complete ID/display name. Suggestions never pick a theme.
    pub fn require_by_id_or_name(&self, query: &str) -> Result<&ThemeVariant> {
        self.get_by_id_or_name(query).ok_or_else(|| {
            let suggestions = self.suggest_ids(query);
            let mut guidance = if suggestions.is_empty() {
                String::new()
            } else {
                format!("Suggested IDs: {}.\n", suggestions.join(", "))
            };
            guidance.push_str("Run `slate list` to search available themes, then use a full ID or quoted display name. No theme was applied.");
            // Bound and debug-escape echoed input, including newlines, terminal
            // control codes and invisible formatting characters.
            let mut preview: String = query.chars().take(80).collect();
            if query.chars().nth(80).is_some() {
                preview.push_str("...");
            }
            SlateError::ThemeLookupFailed {
                input: format!("{preview:?}"),
                guidance,
            }
        })
    }

    /// At most three deterministic, advisory IDs. Prefer substring/term matches,
    /// then conservative edit-distance matches (including adjacent transposition).
    pub fn suggest_ids(&self, query: &str) -> Vec<&str> {
        // Bound work before normalization or edit-distance allocation. Very short
        // inputs produce noisy catalog matches and are better served by `list`.
        if query.chars().take(65).count() > 64 {
            return Vec::new();
        }
        let normalized = normalize_theme_lookup(query);
        let query_chars: Vec<_> = normalized.chars().collect();
        if query_chars.len() < 3 || query_chars.len() > 64 {
            return Vec::new();
        }
        let matches = self.search(query);
        if !matches.is_empty() {
            return matches
                .into_iter()
                .take(3)
                .map(|theme| theme.id.as_str())
                .collect();
        }

        let threshold = match query_chars.len() {
            0..=4 => 1,
            5..=11 => 2,
            _ => 3,
        };
        let mut candidates: Vec<_> = self
            .embedded
            .all()
            .filter_map(|theme| {
                let distance = [&theme.id, &theme.name]
                    .into_iter()
                    .filter_map(|name| {
                        close_distance(&query_chars, &normalize_theme_lookup(name), threshold)
                    })
                    .min()?;
                Some((distance, theme.id.as_str()))
            })
            .collect();
        candidates.sort_unstable();
        candidates.into_iter().take(3).map(|(_, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_input_suggestions_are_bounded_advisory_and_deterministic() {
        for (query, candidate, expected) in [
            ("nrd", "nord", Some(1)),
            ("noord", "nord", Some(1)),
            ("nrod", "nord", Some(1)),
            ("nart", "nord", Some(2)),
            ("日木", "日本", Some(1)),
            ("", "nord", None),
        ] {
            assert_eq!(
                close_distance(&query.chars().collect::<Vec<_>>(), candidate, 2),
                expected
            );
        }
        let registry = ThemeRegistry::new().unwrap();
        for (query, expected) in [
            ("catppuccin-mocah", "catppuccin-mocha"),
            ("nrod", "nord"),
            ("solarised light", "solarized-light"),
            ("rose dawn", "rose-pine-dawn"),
        ] {
            assert_eq!(registry.suggest_ids(query)[0], expected, "{query}");
            assert!(
                registry.require_by_id_or_name(query).is_err(),
                "must not auto-correct {query}"
            );
            assert!(registry
                .require_by_id_or_name(query)
                .unwrap_err()
                .to_string()
                .contains(expected));
        }
        assert_eq!(registry.suggest_ids("kanagawa").len(), 3);
        assert_eq!(registry.suggest_ids("catp").len(), 3);
        assert_eq!(registry.suggest_ids("catp"), registry.suggest_ids("catp"));
        for query in ["", "n", "aa", "totally-unrelated", "🌌"] {
            assert!(registry.suggest_ids(query).is_empty(), "{query}");
        }
        assert!(registry.suggest_ids(&"a".repeat(10_000)).is_empty());
        for theme in registry.all() {
            assert_eq!(
                registry.require_by_id_or_name(&theme.id).unwrap().id,
                theme.id
            );
            assert_eq!(
                registry.require_by_id_or_name(&theme.name).unwrap().id,
                theme.id
            );
        }
    }

    #[test]
    fn theme_input_errors_escape_control_sequences_and_limit_echo_length() {
        let registry = ThemeRegistry::new().unwrap();
        let query = format!("\x1b[31m\n\u{202e}{}", "x".repeat(1_000));
        let error = registry
            .require_by_id_or_name(&query)
            .unwrap_err()
            .to_string();
        assert!(!error.contains('\x1b') && !error.contains('\u{202e}'));
        assert!(error.contains("\\n") && error.contains("..."));
        assert!(error.len() < 400, "{error}");
        assert!(error.contains("No theme was applied"));
    }
}
