use super::*;
use std::{fs, os::unix::fs::symlink};

fn fixture() -> (tempfile::TempDir, SlateEnv, PickerState) {
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let config = crate::config::ConfigManager::with_env(&env).unwrap();
    config.set_current_font("JetBrainsMono Nerd Font").unwrap();
    let state = PickerState::new("nord", OpacityPreset::Solid).unwrap();
    (root, env, state)
}

#[test]
fn preview_starship_inputs_preserve_previous_preview_on_read_or_parse_failure() {
    let (_root, env, state) = fixture();
    let source = env.xdg_config_home().join("starship.toml");
    let output = env.managed_file("managed/starship/picker-preview.toml");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, "# previous usable preview\n").unwrap();
    let check = || {
        let error = prepare_preview_starship_config(&state, &env)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("PRIVATE_SOURCE"));
        assert_eq!(fs::read(&output).unwrap(), b"# previous usable preview\n");
    };
    for bytes in [b"format = 'PRIVATE_SOURCE'\n[broken".as_slice(), &[0xff, 0]] {
        fs::write(&source, bytes).unwrap();
        check();
        assert_eq!(fs::read(&source).unwrap(), bytes);
    }
    fs::File::create(&source)
        .unwrap()
        .set_len(crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1)
        .unwrap();
    check();
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    check();
    fs::remove_dir(&source).unwrap();
    symlink(env.home().join("missing"), &source).unwrap();
    check();
}

#[test]
fn preview_starship_output_limit_and_linked_configuration_remain_read_only() {
    let (_root, env, state) = fixture();
    let source = env.home().join("linked-config");
    symlink(&source, env.xdg_config_home().join("starship.toml")).unwrap();
    let original = b"# user custom prompt\nformat = '$directory$character'\n";
    fs::write(&source, original).unwrap();
    let output = prepare_preview_starship_config(&state, &env).unwrap();
    let saved = fs::read(&output).unwrap();
    assert!(String::from_utf8_lossy(&saved).contains("[palettes.slate]"));
    assert_eq!(fs::read(&source).unwrap(), original);
    // Input is exactly allowed; added palette would make the generated file too large.
    let mut large = vec![b'x'; crate::config::file_read::MAX_TOOL_CONFIG_BYTES as usize];
    large[0] = b'#';
    fs::write(&source, &large).unwrap();
    let error = prepare_preview_starship_config(&state, &env)
        .unwrap_err()
        .to_string();
    assert!(error.contains("output exceeds 8 MiB"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), saved);
    assert_eq!(fs::metadata(&source).unwrap().len(), large.len() as u64);
}

#[test]
fn preview_starship_failure_cache_avoids_retries_until_resize() {
    let (_root, env, mut state) = fixture();
    let source = env.xdg_config_home().join("starship.toml");
    fs::write(source, "[PRIVATE_SOURCE_invalid").unwrap();
    let theme = state.get_current_theme_id().to_owned();
    fork_and_cache_prompt(&mut state, &env);
    assert!(state.prompt_attempted(&theme));
    assert!(state.cached_prompt(&theme).is_none());
    let config = crate::config::ConfigManager::with_env(&env).unwrap();
    config.set_starship_enabled(false).unwrap();
    // A retry would now cache the disabled prompt. The prior failure suppresses it.
    fork_and_cache_prompt(&mut state, &env);
    assert!(state.cached_prompt(&theme).is_none());
    state.invalidate_prompt_cache();
    assert!(!state.prompt_attempted(&theme));
    fork_and_cache_prompt(&mut state, &env);
    assert_eq!(
        state.cached_prompt(&theme),
        Some(disabled_prompt_preview().as_str())
    );
}
