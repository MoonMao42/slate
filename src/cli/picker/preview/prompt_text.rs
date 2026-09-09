//! Display-only allowlist for external prompt text, not a terminal emulator.
//! Keep bounded numeric SGR styling; consume all other escape/control strings.
//! See https://invisible-island.net/xterm/ctlseqs/ctlseqs.html for sequence syntax.
//! Malformed/unterminated control strings are discarded, never replayed.

pub(super) const RESET: &str = "\x1b[0m";
const MAX_SGR_PARAMETERS: usize = 128;

#[derive(Clone, Copy)]
enum Mode {
    Text,
    Escape,
    EscapeIntermediate,
    Csi,
    ControlString { osc: bool, escaped: bool },
}

// SWATCH-RENDERER: preserve only validated user-prompt SGR, not Slate brand styling.
pub(super) fn sanitize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut mode = Mode::Text;
    let mut parameters = String::with_capacity(MAX_SGR_PARAMETERS);
    let mut valid_sgr = false;
    for ch in input.chars() {
        if let Mode::ControlString { osc, escaped } = mode {
            if ch == '\u{9c}' || (escaped && ch == '\\') || (osc && ch == '\x07') {
                mode = Mode::Text;
            } else {
                mode = Mode::ControlString {
                    osc,
                    escaped: ch == '\x1b',
                };
            }
            continue;
        }
        // Always discard raw control introducers; only the SGR branch below
        // can construct an ESC in the returned string. Treat decoded C1 forms
        // conservatively as controls, without turning them into styling.
        match ch {
            '\x1b' => {
                mode = Mode::Escape;
                continue;
            }
            '\u{9d}' => {
                mode = Mode::ControlString {
                    osc: true,
                    escaped: false,
                };
                continue;
            }
            '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
                mode = Mode::ControlString {
                    osc: false,
                    escaped: false,
                };
                continue;
            }
            '\u{9b}' => {
                mode = Mode::Csi;
                parameters.clear();
                valid_sgr = false;
                continue;
            }
            '\x18' | '\x1a' => {
                mode = Mode::Text;
                continue;
            }
            _ => {}
        }
        match mode {
            Mode::Text => match ch {
                '\n' => output.push('\n'),
                '\t' => output.push_str("    "),
                ch if ch.is_control() => {}
                '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
                    output.extend(ch.escape_default())
                }
                ch => output.push(ch),
            },
            Mode::Escape => match ch {
                '[' => {
                    mode = Mode::Csi;
                    parameters.clear();
                    valid_sgr = true;
                }
                ']' => {
                    mode = Mode::ControlString {
                        osc: true,
                        escaped: false,
                    }
                }
                'P' | 'X' | '^' | '_' => {
                    mode = Mode::ControlString {
                        osc: false,
                        escaped: false,
                    }
                }
                '\x20'..='\x2f' => mode = Mode::EscapeIntermediate,
                _ => mode = Mode::Text,
            },
            Mode::EscapeIntermediate => {
                if !('\x20'..='\x2f').contains(&ch) {
                    mode = Mode::Text;
                }
            }
            Mode::Csi => {
                if ('\x40'..='\x7e').contains(&ch) {
                    if ch == 'm' && valid_sgr && numeric_sgr(&parameters) {
                        output.push_str("\x1b[");
                        output.push_str(&parameters);
                        output.push('m');
                    }
                    mode = Mode::Text;
                } else if (ch.is_ascii_digit() || matches!(ch, ';' | ':'))
                    && parameters.len() < MAX_SGR_PARAMETERS
                {
                    parameters.push(ch);
                } else {
                    valid_sgr = false;
                }
            }
            Mode::ControlString { .. } => unreachable!("control strings are consumed above"),
        }
    }
    output
}

fn numeric_sgr(parameters: &str) -> bool {
    parameters
        .split([';', ':'])
        .all(|part| part.is_empty() || (part.len() <= 5 && part.parse::<u8>().is_ok()))
}

#[cfg(test)]
mod tests;
