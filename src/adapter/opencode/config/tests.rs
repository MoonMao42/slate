use super::*;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

fn apply(input: &str) -> String {
    let path = Path::new("tui.jsonc");
    let doc = Document::parse(input, path).unwrap();
    String::from_utf8(
        doc.system(path)
            .unwrap()
            .unwrap_or_else(|| input.as_bytes().to_vec()),
    )
    .unwrap()
}

fn clean(input: &str) -> Option<String> {
    let path = Path::new("tui.jsonc");
    Document::parse(input, path)
        .unwrap()
        .without_system_theme(path)
        .unwrap()
        .map(|bytes| String::from_utf8(bytes).unwrap())
}

#[test]
fn opencode_edits_only_theme_and_missing_schema_bytes() {
    let input = "// 个人配置\r\n{\r\n\t\"$schema\": \"custom-schema\",\r\n\t\"th\\u0065me\" /* key */ : /* value */ \"custom\", // tail\r\n\t\"url\": \"https://example.test/a/*b*/\",\r\n\t\"data\": {\"theme\": \"nested\", \"n\": 1.2300e+02,},\r\n}\r\n";
    let after = apply(input);
    assert_eq!(after, input.replacen("\"custom\"", "\"system\"", 1));
    assert_eq!(apply(&after), after);
    let cleaned = clean(&after).unwrap();
    assert_eq!(
        cleaned,
        after
            .replace("\"th\\u0065me\"", "")
            .replace("\"system\",", "")
            .replace(" : ", "  ")
    );
    assert_eq!(clean(&cleaned).unwrap(), cleaned);
    for schema in ["42", "null", "{\"custom\":true}"] {
        let source = format!("{{\"$schema\":{schema},\"theme\":\"custom\"}}");
        assert_eq!(apply(&source), source.replace("\"custom\"}", "\"system\"}"));
    }
}

#[test]
fn opencode_appends_before_trailing_comments() {
    for input in [
        "{}",
        "{ }",
        "{/* notes */}",
        "{\n// notes\n}",
        "// header\n{\"mouse\":false // trailing\n}",
        "{\"mouse\":false, /* trailing */}",
        "{\r\n\t\"mouse\":false, // trailing\r\n}\r\n",
        "{\"theme\":\"custom\"/* trailing */}",
        "{\"$schema\":\"user\"}",
    ] {
        let after = apply(input);
        let doc = Document::parse(&after, Path::new("tui.jsonc")).unwrap();
        assert_eq!(doc.string("theme"), Some("system"));
        assert!(doc.property("$schema").is_some());
        for note in [
            "// header",
            "// notes",
            "/* notes */",
            "// trailing",
            "/* trailing */",
            "\"mouse\":false",
        ] {
            if input.contains(note) {
                assert!(after.contains(note), "{after}");
            }
        }
        if input.contains("\r\n") {
            assert!(!after.replace("\r\n", "").contains('\n'));
        }
        assert_eq!(apply(&after), after);
    }
}

#[test]
fn opencode_cleanup_preserves_notes_at_every_root_comma_position() {
    assert_eq!(clean("{\"theme\":\"system\"}"), None);
    assert_eq!(
        clean(&format!(
            "{{\"theme\":\"system\",\"$schema\":\"{}\"}}",
            OpencodeAdapter::TUI_SCHEMA
        )),
        None
    );
    for newline in ["\n", "\r\n"] {
        for position in 0..3 {
            for trailing in [false, true] {
                for inside_comments in [false, true] {
                    let theme = if inside_comments {
                        "\"theme\"/*key*/:/*value*/\"system\""
                    } else {
                        "\"theme\":\"system\""
                    };
                    let mut props = vec!["\"user\":{\"theme\":\"system\"}", "\"number\":1.00e2"];
                    props.insert(position, theme);
                    let separator = format!(", /* separator, */{newline}\t");
                    let input = format!(
                        "// notes{newline}{{{newline}\t{}{} // last{newline}}}{newline}",
                        props.join(&separator),
                        if trailing { "," } else { "" }
                    );
                    let after = clean(&input).unwrap();
                    let parsed = Document::parse(&after, Path::new("tui.jsonc")).unwrap();
                    assert!(parsed.property("theme").is_none());
                    assert_eq!(parsed.root.properties.len(), 2);
                    let original = Document::parse(&input, Path::new("tui.jsonc")).unwrap();
                    let comments = |d: &Document<'_>| {
                        d.tokens
                            .iter()
                            .filter(|t| comment(&t.token))
                            .map(|t| t.text(d.text).to_owned())
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(comments(&original), comments(&parsed));
                    assert!(after.contains("\"number\":1.00e2"));
                    assert!(after.contains("\"user\":{\"theme\":\"system\"}"));
                    assert_eq!(clean(&after).unwrap(), after);
                }
            }
        }
    }
    assert_eq!(
        clean("// notes\n{\"theme\":\"system\"/* end */}").unwrap(),
        "// notes\n{/* end */}"
    );
    assert_eq!(
        clean("{\"theme\":\"custom\",\"n\":1.00}"),
        Some("{\"theme\":\"custom\",\"n\":1.00}".into())
    );
}

#[test]
fn opencode_rejects_ambiguous_or_malformed_jsonc_without_content_in_errors() {
    for input in [
        "{} /* PRIVATE_CONTENT",
        "{\"n\":1/* PRIVATE_CONTENT */2}",
        "{,}",
        "{\"n\":[,]}",
        "{\"n\":1,,}",
        "{\"theme\":true}",
        "[]",
        "// PRIVATE_CONTENT",
        "{\"theme\":\"system\",\"th\\u0065me\":\"PRIVATE_CONTENT\"}",
        "{\"theme\":\"system\",\"x\":1,\"x\":2}",
        "{theme:\"system\"}",
        "{'theme':'system'}",
        "{\"x\":+1}",
        "{\"x\":0xff}",
        "{\"x\":1 \"theme\":\"system\"}",
        "{\"x\":\"PRIVATE_CONTENT\n\"}",
    ] {
        let Err(error) = Document::parse(input, Path::new("tui.jsonc")) else {
            panic!("accepted {input}");
        };
        assert!(!error.to_string().contains("PRIVATE_CONTENT"));
    }
}

#[test]
fn prepared_opencode_rejects_changed_sources_and_oversize_output() {
    for mutation in ["bytes", "identity", "mode", "missing", "link", "created"] {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("tui.jsonc");
        let input = "{\"theme\":\"custom\"}";
        if mutation != "created" {
            fs::write(&path, input).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let prepared = Prepared::read(&path).unwrap();
        prepared.verify().unwrap();
        match mutation {
            "bytes" | "created" => fs::write(&path, "// PRIVATE_CONTENT\n{}").unwrap(),
            "identity" => {
                let other = td.path().join("other");
                fs::write(&other, input).unwrap();
                fs::set_permissions(&other, fs::Permissions::from_mode(0o640)).unwrap();
                fs::rename(other, &path).unwrap();
            }
            "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap(),
            "missing" => fs::remove_file(&path).unwrap(),
            "link" => {
                let other = td.path().join("other");
                fs::rename(&path, &other).unwrap();
                symlink(other, &path).unwrap();
            }
            _ => unreachable!(),
        }
        let before = fs::read(&path).ok();
        assert!(!prepared
            .publish()
            .unwrap_err()
            .to_string()
            .contains("PRIVATE_CONTENT"));
        assert_eq!(fs::read(&path).ok(), before);
    }
    let td = tempfile::tempdir().unwrap();
    let path = td.path().join("tui.jsonc");
    let input = format!("{}{{}}", " ".repeat(MAX_TOOL_CONFIG_BYTES as usize - 2));
    fs::write(&path, &input).unwrap();
    assert!(Prepared::read(&path).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), input);
}

#[test]
fn prepared_opencode_backup_keeps_captured_bytes_and_refuses_later_edit() {
    let td = tempfile::tempdir().unwrap();
    let env = crate::env::SlateEnv::with_home(td.path().to_owned());
    let path = td.path().join("tui.jsonc");
    let initial = b"// personal comment\n{\"theme\":\"custom\"}";
    fs::write(&path, initial).unwrap();
    let prepared = Prepared::read(&path).unwrap();
    let external = b"// later edit\n{\"theme\":\"later\"}";
    fs::write(&path, external).unwrap();
    let backup = crate::config::ConfigManager::from_env_paths(&env)
        .backup_captured_file(&path, prepared.original_bytes().unwrap())
        .unwrap();
    assert_eq!(fs::read(&backup).unwrap(), initial);
    assert_eq!(
        fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(prepared.publish().is_err());
    assert_eq!(fs::read(&path).unwrap(), external);
    assert_eq!(fs::read(&backup).unwrap(), initial);
}

#[test]
fn prepared_opencode_pins_directory_destination_even_when_source_identity_matches() {
    for existing in [false, true] {
        for retarget in [false, true] {
            let td = tempfile::tempdir().unwrap();
            let first = td.path().join("first");
            let second = td.path().join("second");
            fs::create_dir(&first).unwrap();
            fs::create_dir(&second).unwrap();
            let original = first.join("tui.jsonc");
            let other = second.join("tui.jsonc");
            let input = "{\"theme\":\"custom\"}";
            if existing {
                fs::write(&original, input).unwrap();
                fs::set_permissions(&original, fs::Permissions::from_mode(0o640)).unwrap();
                fs::hard_link(&original, &other).unwrap();
            }
            let alias = td.path().join("config");
            symlink(&first, &alias).unwrap();
            let prepared = Prepared::read(&alias.join("tui.jsonc")).unwrap();
            if retarget {
                fs::remove_file(&alias).unwrap();
                symlink(&second, &alias).unwrap();
                let error = prepared.publish().unwrap_err().to_string();
                assert!(error.contains("destination changed"));
                for path in [&original, &other] {
                    assert_eq!(
                        fs::read(path).ok(),
                        existing.then(|| input.as_bytes().to_vec())
                    );
                }
            } else {
                prepared.publish().unwrap();
                let text = fs::read_to_string(&original).unwrap();
                assert_eq!(
                    Document::parse(&text, &original)
                        .unwrap()
                        .has_system_theme(),
                    Some(true)
                );
                if existing {
                    assert_eq!(
                        fs::metadata(&original).unwrap().permissions().mode() & 0o777,
                        0o640
                    );
                    assert_eq!(fs::read_to_string(&other).unwrap(), input);
                } else {
                    assert!(!other.exists());
                }
            }
        }
    }
}
