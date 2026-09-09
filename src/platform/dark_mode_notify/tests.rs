use super::*;
use std::fs;
use std::process::{Child, Stdio};
use std::time::Instant;

#[test]
fn watcher_readiness_requires_current_generation_and_live_lock() {
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let profile = runtime::Profile::new(&env).unwrap();
    assert_eq!(RuntimeInspection::inspect(&env).state, RuntimeState::Absent);
    assert!(
        !profile.directory.exists(),
        "inspection initialized the runtime"
    );
    let snapshot = || {
        fs::read_dir(&profile.directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let check = |state, held| {
        let before = snapshot();
        let report = RuntimeInspection::inspect(&env);
        assert_eq!(report.state, state);
        assert_eq!(report.lock_held, Some(held));
        assert_eq!(snapshot(), before, "inspection modified control files");
    };
    // Only private in-process leases; no real watcher or appearance helper.
    let first = profile.claim().unwrap().unwrap();
    check(RuntimeState::Starting, true);
    first.ready().unwrap();
    check(RuntimeState::Ready, true);
    drop(first);
    check(RuntimeState::Stale, false);

    // The retained ready marker belongs to the old lease, not this instance.
    let second = profile.claim().unwrap().unwrap();
    check(RuntimeState::Starting, true);
    second.ready().unwrap();
    check(RuntimeState::Ready, true);
    second.finish(runtime::ExitKind::Failed).unwrap();
    check(RuntimeState::Stopping, true);
    drop(second);
    check(RuntimeState::Failed, false);

    // Neither the previous ready marker nor its failure applies to a new lease.
    let third = profile.claim().unwrap().unwrap();
    check(RuntimeState::Starting, true);
    drop(third);
    check(RuntimeState::Stale, false);
}

#[test]
fn watcher_installation_error_identifies_destination_and_acl_without_control_output() {
    let path = std::path::Path::new("/fixture/managed/bin/helper\n\x1b");
    let error = installation_error(
        path,
        "save appearance helper",
        std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
    )
    .to_string();
    assert!(error.contains("save appearance helper"));
    assert!(error.contains("/fixture/managed/bin/helper"));
    assert!(error.contains("ACL rules"));
    assert!(error.contains("did not change permissions"));
    assert!(!error.contains('\n'));
    assert!(!error.contains('\x1b'));
}

#[cfg(target_os = "macos")]
#[test]
fn watcher_installation_blocked_directory_reports_stage_without_changing_target() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::from_env_paths(&env);
    let directory = config.managed_dir("bin");
    fs::create_dir_all(directory.parent().unwrap()).unwrap();
    fs::write(&directory, b"private existing file").unwrap();
    let error = ensure_binary(&config).unwrap_err().to_string();
    assert!(error.contains("prepare watcher directory"), "{error}");
    assert!(error.contains(&directory.to_string_lossy().to_string()));
    assert_eq!(fs::read(&directory).unwrap(), b"private existing file");
    assert!(!env.managed_file("config.toml").exists());
}

fn fixture_env(root: &std::path::Path, profile: &str) -> SlateEnv {
    SlateEnv::from_vars(|key| match key {
        "HOME" => Some(root.as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(root.join(profile).into_os_string()),
        "XDG_CACHE_HOME" => Some(root.join("cache").into_os_string()),
        _ => None,
    })
    .unwrap()
}

struct OwnedFixture(Child);
impl Drop for OwnedFixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_fixture(root: &std::path::Path, name: &str) -> OwnedFixture {
    OwnedFixture(fixture_command(root, name).spawn().unwrap())
}

fn fixture_command(root: &std::path::Path, name: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "platform::dark_mode_notify::tests::watcher_fixture_process",
            "--nocapture",
        ])
        .env("SLATE_WATCHER_FIXTURE_ROOT", root)
        .env("SLATE_WATCHER_FIXTURE_NAME", name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

#[track_caller]
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !condition() {
        assert!(Instant::now() < deadline, "watcher fixture timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "private subprocess fixture; invoked only by watcher lifecycle tests"]
fn watcher_fixture_process() {
    let root = std::path::PathBuf::from(
        std::env::var_os("SLATE_WATCHER_FIXTURE_ROOT").expect("private root"),
    );
    let name = std::env::var("SLATE_WATCHER_FIXTURE_NAME").expect("private profile");
    assert!(root.is_dir() && ["profile-a", "profile-b"].contains(&name.as_str()));
    let env = fixture_env(&root, &name);
    if std::env::var_os("SLATE_WATCHER_CHECK_SESSION").is_some() {
        let ids = unsafe { [libc::getpid(), libc::getpgrp(), libc::getsid(0)] };
        fs::write(
            root.join("session-ids.json"),
            serde_json::to_vec(&ids).unwrap(),
        )
        .unwrap();
    }
    assert!(
        ConfigManager::from_env_paths(&env)
            .is_auto_theme_enabled()
            .unwrap(),
        "fixture preference missing"
    );
    let marker = env.config_dir().join("event-applied");
    let blocked = root.join("defer-events");
    let (source, sender) = events::Source::fixture();
    // Self-limiting even if a parent test aborts before its Child cleanup.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        let _ = sender.send(events::Event::Failed("fixture deadline".into()));
    });
    let result = run_with_events(
        &env,
        || Ok(source),
        || {
            if blocked.exists() {
                return Ok(false);
            }
            fs::write(&marker, b"applied")?;
            Ok(true)
        },
    );
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn watcher_profiles_stop_independently_and_crashed_records_do_not_own_processes() {
    use std::os::unix::process::CommandExt;
    let td = tempfile::tempdir().unwrap();
    let a = fixture_env(td.path(), "profile-a");
    let b = fixture_env(td.path(), "profile-b");
    let pa = runtime::Profile::new(&a).unwrap();
    let pb = runtime::Profile::new(&b).unwrap();
    assert_ne!(pa.directory, pb.directory);
    assert!(!pa.is_running().unwrap());
    pa.stop().unwrap();
    assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    for env in [&a, &b] {
        ConfigManager::with_env(env)
            .unwrap()
            .set_auto_theme_enabled(true)
            .unwrap();
    }
    assert_eq!(
        pa.directory,
        runtime::Profile::new(&a).unwrap().directory,
        "profile identity must be stable before/after config creation"
    );
    // A same-named but unowned process must be ignored completely.
    let mut unrelated = OwnedFixture(
        Command::new("/bin/sleep")
            .arg0(LAUNCHER)
            .arg("15")
            .spawn()
            .unwrap(),
    );
    fs::write(td.path().join("defer-events"), b"busy").unwrap();
    let mut first = spawn_fixture(td.path(), "profile-a");
    let mut second = spawn_fixture(td.path(), "profile-b");
    wait_until(|| {
        RuntimeInspection::inspect(&a).state == RuntimeState::Ready
            && RuntimeInspection::inspect(&b).state == RuntimeState::Ready
    });
    assert!(!a.config_dir().join("event-applied").exists());
    let original = fs::read(pa.directory.join("instance.json")).unwrap();
    let mut duplicate = spawn_fixture(td.path(), "profile-a");
    wait_until(|| duplicate.0.try_wait().unwrap().is_some());
    assert_eq!(
        fs::read(pa.directory.join("instance.json")).unwrap(),
        original
    );
    fs::remove_file(td.path().join("defer-events")).unwrap();
    wait_until(|| {
        a.config_dir().join("event-applied").exists()
            && b.config_dir().join("event-applied").exists()
    });
    pa.start(&mut Command::new("/fixture-must-not-spawn-another-process"))
        .unwrap();
    assert_eq!(RuntimeInspection::inspect(&a).state, RuntimeState::Ready);
    pa.stop().unwrap();
    wait_until(|| first.0.try_wait().unwrap().is_some());
    assert!(!pa.is_running().unwrap());
    assert_eq!(RuntimeInspection::inspect(&a).state, RuntimeState::Stopped);
    assert!(pb.is_running().unwrap());
    assert!(unrelated.0.try_wait().unwrap().is_none());
    // The previous stop token must not stop a freshly claimed generation.
    let mut restarted = spawn_fixture(td.path(), "profile-a");
    wait_until(|| pa.is_running().unwrap());
    wait_until(|| fs::read(pa.directory.join("instance.json")).unwrap() != original);
    std::thread::sleep(Duration::from_millis(250));
    assert!(restarted.0.try_wait().unwrap().is_none());
    restarted.0.kill().unwrap();
    restarted.0.wait().unwrap();
    assert!(!pa.is_running().unwrap());
    assert_eq!(RuntimeInspection::inspect(&a).state, RuntimeState::Stale);
    pa.stop().unwrap(); // Stale record, no live lease: no signalling.
    assert!(unrelated.0.try_wait().unwrap().is_none());
    pb.stop().unwrap();
    wait_until(|| second.0.try_wait().unwrap().is_some());
}

#[test]
fn watcher_start_owns_a_session_and_still_stops_through_profile_control() {
    let root = tempfile::tempdir().unwrap();
    let env = fixture_env(root.path(), "profile-a");
    ConfigManager::with_env(&env)
        .unwrap()
        .set_auto_theme_enabled(true)
        .unwrap();
    let profile = runtime::Profile::new(&env).unwrap();
    let mut command = fixture_command(root.path(), "profile-a");
    command.env("SLATE_WATCHER_CHECK_SESSION", "1");
    profile.start(&mut command).unwrap();
    let ids: [i32; 3] =
        serde_json::from_slice(&fs::read(root.path().join("session-ids.json")).unwrap()).unwrap();
    let independent = ids[0] == ids[1] && ids[0] == ids[2] && ids[2] != unsafe { libc::getsid(0) };
    // Always request cooperative stop before asserting session identity.
    profile.stop().unwrap();
    assert!(!profile.is_running().unwrap());
    assert!(independent, "child PID/group/session: {ids:?}");
}

#[test]
#[ignore = "private supervisor fixture; invoked only by watcher lifecycle tests"]
fn watcher_fixture_supervisor() {
    let root = std::path::PathBuf::from(
        std::env::var_os("SLATE_WATCHER_FIXTURE_ROOT").expect("private root"),
    );
    assert!(root.is_dir());
    let env = fixture_env(&root, "profile-a");
    let profile = runtime::Profile::new(&env).unwrap();
    profile
        .start(&mut fixture_command(&root, "profile-a"))
        .unwrap();
    fs::write(root.join("supervisor-ready"), b"ready").unwrap();
    std::thread::sleep(Duration::from_secs(10));
    profile.stop().unwrap();
}

#[test]
fn watcher_survives_termination_of_its_launching_process_group() {
    use std::os::unix::process::CommandExt;
    let root = tempfile::tempdir().unwrap();
    let env = fixture_env(root.path(), "profile-a");
    ConfigManager::with_env(&env)
        .unwrap()
        .set_auto_theme_enabled(true)
        .unwrap();
    // Keep event application pending until after the launching group exits.
    fs::write(root.path().join("defer-events"), b"busy").unwrap();
    let mut supervisor = OwnedFixture(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "platform::dark_mode_notify::tests::watcher_fixture_supervisor",
            ])
            .env("SLATE_WATCHER_FIXTURE_ROOT", root.path())
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    wait_until(|| root.path().join("supervisor-ready").exists());
    let pid = i32::try_from(supervisor.0.id()).unwrap();
    assert!(supervisor.0.try_wait().unwrap().is_none());
    assert!(pid > 1);
    assert_eq!(unsafe { libc::getpgid(pid) }, pid);
    // This is exclusively the group created for our still-owned child.
    assert_eq!(unsafe { libc::kill(-pid, libc::SIGTERM) }, 0);
    wait_until(|| supervisor.0.try_wait().unwrap().is_some());
    fs::remove_file(root.path().join("defer-events")).unwrap();
    wait_until(|| env.config_dir().join("event-applied").exists());
    let inspection = RuntimeInspection::inspect(&env);
    let profile = runtime::Profile::new(&env).unwrap();
    profile.stop().unwrap();
    assert_eq!(inspection.state, RuntimeState::Ready);
    assert_eq!(inspection.lock_held, Some(true));
    assert!(!profile.is_running().unwrap());
}

#[test]
fn watcher_private_control_paths_and_start_failures_are_reported() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let td = tempfile::tempdir().unwrap();
    let env = fixture_env(td.path(), "profile-a");
    let profile = runtime::Profile::new(&env).unwrap();
    assert!(profile.start(&mut Command::new("/usr/bin/false")).is_err());
    assert!(!profile.is_running().unwrap());
    ConfigManager::with_env(&env)
        .unwrap()
        .set_auto_theme_enabled(true)
        .unwrap();
    profile
        .start(&mut fixture_command(td.path(), "profile-a"))
        .unwrap();
    assert!(profile.is_running().unwrap());
    profile.stop().unwrap();
    assert!(!profile.is_running().unwrap());
    // Only this fixture's confirmed-unlocked inode, to inject a link failure.
    fs::remove_file(profile.directory.join("instance.lock")).unwrap();
    let outside = td.path().join("untouched");
    fs::write(&outside, b"do not change").unwrap();
    symlink(&outside, profile.directory.join("instance.lock")).unwrap();
    assert!(profile.is_running().is_err());
    assert!(profile.stop().is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"do not change");
    fs::remove_file(profile.directory.join("instance.lock")).unwrap();
    let lease = profile.claim().unwrap().unwrap();
    lease.ready().unwrap();
    let previous_stop = fs::read(profile.directory.join("stop")).unwrap();
    fs::set_permissions(
        profile.directory.join("instance.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(profile.stop().is_err());
    assert_eq!(
        fs::read(profile.directory.join("stop")).unwrap(),
        previous_stop
    );
    drop(lease);
}

#[test]
fn write_guard_busy_or_pending_auto_events_do_not_stop_the_watcher() {
    use crate::config::write_guard::{open_lock, record_path, try_lock};
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    let lock = open_lock(&env, true).unwrap().unwrap();
    assert!(try_lock(&lock).unwrap());
    assert!(!apply_auto_theme_quiet_with_env(&env).unwrap());
    assert!(!env.config_dir().exists());
    drop(lock);
    fs::write(record_path(&env), b"unfinished fixture").unwrap();
    assert!(!apply_auto_theme_quiet_with_env(&env).unwrap());
    assert!(!env.config_dir().exists());
    assert_eq!(fs::read(record_path(&env)).unwrap(), b"unfinished fixture");
}

#[test]
fn watcher_launcher_pins_injected_profile_and_isolated_start_is_inert() {
    let td = tempfile::tempdir().unwrap();
    let env = fixture_env(td.path(), "config root with 'quotes'");
    let script = launcher_contents(&env).unwrap();
    assert!(script.contains("unset SLATE_HOME"));
    assert!(script.contains("${SLATE_HOME:-}"));
    assert!(script.contains("export XDG_CACHE_HOME="));
    assert!(script.contains("__watch-auto-theme"));
    assert!(!script.contains("pgrep") && !script.contains("pkill"));
    let env = SlateEnv::with_home(td.path().join("isolated"));
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    start(&config).unwrap();
    stop_with_env(&env).unwrap();
    assert!(!env.slate_cache_dir().join("watchers").exists());
}

#[test]
fn watcher_exit_receipts_distinguish_source_failure_from_disabled_start() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    let failure = run_with_events(
        &env,
        || Err(error("private fixture backend failure")),
        || panic!("must not apply"),
    );
    assert!(failure.is_err());
    let report = RuntimeInspection::inspect(&env);
    assert_eq!(report.state, RuntimeState::Failed);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("private fixture"));
    config.set_auto_theme_enabled(false).unwrap();
    run_with_events(
        &env,
        || panic!("disabled start must not open a backend"),
        || panic!("must not apply"),
    )
    .unwrap();
    assert_eq!(
        RuntimeInspection::inspect(&env).state,
        RuntimeState::Stopped
    );
}
