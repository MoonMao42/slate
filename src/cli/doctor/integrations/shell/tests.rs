use super::*;
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt};
use tempfile::TempDir;

#[test]
fn doctor_shell_literal_source_shapes_do_not_confuse_printing_with_loading() {
    for expected in ["/tmp/slate/env.zsh", "/tmp/space 中文/a'b\\\\$()/env.fish"] {
        for fish in [false, true] {
            let quoted = if fish {
                fish_quote(expected)
            } else {
                shell_quote(expected)
            };
            for line in [
                format!("source {quoted}"),
                format!("\t source\t{quoted} # comment\r\n"),
            ] {
                assert!(
                    literal_reference(line.as_bytes(), expected, fish),
                    "{line:?}"
                );
            }
            for line in [
                format!("# source {quoted}"),
                format!("echo source {quoted}"),
                format!("printf {quoted}"),
                format!("LOADER={quoted}"),
                format!("source_extra {quoted}"),
                format!("source {quoted}.old"),
                format!("source {quoted} extra"),
                format!("source {quoted}#not-a-comment"),
                format!("source {quoted}; echo ignored"),
            ] {
                assert!(
                    !literal_reference(line.as_bytes(), expected, fish),
                    "{line:?}"
                );
            }
            assert_eq!(
                literal_reference(format!(". {quoted}").as_bytes(), expected, fish),
                !fish
            );
        }
    }
    for line in [
        "source /tmp/slate/env.bash",
        "source \"/tmp/slate/env.bash\"",
    ] {
        assert!(literal_reference(
            line.as_bytes(),
            "/tmp/slate/env.bash",
            false
        ));
    }
    assert!(!literal_reference(
        b"source \"/tmp/$HOME/env.bash\"",
        "/tmp/$HOME/env.bash",
        false
    ));
}

fn status<'a>(report: &'a Report, code: &str) -> Option<&'a str> {
    report
        .checks
        .iter()
        .find(|check| check.code == Some(code))
        .map(|check| check.status)
}

#[test]
// SWATCH-RENDERER: hostile styling bytes are test input for source-redaction checks.
fn doctor_shell_marker_diagnostics_preserve_opaque_bytes_and_omit_source() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    for target in ["bash", "zsh"] {
        let startup = if target == "bash" {
            env.bash_integration_path()
        } else {
            env.zshrc_path()
        };
        let managed = env.config_dir().join(format!("managed/shell/env.{target}"));
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        fs::write(&managed, b"PRIVATE_SOURCE\x1b[31m\xff").unwrap();
        let reference = format!("source {}\n", shell_quote(managed.to_str().unwrap()));
        for (markers, expected) in [
            (
                format!(
                    "{}\n{reference}{}\n",
                    marker_block::START,
                    marker_block::END
                ),
                "ok",
            ),
            (reference.clone(), "info"),
            (format!("{}\n{reference}", marker_block::START), "error"),
            (
                format!("{}\n{}\n", marker_block::END, marker_block::START),
                "error",
            ),
            (
                format!(
                    "{}\n{reference}{}\n{}\n",
                    marker_block::START,
                    marker_block::END,
                    marker_block::END
                ),
                "error",
            ),
        ] {
            let content = [
                b"# PRIVATE_SOURCE\x1b[31m\xff\n".as_slice(),
                markers.as_bytes(),
            ]
            .concat();
            fs::write(&startup, &content).unwrap();
            let report = super::super::inspect(target, &env);
            assert_eq!(status(&report, "startup_file"), Some("ok"));
            assert_eq!(status(&report, "managed_environment"), Some("ok"));
            assert_eq!(status(&report, "loader_markers"), Some(expected));
            assert_eq!(
                status(&report, "loader_reference"),
                if expected == "error" {
                    None
                } else {
                    Some("ok")
                }
            );
            for output in [
                serde_json::to_string(&report).unwrap(),
                super::super::text_report(&report),
            ] {
                assert!(!output.contains("PRIVATE_SOURCE"));
                assert!(output.contains("not proof of shell execution"));
            }
            assert_eq!(fs::read(&startup).unwrap(), content);
        }
        fs::write(
            &startup,
            format!("echo {}\n", shell_quote(managed.to_str().unwrap())),
        )
        .unwrap();
        let report = super::super::inspect(target, &env);
        assert_eq!(status(&report, "loader_reference"), Some("warning"));
        assert!(report
            .checks
            .iter()
            .any(|check| check.code == Some("loader_reference") && check.suggestion.is_some()));
    }
}

#[test]
// SWATCH-RENDERER: test-only paths contain hostile styling bytes for output escaping.
fn doctor_shell_non_utf8_and_multiline_paths_are_unknown_not_missing_references() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    for component in [
        OsString::from_vec(b"profile-\xff".to_vec()),
        OsString::from("profile-\n\x1b[31m"),
    ] {
        // APFS cannot create every Unix path byte sequence. Exercise the same
        // reporting path in memory instead of weakening this check on macOS.
        let startup = td.path().join(component).join(".zshrc");
        let managed = startup.parent().unwrap().join("managed/env.zsh");
        let mut report = super::super::inspect("zsh", &env);
        report.checks.clear();
        inspect_reference(
            &mut report,
            b"# PRIVATE_SOURCE\n",
            &startup,
            &managed,
            false,
        );
        assert_eq!(status(&report, "loader_reference"), Some("info"));
        let text = super::super::text_report(&report);
        assert!(text.contains("comparison is unavailable"));
        assert!(!text.contains('\x1b'));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(
            json["checks"][0]["path_is_lossy"],
            startup.to_str().is_none()
        );
    }
}
