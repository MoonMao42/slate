use super::*;
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[test]
fn saved_opacity_refresh_reports_failures_without_retrying_or_skipping_terminals() {
    use std::cell::RefCell;
    for ghostty_fails in [false, true] {
        for kitty_fails in [false, true] {
            let calls = RefCell::new(Vec::new());
            let mut warnings = Vec::new();
            let outcome = |name, fails| {
                calls.borrow_mut().push(name);
                if fails {
                    Err(crate::error::SlateError::Internal(format!(
                        "{name} fixture failed"
                    )))
                } else {
                    Ok(())
                }
            };
            reload_saved_opacity_with(
                || outcome("ghostty", ghostty_fails),
                || outcome("kitty", kitty_fails),
                |tool, error| warnings.push((tool.to_owned(), error.to_string())),
            );
            assert_eq!(*calls.borrow(), ["ghostty", "kitty"]);
            assert_eq!(
                warnings.len(),
                usize::from(ghostty_fails) + usize::from(kitty_fails)
            );
            for (tool, message) in warnings {
                assert!(message.contains(&format!("{tool} fixture failed")));
                assert!(if tool == "ghostty" {
                    ghostty_fails
                } else {
                    kitty_fails
                });
            }
        }
    }
}

struct RecordingAdapter {
    name: &'static str,
    calls: Arc<Mutex<Vec<String>>>,
    fail_apply: bool,
}

impl ToolAdapter for RecordingAdapter {
    fn tool_name(&self) -> &'static str {
        self.name
    }
    fn is_installed(&self) -> Result<bool> {
        Ok(true)
    }
    fn integration_config_path(&self) -> Result<PathBuf> {
        panic!("native path")
    }
    fn managed_config_path(&self) -> PathBuf {
        panic!("native path")
    }
    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }
    fn apply_theme(&self, _: &ThemeVariant) -> Result<ApplyOutcome> {
        panic!("native apply")
    }
    fn apply_theme_with_env(&self, _: &ThemeVariant, _: &SlateEnv) -> Result<ApplyOutcome> {
        if self.fail_apply {
            return Err(crate::error::SlateError::Internal(
                "fixture apply failed".into(),
            ));
        }
        Ok(ApplyOutcome::applied_no_shell())
    }
    fn reload_with_env(&self, _: &SlateEnv) -> Result<()> {
        self.calls.lock().unwrap().push(self.name.into());
        Ok(())
    }
}

#[test]
fn picker_defers_only_successful_terminal_reload_and_preserves_other_paths() {
    for defer in [false, true] {
        for fail in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::from_vars(|key| {
                (key == "HOME").then(|| home.path().as_os_str().to_owned())
            })
            .unwrap();
            let calls = Arc::new(Mutex::new(Vec::new()));
            let mut registry = ToolRegistry::new();
            for name in ["ghostty", "kitty", "tmux", "ls_colors"] {
                registry.register(Box::new(RecordingAdapter {
                    name,
                    calls: calls.clone(),
                    fail_apply: fail && name == "ls_colors",
                }));
            }
            let theme = crate::theme::nord::nord().unwrap();
            let mut report = apply_theme_with_registry(
                &env,
                &theme,
                ThemeApplyOptions {
                    snapshot_policy: SnapshotPolicy::Create,
                    target_tools: None,
                },
                true,
                AutoPairPolicy::Preserve,
                defer,
                &registry,
            )
            .unwrap();
            let observed = calls.lock().unwrap().clone();
            assert_eq!(observed.iter().filter(|id| *id == "tmux").count(), 1);
            for name in ["ghostty", "kitty"] {
                assert_eq!(
                    observed.iter().filter(|id| *id == name).count(),
                    usize::from(!defer || fail)
                );
            }
            if defer && !fail {
                assert!(report.ensure_no_failures().is_ok());
                reload_applied_targets(&env, &registry, &mut report, ReloadScope::Terminal);
                let completed = calls.lock().unwrap();
                for name in ["ghostty", "kitty", "tmux"] {
                    assert_eq!(completed.iter().filter(|id| *id == name).count(), 1);
                }
            }
        }
    }
}

#[test]
fn deferred_reload_respects_session_gates_and_retains_errors() {
    for session in ["local", "remote", "isolated"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.path().as_os_str().to_owned()),
            "SLATE_HOME" if session == "isolated" => Some(home.path().as_os_str().to_owned()),
            "SSH_CONNECTION" if session == "remote" => Some("fixture".into()),
            _ => None,
        })
        .unwrap();
        let mut report = ThemeApplyReport {
            results: ["ghostty", "kitty", "tmux"]
                .into_iter()
                .map(|name| ToolApplyResult {
                    tool_name: name.into(),
                    status: ToolApplyStatus::Applied,
                    requires_new_shell: false,
                })
                .collect(),
            commit_failure: None,
            reload_warnings: vec![],
            restore_point_id: None,
        };
        let mut calls = vec![];
        reload_applied_targets_with(&env, &mut report, ReloadScope::Terminal, |name| {
            calls.push(name.to_owned());
            Err(crate::error::SlateError::Internal(
                "fixture reload denied".into(),
            ))
        });
        assert_eq!(calls.len(), if session == "local" { 2 } else { 0 });
        assert_eq!(
            report.reload_warnings.len(),
            if session == "isolated" { 0 } else { 2 }
        );
        assert!(report
            .reload_warnings
            .iter()
            .all(|warning| warning.informational == (session == "remote")));
        if session == "local" {
            assert!(report
                .reload_warnings
                .iter()
                .all(|warning| warning.message.contains("fixture reload denied")));
        }
    }
}
