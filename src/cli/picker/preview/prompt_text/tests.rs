use super::*;

// SWATCH-RENDERER: test-only validation of external prompt styling bytes.
fn assert_display_only(output: &str) {
    let mut chars = output.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            assert_eq!(chars.next(), Some('['));
            let mut params = String::new();
            loop {
                let next = chars.next().expect("complete SGR");
                if next == 'm' {
                    break;
                }
                assert!(next.is_ascii_digit() || matches!(next, ';' | ':'));
                params.push(next);
            }
            assert!(params.len() <= MAX_SGR_PARAMETERS && numeric_sgr(&params));
        } else {
            assert!(ch == '\n' || !ch.is_control());
            assert!(!matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'));
        }
    }
}

#[test]
// SWATCH-RENDERER: test fixtures for external prompt colors, not brand colors.
fn prompt_output_preserves_unicode_and_bounded_sgr_styles() {
    let prompt = "\n\x1b[1;3;4m目录 👩‍💻❯\x1b[0m\r\n\x1b[38;5;183m256\x1b[48;2;1;2;3mRGB\x1b[38:2::10:20:30mcolon\x1b[m\ttext";
    let expected = prompt.replace('\r', "").replace('\t', "    ");
    assert_eq!(sanitize(prompt), expected);
    assert_eq!(sanitize(&expected), expected);
    assert_display_only(&expected);
    assert_eq!(
        sanitize("أهلاً中文\u{202e}txt\u{2066}name"),
        "أهلاً中文\\u{202e}txt\\u{2066}name"
    );
}

#[test]
// SWATCH-RENDERER: hostile terminal-control fixtures are never sent to a terminal.
fn prompt_output_discards_terminal_actions_and_control_string_payloads() {
    for sequence in [
        "\x1b[2J",
        "\x1b[99;1H",
        "\x1b[?1049l",
        "\x1b[?25l",
        "\x1b[6n",
        "\x1b[22;0t",
        "\x1b]52;c;PRIVATE_CLIPBOARD\x07",
        "\x1b]0;PRIVATE_TITLE\x1b\\",
        "\x1b]8;;https://invalid.example/PRIVATE_LINK\x1b\\",
        "\x1bPPRIVATE_DCS\x1b\\",
        "\x1bXPRIVATE_SOS\x1b\\",
        "\x1b^PRIVATE_PM\x1b\\",
        "\x1b_PRIVATE_APC\x1b\\",
        "\x1bPtmux;\x1b\x1b]52;c;PRIVATE_NESTED\x07\x1b\\",
        "\x1b(B",
        "\x1b#8",
        "\x1b7",
        "\x1b8",
        "\x1bc",
        "\u{9d}52;c;PRIVATE_C1\u{9c}",
        "\u{90}PRIVATE_C1_DCS\u{9c}",
        "\u{9b}31m",
        "\x07\x08\r\x0b\x0c\x0e\x0f\x7f",
    ] {
        let input = format!("before{sequence}after");
        assert_eq!(sanitize(&input), "beforeafter", "{sequence:?}");
    }
    assert_eq!(
        sanitize("\x1b]8;;https://invalid.example\x1b\\visible link\x1b]8;;\x1b\\"),
        "visible link"
    );
}

#[test]
// SWATCH-RENDERER: malformed external styling fixtures for the display filter.
fn prompt_output_drops_malformed_incomplete_and_oversized_sequences() {
    for input in [
        "text\x1b",
        "text\x1b[31",
        "text\x1b]52;c;PRIVATE_UNTERMINATED",
        "text\x1bPPRIVATE_UNTERMINATED\x1b",
    ] {
        assert_eq!(sanitize(input), "text");
    }
    for sequence in ["\x1b[?31m", "\x1b[1$m", "\x1b[99999m", "\x1b[31\x07m"] {
        assert_eq!(sanitize(&format!("a{sequence}b")), "ab");
    }
    let accepted = format!("\x1b[{}m", ";".repeat(MAX_SGR_PARAMETERS));
    assert_eq!(sanitize(&accepted), accepted);
    let rejected = format!("\x1b[{}mtail", ";".repeat(MAX_SGR_PARAMETERS + 1));
    assert_eq!(sanitize(&rejected), "tail");
    assert_eq!(sanitize("\x1b[31\x1b[32mgreen"), "\x1b[32mgreen");
}

#[test]
// SWATCH-RENDERER: mixed external styling fixtures for the output invariant.
fn prompt_output_only_emits_complete_allowed_controls_for_mixed_inputs() {
    let atoms = [
        "plain",
        "\x1b",
        "[",
        "]",
        "P",
        "_",
        "31m",
        "?25l",
        "52;c;PRIVATE",
        "\x07",
        "\x1b\\",
        "\u{9b}",
        "\u{9d}",
        "\u{9c}",
        "\n",
        "\r",
        "\u{202e}",
        "🦀",
        "\x18",
        "\x1b[38:2::1:2:3m",
    ];
    for a in atoms {
        for b in atoms {
            for c in atoms {
                let output = sanitize(&format!("{a}{b}{c}"));
                assert_display_only(&output);
                assert_eq!(sanitize(&output), output);
            }
        }
    }
}
