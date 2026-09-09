//! UI language only: never changes shell locale, theme, command IDs or JSON keys.
use super::{
    file_read::{self, Links, MAX_DOCUMENT_BYTES},
    flags, recovery_paths, ConfigWriteGuard,
};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
};
use toml_edit::DocumentMut;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLanguage {
    Chinese,
    English,
}

impl UiLanguage {
    pub fn id(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "zh-CN" => Ok(Self::Chinese),
            "en" => Ok(Self::English),
            _ => Err(SlateError::InvalidConfig(
                "UI language must be zh-CN or en".into(),
            )),
        }
    }
}

/// Every migrated message supplies both versions at its definition site.
pub struct Text {
    pub zh: &'static str,
    pub en: &'static str,
}

impl Text {
    pub fn get(&self, language: UiLanguage) -> &'static str {
        match language {
            UiLanguage::Chinese => self.zh,
            UiLanguage::English => self.en,
        }
    }
}

fn selected(doc: &DocumentMut) -> Result<Option<UiLanguage>> {
    let Some(preferences) = doc.get("preferences") else {
        return Ok(None);
    };
    let table = preferences.as_table_like().ok_or_else(|| {
        SlateError::InvalidConfig("config.toml [preferences] must be a table".into())
    })?;
    table
        .get("language")
        .map(|item| {
            item.as_str()
                .ok_or_else(|| {
                    SlateError::InvalidConfig(
                        "config.toml [preferences].language must be a string".into(),
                    )
                })
                .and_then(UiLanguage::parse)
        })
        .transpose()
}

fn capture(env: &SlateEnv) -> Result<(Option<file_read::Source>, DocumentMut)> {
    let path = env.managed_file("config.toml");
    recovery_paths::validate_file_path(env, &path, "UI language")?;
    let source = file_read::read(&path, MAX_DOCUMENT_BYTES, Links::Reject)
        .map_err(|error| SlateError::ConfigReadError(format!("{path:?}"), error.to_string()))?;
    let doc = match &source {
        Some(source) => flags::parse_document(
            &path,
            std::str::from_utf8(&source.bytes)
                .map_err(|_| SlateError::InvalidConfig("config.toml is not UTF-8".into()))?,
        )?,
        None => DocumentMut::default(),
    };
    Ok((source, doc))
}

/// Missing is distinct from unreadable: first-run prompting must not replace
/// an invalid saved preference. Reads never create files or acquire a writer.
pub fn read(env: &SlateEnv) -> Result<Option<UiLanguage>> {
    let (_, doc) = capture(env)?;
    selected(&doc)
}

// Only the language value is replaceable through an explicit choice. Broken
// documents, unsafe files and malformed preference tables still fail closed.
fn editable_selection(doc: &DocumentMut) -> Result<Option<UiLanguage>> {
    if let Some(preferences) = doc.get("preferences") {
        let table = preferences.as_table_like().ok_or_else(|| {
            SlateError::InvalidConfig("config.toml [preferences] must be a table".into())
        })?;
        if table.get("language").is_some_and(|item| !item.is_value()) {
            return Err(SlateError::InvalidConfig(
                "config.toml language must be a value, not a table".into(),
            ));
        }
    }
    Ok(selected(doc).ok().flatten())
}

/// Snapshot the choice before showing its menu, without taking a write lock.
pub struct LanguageChoice {
    env: SlateEnv,
    source: Option<file_read::Source>,
    selected: Option<UiLanguage>,
}

impl LanguageChoice {
    pub fn selected(&self) -> Option<UiLanguage> {
        self.selected
    }

    pub fn save(self, language: UiLanguage) -> Result<()> {
        save_captured(&self.env, self.source, language)
    }
}

pub fn prepare_choice(env: &SlateEnv) -> Result<LanguageChoice> {
    let (source, doc) = capture(env)?;
    Ok(LanguageChoice {
        env: env.clone(),
        source,
        selected: editable_selection(&doc)?,
    })
}

/// Call only after an explicit language choice. Preserve unrelated TOML and
/// reject stale input; cooperative locking does not lock out external editors.
pub fn save(env: &SlateEnv, language: UiLanguage) -> Result<()> {
    prepare_choice(env)?.save(language)
}

fn save_captured(
    env: &SlateEnv,
    expected: Option<file_read::Source>,
    language: UiLanguage,
) -> Result<()> {
    let (initial, initial_doc) = capture(env)?;
    if initial != expected {
        return Err(SlateError::InvalidConfig(
            "Configuration changed; choose the language again".into(),
        ));
    }
    if editable_selection(&initial_doc)? == Some(language) {
        return Ok(());
    }
    let _guard = ConfigWriteGuard::acquire(env)?;
    let (current, mut doc) = capture(env)?;
    if current != initial {
        return Err(SlateError::InvalidConfig(
            "Configuration changed; choose the language again".into(),
        ));
    }
    if !doc.contains_key("preferences") {
        doc.insert("preferences", toml_edit::table());
    }
    let table = doc
        .get_mut("preferences")
        .and_then(|item| item.as_table_like_mut())
        .ok_or_else(|| {
            SlateError::InvalidConfig("config.toml [preferences] must be a table".into())
        })?;
    flags::set_value(table, "language", toml_edit::Value::from(language.id()));
    std::fs::create_dir_all(env.config_dir())?;
    super::state_files::atomic_write_synced_mode(
        &env.managed_file("config.toml"),
        doc.to_string().as_bytes(),
        current
            .as_ref()
            .and_then(|source| source.mode)
            .or(Some(0o600)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_choice_rejects_changes_while_menu_is_open() {
        for initial in [None, Some("[preferences]\nlanguage = 'zh-CN'\n")] {
            for replacement in [
                "[preferences]\nlanguage = 'en'\n",
                "# personal edit\n[preferences]\nlanguage = 'zh-CN'\n",
            ] {
                let home = tempfile::tempdir().unwrap();
                let env = SlateEnv::with_home(home.path().into());
                let path = env.managed_file("config.toml");
                if let Some(initial) = initial {
                    std::fs::create_dir_all(env.config_dir()).unwrap();
                    std::fs::write(&path, initial).unwrap();
                }
                let choice = prepare_choice(&env).unwrap();
                std::fs::create_dir_all(env.config_dir()).unwrap();
                std::fs::write(&path, replacement).unwrap();
                assert!(choice
                    .save(UiLanguage::Chinese)
                    .unwrap_err()
                    .to_string()
                    .contains("Configuration changed"));
                assert_eq!(std::fs::read_to_string(&path).unwrap(), replacement);
                assert!(!env.slate_cache_dir().exists());
            }
        }
    }

    #[test]
    fn language_choice_busy_failure_preserves_invalid_value() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        std::fs::create_dir_all(env.config_dir()).unwrap();
        let path = env.managed_file("config.toml");
        let original = "[preferences]\nlanguage = 42\n";
        std::fs::write(&path, original).unwrap();
        let choice = prepare_choice(&env).unwrap();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let other_env = env.clone();
        let thread = std::thread::spawn(move || {
            let _guard = ConfigWriteGuard::acquire(&other_env).unwrap();
            held_tx.send(()).unwrap();
            release_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        });
        held_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let result = choice.save(UiLanguage::English);
        release_tx.send(()).unwrap();
        thread.join().unwrap();
        assert!(matches!(result, Err(SlateError::ConfigurationBusy)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        prepare_choice(&env)
            .unwrap()
            .save(UiLanguage::English)
            .unwrap();
        assert_eq!(read(&env).unwrap(), Some(UiLanguage::English));
    }

    #[test]
    fn language_round_trip_preserves_other_preferences() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        assert_eq!(read(&env).unwrap(), None);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        std::fs::create_dir_all(env.config_dir()).unwrap();
        let path = env.managed_file("config.toml");
        std::fs::write(
            &path,
            "# personal\n[preferences]\nsound = false # retain\n[prompt]\nstyle = 'focus'\n",
        )
        .unwrap();
        for language in [UiLanguage::English, UiLanguage::Chinese] {
            save(&env, language).unwrap();
            assert_eq!(read(&env).unwrap(), Some(language));
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(
                content.contains("# personal")
                    && content.contains("sound = false # retain")
                    && content.contains("style = 'focus'")
            );
            let before = std::fs::metadata(&path).unwrap().modified().unwrap();
            save(&env, language).unwrap();
            assert_eq!(
                std::fs::metadata(&path).unwrap().modified().unwrap(),
                before
            );
        }
    }
    #[test]
    fn invalid_language_is_not_silently_replaced() {
        for text in [
            "[preferences]\nlanguage = 'xx'",
            "[preferences]\nlanguage = 42",
            "preferences = false",
            "[broken",
        ] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().into());
            std::fs::create_dir_all(env.config_dir()).unwrap();
            let path = env.managed_file("config.toml");
            std::fs::write(&path, text).unwrap();
            assert!(read(&env).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
            assert!(!env.slate_cache_dir().exists());
        }
    }

    #[test]
    fn language_choice_repairs_only_invalid_values_after_explicit_save() {
        for value in ["'xx'", "42", "false", "['en']"] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().into());
            std::fs::create_dir_all(env.config_dir()).unwrap();
            let path = env.managed_file("config.toml");
            let original = format!("# personal\n[preferences]\nlanguage = {value} # keep comment\nsound = false\n[prompt]\nstyle = 'focus'\n");
            std::fs::write(&path, &original).unwrap();
            assert!(read(&env).is_err());
            assert_eq!(prepare_choice(&env).unwrap().selected(), None);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            assert!(!env.slate_cache_dir().exists());
            save(&env, UiLanguage::English).unwrap();
            assert_eq!(read(&env).unwrap(), Some(UiLanguage::English));
            let saved = std::fs::read_to_string(&path).unwrap();
            assert!(saved.contains("# personal") && saved.contains("# keep comment"));
            assert!(saved.contains("sound = false") && saved.contains("style = 'focus'"));
        }
        for original in [
            "preferences = false",
            "[broken",
            "[preferences.language]\npersonal = true",
        ] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().into());
            std::fs::create_dir_all(env.config_dir()).unwrap();
            let path = env.managed_file("config.toml");
            std::fs::write(&path, original).unwrap();
            assert!(prepare_choice(&env).is_err());
            assert!(save(&env, UiLanguage::English).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            assert!(!env.slate_cache_dir().exists());
        }
    }

    #[test]
    fn language_save_respects_pending_recovery_and_unsafe_files() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        std::fs::create_dir_all(env.slate_cache_dir()).unwrap();
        std::fs::write(
            env.slate_cache_dir().join("preview-session.json"),
            "pending",
        )
        .unwrap();
        assert!(matches!(
            save(&env, UiLanguage::English),
            Err(SlateError::PreviewRecoveryPending)
        ));
        assert!(!env.config_dir().exists());

        for linked in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().into());
            std::fs::create_dir_all(env.config_dir()).unwrap();
            let path = env.managed_file("config.toml");
            if linked {
                let target = home.path().join("personal.toml");
                std::fs::write(&target, "# untouched\n").unwrap();
                std::os::unix::fs::symlink(&target, &path).unwrap();
            } else {
                std::fs::create_dir(&path).unwrap();
            }
            assert!(read(&env).is_err());
            assert!(save(&env, UiLanguage::Chinese).is_err());
            assert!(!env.slate_cache_dir().exists());
            if linked {
                assert_eq!(
                    std::fs::read_to_string(home.path().join("personal.toml")).unwrap(),
                    "# untouched\n"
                );
            }
        }
    }
}
