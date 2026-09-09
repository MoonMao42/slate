use super::{tool_files, Report};
use crate::{
    adapter::{yazi::config, YaziAdapter},
    config::file_read::MAX_TOOL_CONFIG_BYTES,
    env::SlateEnv,
};

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only Yazi file checks; no tool is launched or configuration changed. Checks use Yazi 26.9.1 flavor semantics, not a native version probe. Custom profiles follow the captured environment. Personal style sections may override flavor defaults; their effective merge and running appearance are not evaluated. Files are observed separately, not atomically.";
    tool_files::availability(report, env, "yazi");
    let theme = tool_files::saved_theme(report, env);
    let path = YaziAdapter::config_path(env);
    if let Ok(content) = tool_files::read(
        report,
        env,
        &path,
        MAX_TOOL_CONFIG_BYTES,
        "config_file",
        "Yazi theme configuration",
    ) {
        match config::inspect_selection(content.as_deref().unwrap_or("").as_bytes()) {
            Ok((selected, personal)) => {
                for (index, code) in ["flavor_dark", "flavor_light"].into_iter().enumerate() {
                    let slot = ["flavor.dark", "flavor.light"][index];
                    report.add_code(code, if selected[index] { "ok" } else { "warning" },
                        if selected[index] { format!("{slot} selects Slate's flavor") }
                        else { format!("{slot} does not select Slate's flavor; absent and personal choices are not treated as connected") }, &path,
                        (!selected[index]).then(|| "If Slate colors are wanted, review `slate tools sync yazi --dry-run` and asset ownership first.".into()));
                }
                report.add_code("personal_overrides", if personal { "warning" } else { "info" },
                    if personal { "Other theme.toml sections are present and may override flavor defaults; their effects were not evaluated" }
                    else { "No other top-level theme.toml sections were found; runtime appearance remains unverified" }, &path,
                    personal.then(|| "Review personal styles locally if colors differ. Sync preserves them; do not remove them just to silence this advisory.".into()));
            }
            Err(_) => report.add_code("config_syntax", "error", "Cannot interpret Yazi TOML/flavor slots; file contents omitted", &path,
                Some("Review malformed TOML, table shape and non-string flavor selections locally. No connection is inferred.".into())),
        }
    }
    let expected = theme
        .as_ref()
        .and_then(|theme| match config::generated_assets(theme) {
            Ok(assets) => Some(assets),
            Err(_) => {
                report.add_code(
                    "palette_generation",
                    "error",
                    "Cannot generate a comparison palette; no match is claimed",
                    &path,
                    None,
                );
                None
            }
        });
    tool_files::generated_asset(
        report,
        env,
        "yazi",
        &YaziAdapter::flavor_path(env),
        ["flavor_file", "flavor_ownership", "flavor_match"],
        config::owns_flavor,
        expected.as_ref().map(|(flavor, _)| flavor.as_str()),
    );
    tool_files::generated_asset(
        report,
        env,
        "yazi",
        &YaziAdapter::syntax_path(env),
        ["syntax_file", "syntax_ownership", "syntax_match"],
        config::owns_syntax,
        expected.as_ref().map(|(_, syntax)| syntax.as_str()),
    );
    report.add_code("reload", "info", "A running Yazi instance has not been inspected or reloaded", &path,
        Some("After syncing, reopen Yazi using this profile. Personal theme overrides still win; a byte-matching asset does not prove the live view uses it.".into()));
}
