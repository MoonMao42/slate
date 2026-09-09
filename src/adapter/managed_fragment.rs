//! Single managed-asset publication; never initializes unrelated profile state.
use crate::{
    config::{
        file_read::{self, Links, MAX_TOOL_CONFIG_BYTES},
        recovery_paths,
        state_files::atomic_write_synced_mode,
    },
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::path::Path;

pub(super) fn write(env: &SlateEnv, path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let invalid = |reason: &str| {
        SlateError::InvalidConfig(format!("Cannot update {label} fragment: {reason}"))
    };
    if !path.starts_with(env.managed_file("managed")) || bytes.len() as u64 > MAX_TOOL_CONFIG_BYTES
    {
        return Err(invalid(
            "target is outside managed storage or output exceeds 8 MiB",
        ));
    }
    recovery_paths::validate_file_path(env, path, label)?;
    let destination = file_read::directory_alias_target(path)
        .ok_or_else(|| invalid("cannot resolve destination"))?;
    let read = || {
        file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
            .map_err(|_| invalid("existing file is unsafe, unreadable or exceeds 8 MiB"))
    };
    let original = read()?;
    if original
        .as_ref()
        .is_some_and(|source| source.bytes == bytes)
    {
        return Ok(());
    }
    let verify = || -> Result<()> {
        recovery_paths::validate_file_path(env, path, label)?;
        if file_read::directory_alias_target(path).as_ref() != Some(&destination)
            || read()? != original
        {
            return Err(invalid(
                "file or destination changed while preparing; retry without overwriting the edit",
            ));
        }
        Ok(())
    };
    verify()?;
    std::fs::create_dir_all(path.parent().expect("validated absolute managed file"))?;
    verify()?;
    atomic_write_synced_mode(
        path,
        bytes,
        original.as_ref().and_then(|source| source.mode),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapter::{EzaAdapter, FastfetchAdapter, LazygitAdapter, ToolAdapter, ZshHighlightAdapter},
        theme::ThemeRegistry,
    };
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    };

    #[test]
    fn managed_fragments_preserve_permissions_and_noop_identity_without_profile_initialization() {
        for adapter in [
            &EzaAdapter as &dyn ToolAdapter,
            &LazygitAdapter,
            &FastfetchAdapter,
            &ZshHighlightAdapter,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(temp.path().to_owned());
            let path = match adapter.tool_name() {
                "eza" => EzaAdapter::theme_path(&env),
                "fastfetch" => FastfetchAdapter::theme_path(&env),
                "zsh-syntax-highlighting" => ZshHighlightAdapter::theme_path(&env),
                _ => LazygitAdapter::theme_path(&env),
            };
            let themes = ThemeRegistry::new().unwrap();
            adapter
                .apply_theme_with_env(themes.get("nord").unwrap(), &env)
                .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let before = fs::metadata(&path).unwrap();
            adapter
                .apply_theme_with_env(themes.get("nord").unwrap(), &env)
                .unwrap();
            let after = fs::metadata(&path).unwrap();
            assert_eq!(before.ino(), after.ino());
            assert_eq!(before.modified().unwrap(), after.modified().unwrap());
            adapter
                .apply_theme_with_env(themes.get("catppuccin-latte").unwrap(), &env)
                .unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
            assert!(!env.managed_file("current").exists());
            assert!(!env.managed_file("config.toml").exists());
            assert!(!env.slate_cache_dir().exists());
        }
    }

    #[test]
    fn managed_fragment_writer_rejects_unsafe_paths_and_size_limits() {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().join("home"));
        let path = EzaAdapter::theme_path(&env);
        assert!(write(
            &env,
            &path,
            &vec![0; MAX_TOOL_CONFIG_BYTES as usize + 1],
            "eza"
        )
        .is_err());
        assert!(write(&env, &env.home().join("personal"), b"no", "eza").is_err());
        assert!(!env.home().exists());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let outside = temp.path().join("personal.yml");
        fs::write(&outside, "PRIVATE").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(write(&env, &path, b"no", "eza").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"PRIVATE");
        fs::remove_file(&path).unwrap();
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_TOOL_CONFIG_BYTES + 1)
            .unwrap();
        assert!(write(&env, &path, b"no", "eza").is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            MAX_TOOL_CONFIG_BYTES + 1
        );
        let isolated = SlateEnv::with_home(temp.path().join("isolated"));
        fs::create_dir(isolated.home()).unwrap();
        symlink(env.xdg_config_home(), isolated.xdg_config_home()).unwrap();
        assert!(write(&isolated, &EzaAdapter::theme_path(&isolated), b"no", "eza").is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            MAX_TOOL_CONFIG_BYTES + 1
        );
    }
}
