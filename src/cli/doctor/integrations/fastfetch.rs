//! Compare Slate's own generated preset without launching system probes.
use super::{tool_files, Report};
use crate::{adapter::FastfetchAdapter, config::file_read::MAX_TOOL_CONFIG_BYTES, env::SlateEnv};

/// Compare literal tokens, not parsed values: comments/JSON whitespace may vary,
/// but strings, numbers, property order and punctuation must remain identical.
/// Streaming avoids recursive parsing of an untrusted deeply nested document.
fn same_preset_tokens(actual: &str, expected: &str) -> bool {
    use jsonc_parser::{tokens::Token, Scanner, ScannerOptions};
    fn next(scanner: &mut Scanner<'_>) -> Result<Option<(usize, usize)>, ()> {
        loop {
            let previous_end = scanner.token_end();
            let token = scanner.scan().map_err(|_| ())?;
            if !scanner.file_text()[previous_end..scanner.token_start()]
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                return Err(());
            }
            match token {
                Some(Token::CommentLine(_) | Token::CommentBlock(_)) => continue,
                Some(_) => return Ok(Some((scanner.token_start(), scanner.token_end()))),
                None => return Ok(None),
            }
        }
    }
    let options = ScannerOptions {
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let mut left = Scanner::new(actual, &options);
    let mut right = Scanner::new(expected, &options);
    loop {
        match (next(&mut left), next(&mut right)) {
            (Ok(None), Ok(None)) => return true,
            (Ok(Some((a, b))), Ok(Some((c, d)))) if actual[a..b] == expected[c..d] => {}
            _ => return false,
        }
    }
}

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only Fastfetch file and availability and generated-preset comparison; no tool is launched or configuration changed. Only the saved theme (4 KiB) and managed preset (8 MiB) are read, separately rather than atomically. No shell startup, system-information probe, installer or reload. JSONC syntax, personal layouts, wrapper activation, explicit --config selection and live colors are not evaluated.";
    tool_files::availability(report, env, "fastfetch");
    let theme = tool_files::saved_theme(report, env);
    let path = FastfetchAdapter::theme_path(env);
    let preset = tool_files::read(
        report,
        env,
        &path,
        MAX_TOOL_CONFIG_BYTES,
        "preset_file",
        "Slate Fastfetch preset",
    );
    if let (Ok(Some(content)), Some(theme)) = (preset, theme) {
        match FastfetchAdapter.generate_jsonc_config(&theme) {
            Ok(expected) => {
                let exact = content == expected;
                let same = exact || same_preset_tokens(&content, &expected);
                report.add_code("preset_match", if same { "ok" } else { "warning" },
                    if exact { "Preset exactly matches the saved theme's generated configuration; this does not prove Fastfetch uses it" }
                    else if same { "Preset matches generated configuration apart from comments and JSON whitespace; this does not prove Fastfetch uses it" }
                    else { "Preset differs beyond comments and JSON whitespace; edits or an older generator may explain the difference" }, &path,
                    (!same).then(|| "Review `slate tools sync fastfetch --dry-run` before replacing the managed preset. Personal layouts are not merged.".into()));
            }
            Err(_) => report.add_code(
                "preset_match",
                "error",
                "Could not generate the saved theme's expected preset; no repair attempted",
                &path,
                None,
            ),
        }
    }
    report.add_code("activation_unverified", "info", "The shell wrapper and effective --config selection were not checked; a saved preset is not proof of live activation", &path,
        Some("Use your existing Slate shell integration for a manual Fastfetch invocation. An explicit --config bypasses its preset. Sync does not enable startup autorun or install Fastfetch.".into()));
}

#[cfg(test)]
mod tests {
    use super::same_preset_tokens;
    #[test]
    fn preset_tokens_ignore_only_comments_and_json_whitespace() {
        let expected = r#"{"url":"https://example.test/*literal*/","n":1}"#;
        assert!(same_preset_tokens(
            &format!("// heading\n /*comment*/ {expected}\r\n"),
            expected
        ));
        assert!(same_preset_tokens(
            &expected.replace(":1", ": /* note */ 1"),
            expected
        ));
        for changed in [
            expected.replace("literal", "changed"),
            expected.replace(":1", ":2"),
            expected.replace(",", ""),
            expected.replace(",", ",\"n\":1,"),
            expected.replace(":1", ":\u{a0}1"),
            expected.replace(":1", ":+1"),
            format!("{expected} /* unfinished"),
            format!("{expected} trailing"),
            "[".repeat(10000),
        ] {
            assert!(!same_preset_tokens(&changed, expected));
        }
    }
}
