/// WCAG 2.1 contrast ratio audit for theme colors.
/// Reference: https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html
/// Implements relative_luminance and contrast_ratio calculations
/// for verification and warning generation during theme application.
use crate::theme::Palette;

/// Contrast audit result for a single color pair.
#[derive(Debug, Clone)]
pub struct ContrastAudit {
    pub color_name: String,
    pub foreground: String,
    pub background: String,
    pub ratio: f64,
    pub is_accessible: bool,
}

/// Detailed audit failure entry for reporting.
#[derive(Debug, Clone)]
pub struct AuditFailure {
    pub theme_id: String,
    pub color_name: String,
    pub foreground: String,
    pub background: String,
    pub ratio: f64,
}

/// Audit all colors in a palette for WCAG accessibility.
/// Returns list of audit results with warnings for colors below 4.5:1 contrast.
/// Warning mode: all issues reported but non-blocking.
pub fn audit_palette(palette: &Palette) -> Vec<ContrastAudit> {
    let mut audits = Vec::new();

    // Check foreground vs background (primary contrast)
    let fg_rgb = hex_to_rgb(&palette.foreground);
    let bg_rgb = hex_to_rgb(&palette.background);

    if let (Ok(fg), Ok(bg)) = (fg_rgb, bg_rgb) {
        let ratio = contrast_ratio(
            relative_luminance(fg.0, fg.1, fg.2),
            relative_luminance(bg.0, bg.1, bg.2),
        );

        audits.push(ContrastAudit {
            color_name: "foreground".to_string(),
            foreground: palette.foreground.clone(),
            background: palette.background.clone(),
            ratio,
            is_accessible: ratio >= 4.5,
        });
    }

    // Check ANSI colors against background
    let color_pairs = vec![
        ("black", &palette.black),
        ("red", &palette.red),
        ("green", &palette.green),
        ("yellow", &palette.yellow),
        ("blue", &palette.blue),
        ("magenta", &palette.magenta),
        ("cyan", &palette.cyan),
        ("white", &palette.white),
    ];

    for (name, color) in color_pairs {
        if let (Ok(fg), Ok(bg)) = (hex_to_rgb(color), hex_to_rgb(&palette.background)) {
            let ratio = contrast_ratio(
                relative_luminance(fg.0, fg.1, fg.2),
                relative_luminance(bg.0, bg.1, bg.2),
            );

            audits.push(ContrastAudit {
                color_name: name.to_string(),
                foreground: color.clone(),
                background: palette.background.clone(),
                ratio,
                is_accessible: ratio >= 4.5,
            });
        }
    }

    audits
}

/// Generate detailed WCAG audit report for all themes.
/// Returns all failures with theme ID and color information.
pub fn generate_full_audit_report(registry: &crate::theme::ThemeRegistry) -> Vec<AuditFailure> {
    let mut all_failures = Vec::new();

    for theme in registry.all() {
        let audits = audit_palette(&theme.palette);

        for audit in audits {
            if !audit.is_accessible {
                all_failures.push(AuditFailure {
                    theme_id: theme.id.clone(),
                    color_name: audit.color_name,
                    foreground: audit.foreground,
                    background: audit.background,
                    ratio: audit.ratio,
                });
            }
        }
    }

    all_failures
}

/// Log WCAG audit results (warnings only, non-blocking).
pub fn log_audit_warnings(theme_name: &str, audits: &[ContrastAudit]) {
    for audit in audits {
        if !audit.is_accessible {
            eprintln!(
                "⚠ WCAG Warning: {} color '{}' has contrast ratio {:.2} (minimum 4.5)",
                theme_name, audit.color_name, audit.ratio
            );
        }
    }
}

/// Calculate relative luminance per WCAG 2.1 formula.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let r = (r as f64) / 255.0;
    let g = (g as f64) / 255.0;
    let b = (b as f64) / 255.0;

    let r = if r <= 0.03928 {
        r / 12.92
    } else {
        ((r + 0.055) / 1.055).powf(2.4)
    };

    let g = if g <= 0.03928 {
        g / 12.92
    } else {
        ((g + 0.055) / 1.055).powf(2.4)
    };

    let b = if b <= 0.03928 {
        b / 12.92
    } else {
        ((b + 0.055) / 1.055).powf(2.4)
    };

    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Calculate contrast ratio between two luminances.
pub fn contrast_ratio(l1: f64, l2: f64) -> f64 {
    let lighter = l1.max(l2);
    let darker = l1.min(l2);
    (lighter + 0.05) / (darker + 0.05)
}

/// Convert hex color #RRGGBB to (R, G, B) tuple.
pub fn hex_to_rgb(hex: &str) -> Result<(u8, u8, u8), String> {
    if !hex.starts_with('#') || hex.len() != 7 {
        return Err(format!("Invalid hex color format: {}", hex));
    }

    let r =
        u8::from_str_radix(&hex[1..3], 16).map_err(|_| format!("Invalid hex color: {}", hex))?;
    let g =
        u8::from_str_radix(&hex[3..5], 16).map_err(|_| format!("Invalid hex color: {}", hex))?;
    let b =
        u8::from_str_radix(&hex[5..7], 16).map_err(|_| format!("Invalid hex color: {}", hex))?;

    Ok((r, g, b))
}

/// WCAG 2.1 contrast ratio between two `#RRGGBB` hex strings.
/// Composes [`hex_to_rgb`], [`relative_luminance`], and [`contrast_ratio`].
/// Returns `1.0` (the worst-case contrast — equal luminances) on malformed
/// input, so a malformed candidate naturally loses every comparison in the
/// MIN-max selector. Never panics.
pub fn contrast_hex(fg_hex: &str, bg_hex: &str) -> f64 {
    let (Ok(fg), Ok(bg)) = (hex_to_rgb(fg_hex), hex_to_rgb(bg_hex)) else {
        return 1.0;
    };
    contrast_ratio(
        relative_luminance(fg.0, fg.1, fg.2),
        relative_luminance(bg.0, bg.1, bg.2),
    )
}

/// MIN-max foreground selector.
/// Returns the candidate hex whose worst-case contrast across `pill_bgs` is
/// the highest — i.e., the one that maximises the minimum WCAG ratio across
/// the slate of backgrounds.
/// Tie-break is deterministic by candidate input order: when two candidates
/// produce the same MIN, the first one wins. Callers therefore order
/// candidates by preference (highest preference first).
/// Empty `candidates` is invalid; the caller must supply at least one. If
/// `pill_bgs` is empty the MIN is defined as `f64::INFINITY` (i.e., the
/// preferred candidate wins).
pub fn pick_min_max_fg<'a>(candidates: &[&'a str], pill_bgs: &[&str]) -> &'a str {
    debug_assert!(
        !candidates.is_empty(),
        "pick_min_max_fg requires at least one candidate"
    );

    let mut best_idx = 0usize;
    let mut best_min = f64::NEG_INFINITY;

    for (idx, candidate) in candidates.iter().enumerate() {
        let min_for_candidate = pill_bgs
            .iter()
            .map(|bg| contrast_hex(candidate, bg))
            .fold(f64::INFINITY, f64::min);

        // Strict `>` preserves first-in-input-order tie-break.
        if min_for_candidate > best_min {
            best_min = min_for_candidate;
            best_idx = idx;
        }
    }

    candidates[best_idx]
}

/// Adaptive `powerline_fg` for Light themes.
/// Picks from `[bg_darkest|black, background, foreground]` by maximising the
/// MIN WCAG 2.1 contrast across the 6 representative starship pill bgs:
/// `[cyan, mauve|extras["mauve"]|magenta, blue, yellow, red, green]`.
/// Tie-break preference: `bg_darkest` (or `black` cascade) first — this
/// preserves the traditional dark-text-on-light starship aesthetic when two
/// candidates are equally readable. Falls through to `background`, then
/// `foreground`.
/// Returns an owned `String` so the caller can avoid lifetime gymnastics
/// when one of the candidates was synthesised from a cascade (e.g. cloned
/// out of `palette.extras`).
pub fn pick_light_powerline_fg(palette: &Palette) -> String {
    // Candidate slate — preference order encodes tie-break.
    let bg_darkest = palette
        .bg_darkest
        .clone()
        .unwrap_or_else(|| palette.black.clone());
    let candidates_owned = [
        bg_darkest,
        palette.background.clone(),
        palette.foreground.clone(),
    ];
    let candidates: [&str; 3] = [
        candidates_owned[0].as_str(),
        candidates_owned[1].as_str(),
        candidates_owned[2].as_str(),
    ];

    // 6-pill slate. `mauve` cascades through `palette.mauve` -> `extras["mauve"]`
    // -> `palette.magenta` so palettes without a mauve slot do not panic and the
    // worst-case still uses a real palette accent.
    let mauve_or_magenta = palette
        .mauve
        .clone()
        .or_else(|| palette.extras.get("mauve").cloned())
        .unwrap_or_else(|| palette.magenta.clone());

    let pill_bgs_owned: [String; 6] = [
        palette.cyan.clone(),
        mauve_or_magenta,
        palette.blue.clone(),
        palette.yellow.clone(),
        palette.red.clone(),
        palette.green.clone(),
    ];
    let pill_bgs: [&str; 6] = [
        pill_bgs_owned[0].as_str(),
        pill_bgs_owned[1].as_str(),
        pill_bgs_owned[2].as_str(),
        pill_bgs_owned[3].as_str(),
        pill_bgs_owned[4].as_str(),
        pill_bgs_owned[5].as_str(),
    ];

    pick_min_max_fg(&candidates, &pill_bgs).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_luminance() {
        // Pure white
        let lum_white = relative_luminance(255, 255, 255);
        assert!((lum_white - 1.0).abs() < 0.01);

        // Pure black
        let lum_black = relative_luminance(0, 0, 0);
        assert!(lum_black < 0.01);
    }

    #[test]
    fn test_contrast_ratio() {
        // Black vs white should be 21:1
        let ratio = contrast_ratio(0.0, 1.0);
        assert!((ratio - 21.0).abs() < 0.5);
    }

    #[test]
    fn test_hex_to_rgb() {
        assert_eq!(hex_to_rgb("#FFFFFF"), Ok((255, 255, 255)));
        assert_eq!(hex_to_rgb("#000000"), Ok((0, 0, 0)));
        assert_eq!(hex_to_rgb("#FF0000"), Ok((255, 0, 0)));
    }

    #[test]
    fn contrast_hex_matches_black_white_21_to_1() {
        let ratio = contrast_hex("#000000", "#FFFFFF");
        assert!(
            (ratio - 21.0).abs() < 0.5,
            "expected ~21:1 black-on-white contrast, got {ratio}"
        );
    }

    #[test]
    fn contrast_hex_returns_one_on_malformed_input() {
        // Worst-case (1.0) lets a malformed candidate naturally lose every
        // MIN-max comparison without panicking.
        assert_eq!(contrast_hex("not-a-hex", "#FFFFFF"), 1.0);
        assert_eq!(contrast_hex("#FFFFFF", "zzz"), 1.0);
        assert_eq!(contrast_hex("#GGG", "#FFFFFF"), 1.0);
    }

    #[test]
    fn pick_min_max_fg_selects_highest_min_contrast() {
        // Black vs white pills: black wins (high contrast on white).
        let candidates = ["#000000", "#888888"];
        let bgs = ["#FFFFFF", "#EEEEEE"];
        assert_eq!(pick_min_max_fg(&candidates, &bgs), "#000000");
    }

    #[test]
    fn pick_min_max_fg_tie_break_favours_first_input() {
        // Both candidates identical → MIN equal → first in input wins.
        let candidates = ["#111111", "#111111"];
        let bgs = ["#FFFFFF"];
        assert_eq!(pick_min_max_fg(&candidates, &bgs), candidates[0]);
    }

    #[test]
    fn pick_light_powerline_fg_returns_one_of_three_candidates() {
        // Build a synthetic Light-shaped palette and confirm the picked fg is
        // one of the three intended candidates (bg_darkest / background /
        // foreground). Real-theme MIN-max coverage lives in the starship
        // adapter sweep test; this is a pure-function sanity check.
        use crate::theme::Palette;
        use std::collections::HashMap;

        let p = Palette {
            foreground: "#3c3836".to_string(),
            background: "#fbf1c7".to_string(),
            black: "#7c6f64".to_string(),
            red: "#cc241d".to_string(),
            green: "#98971a".to_string(),
            yellow: "#d79921".to_string(),
            blue: "#458588".to_string(),
            magenta: "#b16286".to_string(),
            cyan: "#689d6a".to_string(),
            white: "#7c6f64".to_string(),
            bright_black: "#928374".to_string(),
            bright_red: "#9d0006".to_string(),
            bright_green: "#79740e".to_string(),
            bright_yellow: "#b57614".to_string(),
            bright_blue: "#076678".to_string(),
            bright_magenta: "#8f3f71".to_string(),
            bright_cyan: "#427b58".to_string(),
            bright_white: "#3c3836".to_string(),
            brand_accent: "#cc241d".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            bg_dim: None,
            bg_darker: None,
            bg_darkest: Some("#3c3836".to_string()),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: None,
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            extras: HashMap::new(),
        };

        let picked = pick_light_powerline_fg(&p);
        assert!(
            picked == p.foreground
                || picked == p.background
                || picked == *p.bg_darkest.as_ref().unwrap(),
            "picked {picked} must be one of the three candidate slots"
        );
    }
}
