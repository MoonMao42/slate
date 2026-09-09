//! Real CLI safety checks; isolated profiles never launch the host Ghostty.
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod tree_snapshot;

#[path = "ghostty_doctor/window_style.rs"]
mod window_style;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(5));
    command
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn entry(home: &Path) -> std::path::PathBuf {
    home.join(".config/ghostty/config")
}

fn report(home: &Path) -> serde_json::Value {
    let output = command(home)
        .args(["doctor", "ghostty", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["target"], "ghostty");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["reference_syntax"], "ghostty-1.3.1");
    assert_eq!(json["entry_order"], "ghostty-1.3.1-defaults");
    assert_eq!(json["validation"]["status"], "skipped");
    json
}

#[test]
fn ghostty_files_only_skips_native_validation_without_isolated_mode() {
    let home = tempfile::tempdir().unwrap();
    write(&entry(home.path()), "background = #123456\n");
    let binary = home.path().join("bin/ghostty");
    write(
        &binary,
        "#!/bin/sh\nprintf invoked > \"$HOME/ghostty-invoked\"\nexit 91\n",
    );
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let before = tree_snapshot::tree(home.path());
    for args in [
        vec!["doctor", "ghostty", "--files-only", "--json"],
        vec!["doctor", "--files-only", "--json"],
    ] {
        let output = command(home.path())
            .env_remove("SLATE_HOME")
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["validation"]["status"], "skipped");
        assert!(String::from_utf8_lossy(&output.stdout).contains("file-only check"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    for args in [
        vec!["doctor", "nvim", "--files-only"],
        vec!["doctor", "ghostty", "--files-only", "--check-version"],
    ] {
        command(home.path())
            .env_remove("SLATE_HOME")
            .args(args)
            .assert()
            .failure();
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn ghostty_doctor_entry_order_agrees_with_adapter_for_every_presence_combination() {
    use slate_cli::adapter::GhosttyAdapter;

    let count = if cfg!(target_os = "macos") { 4 } else { 2 };
    for mask in 0..(1 << count) {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let expected = [
            (env.xdg_config_home().join("ghostty/config"), "XDG config"),
            (
                env.xdg_config_home().join("ghostty/config.ghostty"),
                "XDG config.ghostty",
            ),
            (
                td.path()
                    .join("Library/Application Support/com.mitchellh.ghostty/config"),
                "macOS App Support config",
            ),
            (
                td.path()
                    .join("Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
                "macOS App Support config.ghostty",
            ),
        ];
        for (index, (path, _)) in expected.iter().take(count).enumerate() {
            if mask & (1 << index) != 0 {
                write(path, "# private fixture\n");
            }
        }
        let selected = expected
            .iter()
            .take(count)
            .enumerate()
            .rev()
            .find(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, (path, _))| path)
            .unwrap_or(&expected[1].0);
        assert_eq!(
            &GhosttyAdapter
                .integration_config_path_with_env(&env)
                .unwrap(),
            selected
        );
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(
            json["selected_entry"],
            selected.to_str().unwrap(),
            "mask {mask}: {json}"
        );
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), count);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["selected"] == true)
                .count(),
            1
        );
        for (index, (entry, (path, label))) in entries.iter().zip(expected).enumerate() {
            assert_eq!(entry["path"], path.to_str().unwrap());
            assert_eq!(entry["label"], label);
            assert_eq!(entry["load_order_index"], index);
            assert_eq!(entry["exists"], mask & (1 << index) != 0);
        }
        assert_eq!(json["scan_complete"], true);
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}

#[test]
fn ghostty_doctor_follows_complete_literal_paths_and_bom_prefixed_references() {
    for (value, filename, home_relative) in [
        ("file with spaces.conf", "file with spaces.conf", false),
        (
            "file # not a comment.conf",
            "file # not a comment.conf",
            false,
        ),
        ("?\"optional file.conf\"", "optional file.conf", false),
        ("'single quoted.conf'", "'single quoted.conf'", false),
        ("\"a\\b.conf\"", "a\\b.conf", false),
        ("\"\"?literal.conf\"\"", "?literal.conf", false),
        ("~/home file.conf", "home file.conf", true),
        ("~", "~", false),
    ] {
        let td = tempfile::tempdir().unwrap();
        let root = entry(td.path());
        let nested = if home_relative {
            td.path().join(filename)
        } else {
            root.parent().unwrap().join(filename)
        };
        write(&root, format!("\u{feff}config-file = {value}\r\n"));
        write(&nested, format!("config-file = {}\n", root.display()));
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(json["scan_complete"], true, "{value}: {json}");
        assert_eq!(
            json["config_file_cycles"].as_array().unwrap().len(),
            1,
            "{value}: {json}"
        );
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("space profile");
    let env = SlateEnv::with_home(home.clone());
    let managed = env.managed_file("managed/ghostty/theme #= file.conf");
    write(&managed, "# PRIVATE_CONTENT\n");
    for name in ["config", "config.ghostty"] {
        write(
            &home.join(".config/ghostty").join(name),
            format!("config-file = {}\n", managed.display()),
        );
    }
    let before = tree_snapshot::tree(td.path());
    let json = report(&home);
    assert_eq!(
        json["duplicate_refs"][0]["slate_ref"],
        managed.to_str().unwrap()
    );
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_distinguishes_optional_absence_from_required_or_unsafe_references() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    for (content, missing) in [
        ("config-file = ?absent\n", false),
        ("config-file = absent\n", true),
        ("config-file = ?absent\nconfig-file = absent\n", true),
        ("config-file = absent\nconfig-file = ?absent\n", true),
        ("config-file = ?\nconfig-file = ?\"\"\n", false),
    ] {
        write(&root, content);
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(issue(&json, "required_file_missing"), missing, "{json}");
        assert_eq!(json["scan_complete"], !missing);
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    let unsafe_path = root.parent().unwrap().join("pipe");
    let c = std::ffi::CString::new(unsafe_path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    write(&root, "config-file = ?pipe\n");
    let before = tree_snapshot::tree(td.path());
    let json = report(td.path());
    assert!(issue(&json, "read_error"));
    assert_eq!(json["scan_complete"], false);
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_preserves_absolute_alias_context_after_scanning_relative_alias() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let parent = root.parent().unwrap();
    let actual = td.path().join("actual");
    let alias = parent.join("alias.conf");
    write(&actual.join("source.conf"), "config-file = child.conf\n");
    write(&actual.join("child.conf"), "# no cycle in real directory\n");
    write(
        &parent.join("child.conf"),
        format!("config-file = {}\n", root.display()),
    );
    symlink(actual.join("source.conf"), &alias).unwrap();
    write(&root, "config-file = alias.conf\n");
    assert_eq!(report(td.path())["cycle_risk"], false);
    // Same bytes, two different load spellings. A physical-file cache must not
    // hide the absolute alias's different nested-relative directory.
    write(
        &root,
        format!(
            "config-file = alias.conf\nconfig-file = {}\n",
            alias.display()
        ),
    );
    let before = tree_snapshot::tree(td.path());
    let json = report(td.path());
    assert_eq!(json["scan_complete"], true);
    assert_eq!(json["cycle_risk"], true);
    assert_eq!(json["config_file_cycles"].as_array().unwrap().len(), 1);
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_does_not_lexically_bypass_invalid_absolute_prefixes() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let parent = root.parent().unwrap();
    write(&parent.join("not-directory"), "PRIVATE_CONTENT");
    write(
        &parent.join("other.conf"),
        format!("config-file = {}\n", root.display()),
    );
    for absolute in [false, true] {
        let value = if absolute {
            parent.join("not-directory/../other.conf")
        } else {
            // macOS realpath can resolve file/../sibling; use a genuinely
            // non-directory traversal here, without assuming Linux behavior.
            "not-directory/child.conf".into()
        };
        write(&root, format!("config-file = {}\n", value.display()));
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(json["scan_complete"], false, "absolute={absolute}: {json}");
        assert_eq!(json["cycle_risk"], false);
        assert!(issue(
            &json,
            if absolute {
                "read_error"
            } else {
                "unresolved_reference"
            }
        ));
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}

#[test]
fn ghostty_doctor_exposes_unmodeled_resets_and_native_line_boundaries() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let cycle = format!("config-file = {}\n", root.display());
    for (suffix, reset) in [
        ("config-file = \n", true),
        ("config-file = \"\"\n", true),
        ("config-file = ?\"\"\n", false),
    ] {
        write(&root, format!("{cycle}{suffix}"));
        let json = report(td.path());
        assert_eq!(issue(&json, "reference_reset"), reset);
        assert_eq!(json["cycle_risk"], !reset);
        assert_eq!(json["scan_complete"], !reset);
    }
    for size in [4094, 4095] {
        write(&root, format!("{}\n{cycle}", "#".repeat(size)));
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(issue(&json, "line_limit"), size == 4095);
        assert_eq!(json["cycle_risk"], size == 4094);
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    write(&root, "config-file = PRIVATE_CONTENT\0value\n");
    assert!(issue(&report(td.path()), "invalid_reference"));
}

fn issue(report: &serde_json::Value, code: &str) -> bool {
    report["scan_issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == code)
}

#[test]
fn ghostty_doctor_rejects_bad_entries_and_nested_sources_without_writes_or_hangs() {
    for nested in [false, true] {
        for kind in ["fifo", "directory", "dangling", "large", "binary"] {
            let td = tempfile::tempdir().unwrap();
            let path = if nested {
                td.path().join("nested.conf")
            } else {
                entry(td.path())
            };
            if nested {
                write(
                    &entry(td.path()),
                    format!("config-file = {}\n", path.display()),
                );
            }
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match kind {
                "fifo" => {
                    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
                }
                "directory" => fs::create_dir(&path).unwrap(),
                "dangling" => symlink(td.path().join("absent"), &path).unwrap(),
                "large" => fs::File::create(&path)
                    .unwrap()
                    .set_len(8 * 1024 * 1024 + 1)
                    .unwrap(),
                "binary" => write(&path, b"PRIVATE_CONTENT\xff"),
                _ => unreachable!(),
            }
            let before = tree_snapshot::tree(td.path());
            let json = report(td.path());
            assert_eq!(json["scan_complete"], false, "{kind}: {json}");
            assert!(issue(&json, "read_error"));
            let text = command(td.path())
                .args(["doctor", "ghostty"])
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            assert!(String::from_utf8_lossy(&text).contains("cycle risk: unknown"));
            assert_eq!(tree_snapshot::tree(td.path()), before);
        }
    }
}

#[test]
fn ghostty_doctor_limits_include_breadth_depth_edges_and_total_bytes() {
    for case in ["file_limit", "depth_limit", "edge_limit", "byte_limit"] {
        let td = tempfile::tempdir().unwrap();
        let root = entry(td.path());
        match case {
            "file_limit" => write(
                &root,
                (0..260)
                    .map(|i| format!("config-file = absent-{i}\n"))
                    .collect::<String>(),
            ),
            "depth_limit" => {
                write(&root, "config-file = 0\n");
                for i in 0..66 {
                    write(
                        &root.parent().unwrap().join(i.to_string()),
                        format!("config-file = {}\n", i + 1),
                    );
                }
            }
            "edge_limit" => write(&root, "config-file = absent\n".repeat(4097)),
            "byte_limit" => {
                write(
                    &root,
                    (0..6)
                        .map(|i| format!("config-file = large-{i}\n"))
                        .collect::<String>(),
                );
                for i in 0..6 {
                    write(
                        &root.parent().unwrap().join(format!("large-{i}")),
                        vec![b'#'; 6 * 1024 * 1024],
                    );
                }
            }
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(json["scan_complete"], false);
        assert!(issue(&json, case), "{json}");
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}

#[test]
fn ghostty_doctor_preserves_cycles_aliases_and_isolated_boundaries() {
    let td = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let nested = td.path().join("nested.conf");
    let alias = td.path().join("alias.conf");
    write(&root, format!("config-file = {}\n", alias.display()));
    write(&nested, format!("config-file = {}\n", root.display()));
    symlink(&nested, &alias).unwrap();
    let json = report(td.path());
    assert_eq!(json["scan_complete"], true);
    assert_eq!(json["cycle_risk"], true);
    assert_eq!(json["config_file_cycles"].as_array().unwrap().len(), 1);
    write(&outside.path().join("private.conf"), "PRIVATE_CONTENT\n");
    fs::remove_file(&alias).unwrap();
    symlink(outside.path().join("private.conf"), &alias).unwrap();
    let before = tree_snapshot::tree(td.path());
    let external_before = tree_snapshot::tree(outside.path());
    let json = report(td.path());
    assert_eq!(json["scan_complete"], false);
    assert!(issue(&json, "outside_profile"));
    assert_eq!(tree_snapshot::tree(td.path()), before);
    assert_eq!(tree_snapshot::tree(outside.path()), external_before);
}

#[test]
fn ghostty_doctor_empty_and_linked_profiles_are_read_only_and_do_not_launch_tools() {
    let td = tempfile::tempdir().unwrap();
    assert_eq!(report(td.path())["scan_complete"], true);
    assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    let tool = td.path().join("bin/ghostty");
    write(
        &tool,
        "#!/bin/sh\n: > \"$HOME/unexpected-launch\"\nexit 91\n",
    );
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    let source = td.path().join("user.conf");
    write(&source, "# PRIVATE_CONTENT\n");
    fs::create_dir_all(entry(td.path()).parent().unwrap()).unwrap();
    symlink(&source, entry(td.path())).unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let before = tree_snapshot::tree(td.path());
    assert_eq!(report(env.home())["scan_complete"], true);
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_paths_and_closed_stdout_remain_safe_during_pending_recovery() {
    use assert_cmd::assert::OutputAssertExt;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::{Command, Stdio};
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile-\u{1b}[31m\n\u{202e}");
    fs::create_dir(&home).unwrap();
    write(&entry(&home), "# PRIVATE_CONTENT\n");
    write(
        &home.join(".cache/slate/preview-session.json"),
        "PRIVATE_CONTENT",
    );
    let before = tree_snapshot::tree(td.path());
    let json = report(&home);
    assert_eq!(json["paths_are_lossy"], false);
    let output = command(&home)
        .args(["doctor", "ghostty"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(!text.contains('\u{1b}') && !text.contains('\u{202e}'));
    assert!(text.contains("\\u{1b}[31m\\n"));
    for json in [false, true] {
        let (closed, output) = UnixStream::pair().unwrap();
        drop(closed);
        let fd: OwnedFd = output.into();
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
        cmd.env_clear()
            .env("HOME", &home)
            .env("SLATE_HOME", &home)
            .args(["doctor", "ghostty"])
            .stdout(Stdio::from(fd))
            .stderr(Stdio::piped());
        if json {
            cmd.arg("--json");
        }
        let result = redirected_output::run(&mut cmd)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(result.stderr.is_empty());
    }
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_accepts_non_unicode_home_without_creating_it() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let td = tempfile::tempdir().unwrap();
    // No filesystem support for creating a non-Unicode name is required.
    let home = td.path().join(OsString::from_vec(b"profile-\xff".to_vec()));
    let before = tree_snapshot::tree(td.path());
    let json = report(&home);
    assert_eq!(json["paths_are_lossy"], true);
    for entry in json["entries"].as_array().unwrap() {
        assert_eq!(entry["path_is_lossy"], true);
        assert_eq!(entry["exists"], false);
    }
    let output = command(&home)
        .args(["doctor", "ghostty"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("lossy display; not an exact path"));
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_native_validation_uses_private_binary_and_skips_incomplete_scan() {
    for status in ["passed", "failed", "skipped"] {
        let td = tempfile::tempdir().unwrap();
        let marker = td.path().join("validator-launched");
        let script = format!("#!/bin/sh\n[ \"$1\" = +validate-config ] || exit 91\nif read -r unexpected; then exit 92; fi\nprintf 'selected' > \"$HOME/validator-launched\"\n{}\n",
            if status == "failed" { "printf 'native error\\n' >&2; exit 2" } else { "exit 0" });
        let app_binary = td
            .path()
            .join("Applications/Ghostty.app/Contents/MacOS/ghostty");
        let path_binary = td.path().join("bin/ghostty");
        for binary in
            std::iter::once(&path_binary).chain(cfg!(target_os = "macos").then_some(&app_binary))
        {
            write(binary, &script);
            fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let theme = td.path().join(".config/slate/managed/ghostty/theme.conf");
        write(&theme, "macos-titlebar-style = transparent\n");
        write(
            &entry(td.path()),
            format!("# PRIVATE_CONTENT\nconfig-file = {}\n", theme.display()),
        );
        if status == "skipped" {
            write(&entry(td.path()), b"PRIVATE_CONTENT\xff");
        }
        let output = command(td.path())
            .env_remove("SLATE_HOME")
            .args(["doctor", "ghostty", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(output.stderr.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["validation"]["status"], status, "{json}");
        assert_eq!(json["scan_complete"], status != "skipped");
        assert_eq!(
            json["window_style"]["status"],
            if status == "skipped" {
                "unknown"
            } else {
                "managed_override"
            }
        );
        assert_eq!(json["validation"]["timeout_ms"], 5000);
        assert_eq!(json["validation"]["output_limit_bytes"], 65536);
        assert_eq!(json["validation"]["output_truncated"], false);
        assert_eq!(marker.exists(), status != "skipped");
        if status != "skipped" {
            let expected = if cfg!(target_os = "macos") {
                &app_binary
            } else {
                &path_binary
            };
            assert_eq!(json["validation"]["binary"], expected.to_str().unwrap());
        }
    }
}
