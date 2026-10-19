#[cfg(target_os = "macos")]
use super::*;

#[cfg(target_os = "macos")]
fn play_count(log: &std::path::Path) -> usize {
    fs::read_to_string(log)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn saved_sound_enabled(env: &SlateEnv) -> bool {
    let config: toml::Value = fs::read_to_string(env.managed_file("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    config["preferences"]["sound"].as_bool().unwrap()
}

#[cfg(target_os = "macos")]
fn wait_for_play(log: &std::path::Path, minimum: usize) {
    let started = Instant::now();
    while play_count(log) < minimum {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "afplay was not called"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
fn visit_picker_and_move(hub: &mut PickerProcess) {
    let keys = menu_choice_keys(hub, "想调整什么？", "切换主题");
    navigate(hub, &keys, "s 保存配对");
    navigate(hub, b"j", "s 保存配对");
    navigate(hub, b"q", "想调整什么？");
}

#[cfg(target_os = "macos")]
fn toggle_sound(hub: &mut PickerProcess, label: &str) {
    let keys = menu_choice_keys(hub, "想调整什么？", "终端偏好");
    navigate(hub, &keys, label);
    let keys = menu_choice_keys(hub, "终端偏好", label);
    navigate(hub, &keys, "● 返回首页");
    navigate(hub, b"\r", "想调整什么？");
}

#[cfg(target_os = "macos")]
fn start_hub_with_fake_afplay(
    enabled: bool,
) -> (TempDir, SlateEnv, PickerProcess, std::path::PathBuf) {
    let (home, env) = queued_picker_fixture();
    fs::write(
        env.managed_file("config.toml"),
        format!("[preferences]\nsound = {enabled}\n"),
    )
    .unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let afplay = bin.join("afplay");
    fs::write(
        &afplay,
        "#!/bin/sh\nprintf 'play\\n' >> \"$HOME/sound.log\"\n",
    )
    .unwrap();
    fs::set_permissions(&afplay, fs::Permissions::from_mode(0o755)).unwrap();
    let log = home.path().join("sound.log");

    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&bin));
    hub.wait_for_output("想调整什么？");
    assert!(!env.slate_cache_dir().join("sounds").exists());
    (home, env, hub, log)
}

#[cfg(target_os = "macos")]
#[test]
fn hub_sound_toggle_updates_the_installed_sink_both_ways() {
    let (_home, env, mut hub, log) = start_hub_with_fake_afplay(true);

    visit_picker_and_move(&mut hub);
    wait_for_play(&log, 1);
    let before_mute = play_count(&log);

    toggle_sound(&mut hub, "操作音效：开");
    assert!(!saved_sound_enabled(&env));
    visit_picker_and_move(&mut hub);
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        play_count(&log),
        before_mute,
        "muted Hub still called afplay"
    );

    toggle_sound(&mut hub, "操作音效：关");
    assert!(saved_sound_enabled(&env));
    visit_picker_and_move(&mut hub);
    wait_for_play(&log, before_mute + 1);

    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
}

#[cfg(target_os = "macos")]
#[test]
fn hub_initially_muted_sink_can_be_enabled_without_restart() {
    let (_home, env, mut hub, log) = start_hub_with_fake_afplay(false);
    visit_picker_and_move(&mut hub);
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(play_count(&log), 0);
    assert!(!env.slate_cache_dir().join("sounds").exists());

    toggle_sound(&mut hub, "操作音效：关");
    assert!(env.slate_cache_dir().join("sounds/hero.wav").is_file());
    visit_picker_and_move(&mut hub);
    wait_for_play(&log, 1);

    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
}
