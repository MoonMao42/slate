use super::{normalize_theme_lookup, ThemeRegistry, ThemeVariant};

impl ThemeRegistry {
    /// Search IDs, display names and families, keeping the embedded order.
    /// Terms are ANDed; matching ignores case, accents and word separators.
    /// Discovery only: this does not change exact lookup or apply a theme.
    /// Interactive callers must confirm a selected catalog ID, not the query.
    pub fn search(&self, query: &str) -> Vec<&ThemeVariant> {
        if query.trim().is_empty() {
            return self.all();
        }
        let normalized = normalize_theme_lookup(query);
        let terms: Vec<_> = normalized
            .split('-')
            .filter(|term| !term.is_empty())
            .collect();
        if terms.is_empty() {
            return Vec::new();
        }
        self.embedded
            .all()
            .filter(|theme| {
                let fields = normalize_theme_lookup(&format!(
                    "{} {} {}",
                    theme.id, theme.name, theme.family
                ));
                terms.iter().all(|term| fields.contains(term))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_search_normalizes_terms_without_changing_exact_lookup() {
        let registry = ThemeRegistry::new().unwrap();
        for query in [
            "rose dawn",
            "DAWN Rosé",
            "rose_pine_dawn",
            "  Rosé Pine Dawn  ",
        ] {
            let found = registry.search(query);
            assert_eq!(found.len(), 1, "{query}");
            assert_eq!(found[0].id, "rose-pine-dawn");
        }
        assert_eq!(registry.search("catp").len(), 4);
        assert_eq!(registry.search("  ").len(), registry.all().len());
        for query in ["no-such-theme", "🌌", "---"] {
            assert!(registry.search(query).is_empty(), "{query}");
        }
        assert!(registry.get_by_id_or_name("catp").is_none());
        assert!(registry.get_by_id_or_name("rose dawn").is_none());
        assert_eq!(
            registry.get_by_id_or_name("Rosé Pine Dawn").unwrap().id,
            "rose-pine-dawn"
        );
    }
}
