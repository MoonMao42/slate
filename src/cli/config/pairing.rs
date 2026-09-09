use crate::{
    cli::auto_theme_resolution as resolution,
    config::{
        pairing::{self, PreparedPairing, SlotEdit},
        AutoConfig,
    },
    env::SlateEnv,
    error::Result,
    theme::{ThemeAppearance, ThemeRegistry},
};
use serde::Serialize;
use std::io::Write;

#[derive(clap::Args, Default)]
#[command(group(clap::ArgGroup::new("pairing_change").args(["dark", "light", "clear_dark", "clear_light"]).multiple(true)))]
pub struct PairingOptions {
    /// Save an exact dark-theme ID (unspecified light pairing is preserved)
    #[arg(long, value_name = "THEME_ID")]
    pub dark: Option<String>,
    /// Save an exact light-theme ID (unspecified dark pairing is preserved)
    #[arg(long, value_name = "THEME_ID")]
    pub light: Option<String>,
    /// Remove the saved dark override and use normal automatic fallback
    #[arg(long, conflicts_with = "dark")]
    pub clear_dark: bool,
    /// Remove the saved light override and use normal automatic fallback
    #[arg(long, conflicts_with = "light")]
    pub clear_light: bool,
    /// Preview selected changes without writing or creating a checkpoint
    #[arg(long, requires = "pairing_change")]
    pub dry_run: bool,
    /// Emit versioned inspection, preview or save receipt JSON
    #[arg(long)]
    pub json: bool,
}

impl PairingOptions {
    pub fn validate(&self) -> Result<()> {
        if (self.clear_dark && self.dark.is_some()) || (self.clear_light && self.light.is_some()) {
            return Err(crate::error::SlateError::InvalidConfig(
                "A pairing slot cannot be set and cleared in the same request.".into(),
            ));
        }
        if self.dry_run && !self.changes() {
            return Err(crate::error::SlateError::InvalidConfig(
                "Pairing preview requires --dark, --light, --clear-dark or --clear-light.".into(),
            ));
        }
        pairing::validate(self.dark.as_deref(), self.light.as_deref())
    }
    fn changes(&self) -> bool {
        self.dark.is_some() || self.light.is_some() || self.clear_dark || self.clear_light
    }
    fn edits(&self) -> (SlotEdit<'_>, SlotEdit<'_>) {
        fn slot(value: Option<&str>, clear: bool) -> SlotEdit<'_> {
            if clear {
                SlotEdit::Clear
            } else {
                value.map_or(SlotEdit::Keep, SlotEdit::Set)
            }
        }
        (
            slot(self.dark.as_deref(), self.clear_dark),
            slot(self.light.as_deref(), self.clear_light),
        )
    }
}

const SCOPE: &str = "Saved pairing, not the active theme or watcher state. Inspection also explains conditional dark/light choices using the same policy as automatic application; these are not a configuration snapshot, an apply-readiness check or proof of the current desktop appearance. No system appearance is queried. Saving does not enable/restart the watcher or apply a theme. The next appearance event reads the new pairing; use `slate theme --auto` to apply now. Recovery restores files, not processes.";

#[derive(Serialize)]
struct Slot {
    status: &'static str,
    theme_id: Option<String>,
    name: Option<String>,
    appearance: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue: Option<&'static str>,
}
#[derive(Serialize)]
struct Pair {
    dark: Slot,
    light: Slot,
}

fn entries(registry: &ThemeRegistry, config: Option<&AutoConfig>) -> Pair {
    let slot = |id: Option<&String>| match (config, id) {
        (None, _) => Slot {
            status: "error",
            theme_id: None,
            name: None,
            appearance: None,
            issue: Some(
                "Cannot read auto.toml safely or parse its pairing fields; contents omitted.",
            ),
        },
        (_, None) => Slot {
            status: "unset",
            theme_id: None,
            name: None,
            appearance: None,
            issue: None,
        },
        (_, Some(id)) => match registry.get(id) {
            Some(theme) => Slot {
                status: "set",
                theme_id: Some(theme.id.clone()),
                name: Some(theme.name.clone()),
                appearance: Some(match theme.appearance {
                    ThemeAppearance::Dark => "dark",
                    ThemeAppearance::Light => "light",
                }),
                issue: None,
            },
            None => Slot {
                status: "error",
                theme_id: None,
                name: None,
                appearance: None,
                issue: Some("Saved theme ID is not in the current catalog; contents omitted."),
            },
        },
    };
    Pair {
        dark: slot(config.and_then(|config| config.dark_theme.as_ref())),
        light: slot(config.and_then(|config| config.light_theme.as_ref())),
    }
}

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    action: &'static str,
    path: String,
    path_is_lossy: bool,
    pairing: Pair,
    #[serde(skip_serializing_if = "Option::is_none")]
    before: Option<Pair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore_point_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<resolution::Report>,
    scope: &'static str,
}

pub fn handle(env: &SlateEnv, options: &PairingOptions) -> Result<()> {
    options.validate()?;
    let registry = ThemeRegistry::new()?;
    let path = env.managed_file("auto.toml");
    let mut report = Report {
        schema_version: 1,
        action: "inspect",
        path: path.display().to_string(),
        path_is_lossy: path.to_str().is_none(),
        pairing: entries(&registry, None),
        before: None,
        changed: None,
        restore_point_id: None,
        resolution: None,
        scope: SCOPE,
    };
    if options.changes() {
        let (dark, light) = options.edits();
        let plan = PreparedPairing::capture_edits(env, dark, light)?;
        report.changed = Some(plan.changed());
        report.before = Some(entries(&registry, Some(&plan.before)));
        report.pairing = entries(&registry, Some(&plan.after));
        if options.dry_run {
            report.action = "preview";
        } else {
            report.restore_point_id = plan.save()?;
            report.action = if plan.changed() { "saved" } else { "unchanged" };
        }
    } else {
        let pairing = pairing::inspect(env).ok();
        report.pairing = entries(&registry, pairing.as_ref());
        report.resolution = Some(resolution::inspect(env, &registry, pairing.as_ref()));
    }
    let output = if options.json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text(&report)
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn text(report: &Report) -> String {
    use std::fmt::Write;
    let mut text = format!("Pairing: {}\n", report.action);
    for (label, slot) in [
        ("dark", &report.pairing.dark),
        ("light", &report.pairing.light),
    ] {
        let value = slot.theme_id.as_deref().unwrap_or(slot.status);
        let previous = report.before.as_ref().map(|pair| {
            if label == "dark" {
                &pair.dark
            } else {
                &pair.light
            }
        });
        if let Some(previous) = previous {
            let previous = previous.theme_id.as_deref().unwrap_or(previous.status);
            if previous == value {
                let _ = writeln!(text, "  {label}: {value} (unchanged)");
            } else {
                let _ = writeln!(text, "  {label}: {previous} -> {value}");
            }
        } else {
            let _ = writeln!(text, "  {label}: {value}");
        }
        if let Some(appearance) = slot.appearance.filter(|appearance| *appearance != label) {
            let _ = writeln!(text, "    Saved theme has {appearance} appearance.");
        }
        if let Some(issue) = slot.issue {
            let _ = writeln!(text, "    {issue}");
        }
    }
    if let Some(resolution) = &report.resolution {
        resolution::append_text(&mut text, resolution);
    }
    if report.action == "preview" {
        let _ = writeln!(
            text,
            "{}",
            if report.changed == Some(true) {
                "Would update auto.toml after creating a pre-config checkpoint; no files changed."
            } else {
                "No file update or new checkpoint needed; no files changed."
            }
        );
    }
    if let Some(id) = &report.restore_point_id {
        let _ = writeln!(text, "Inspect file recovery: slate restore {id} --dry-run");
    }
    let _ = writeln!(
        text,
        "{}{}\n{}",
        super::catalog::escaped(&report.path),
        if report.path_is_lossy {
            " (lossy display; not an exact path)"
        } else {
            ""
        },
        report.scope
    );
    text
}
