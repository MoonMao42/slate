use super::*;
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct ProbeAdapter {
    name: &'static str,
    post_commit: bool,
    fail: bool,
    probes: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}

impl ToolAdapter for ProbeAdapter {
    fn tool_name(&self) -> &'static str {
        self.name
    }
    fn is_installed(&self) -> Result<bool> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }
    fn integration_config_path(&self) -> Result<PathBuf> {
        panic!("unused native path API")
    }
    fn managed_config_path(&self) -> PathBuf {
        panic!("unused native path API")
    }
    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }
    fn is_post_commit_notification(&self) -> bool {
        self.post_commit
    }
    fn apply_theme(&self, _: &ThemeVariant) -> Result<ApplyOutcome> {
        panic!("must use injected profile")
    }
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.post_commit {
            assert_eq!(
                fs::read_to_string(env.managed_file("current")).unwrap(),
                theme.id
            );
            assert!(env.managed_file("managed/shell/env.zsh").is_file());
        }
        if self.fail {
            return Err(crate::error::SlateError::Internal(
                "injected adapter failure".into(),
            ));
        }
        if self.post_commit {
            // Deliberately distinctive bytes: a duplicated shared tail write
            // would overwrite these, even if the final theme ID were the same.
            fs::write(
                env.slate_cache_dir().join("current_theme.lua"),
                b"one fixture notification\n",
            )?;
        }
        Ok(ApplyOutcome::applied_no_shell())
    }
}

#[test]
fn nvim_notification_stage_runs_once_after_commit_and_never_retries_failure() {
    for case in ["success", "before-commit-failure", "notification-failure"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let _config = ConfigManager::with_env(&env).unwrap();
        fs::write(env.managed_file("current"), b"nord\n").unwrap();
        let state = env.slate_cache_dir().join("current_theme.lua");
        fs::write(&state, b"old notification\n").unwrap();
        let mut registry = ToolRegistry::new();
        let probes = Arc::new(AtomicUsize::new(0));
        let notification_calls = Arc::new(AtomicUsize::new(0));
        for (name, post_commit, fail) in [
            ("ls_colors", false, case == "before-commit-failure"),
            ("nvim", true, case == "notification-failure"),
        ] {
            registry.register(Box::new(ProbeAdapter {
                name,
                post_commit,
                fail,
                probes: probes.clone(),
                calls: if post_commit {
                    notification_calls.clone()
                } else {
                    Arc::new(AtomicUsize::new(0))
                },
            }));
        }
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let report = apply_theme_with_registry(
            &env,
            &theme,
            ThemeApplyOptions {
                snapshot_policy: SnapshotPolicy::Create,
                target_tools: None,
            },
            false,
            AutoPairPolicy::RememberManualSelection,
            false,
            &registry,
        )
        .unwrap();
        assert_eq!(probes.load(Ordering::SeqCst), 2);
        let nvim = report
            .results
            .iter()
            .find(|r| r.tool_name == "nvim")
            .unwrap();
        match case {
            "success" => {
                report.ensure_no_failures().unwrap();
                assert_eq!(notification_calls.load(Ordering::SeqCst), 1);
                assert!(matches!(nvim.status, ToolApplyStatus::Applied));
                assert_eq!(fs::read(&state).unwrap(), b"one fixture notification\n");
            }
            "before-commit-failure" => {
                assert_eq!(notification_calls.load(Ordering::SeqCst), 0);
                assert!(matches!(
                    nvim.status,
                    ToolApplyStatus::Skipped(SkipReason::ThemeNotCommitted)
                ));
                assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
                assert_eq!(fs::read(&state).unwrap(), b"old notification\n");
            }
            _ => {
                assert_eq!(notification_calls.load(Ordering::SeqCst), 1);
                assert!(matches!(nvim.status, ToolApplyStatus::Failed(_)));
                assert!(report
                    .ensure_no_failures()
                    .unwrap_err()
                    .to_string()
                    .contains("already saved"));
                assert_eq!(
                    fs::read(env.managed_file("current")).unwrap(),
                    theme.id.as_bytes()
                );
                assert_eq!(fs::read(&state).unwrap(), b"old notification\n");
                assert!(
                    report.reload_warnings.is_empty(),
                    "no duplicate fallback retry"
                );
            }
        }
    }
}

#[test]
fn nvim_notification_unselected_shared_hook_failure_is_a_retained_warning() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let _config = ConfigManager::with_env(&env).unwrap();
    let external = td.path().join("preserved-target");
    fs::write(&external, b"preserved\n").unwrap();
    std::os::unix::fs::symlink(&external, env.slate_cache_dir().join("current_theme.lua")).unwrap();
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ProbeAdapter {
        name: "ls_colors",
        post_commit: false,
        fail: false,
        probes: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
    }));
    let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    let report = apply_theme_with_registry(
        &env,
        &theme,
        ThemeApplyOptions {
            snapshot_policy: SnapshotPolicy::Skip,
            target_tools: None,
        },
        false,
        AutoPairPolicy::RememberManualSelection,
        false,
        &registry,
    )
    .unwrap();
    report.ensure_no_failures().unwrap();
    assert_eq!(report.applied_count(), 1);
    assert_eq!(report.reload_warnings.len(), 1);
    assert_eq!(report.reload_warnings[0].tool_name, "nvim");
    assert!(report.reload_warnings[0].message.contains("already saved"));
    assert_eq!(
        fs::read(env.managed_file("current")).unwrap(),
        theme.id.as_bytes()
    );
    assert_eq!(fs::read(external).unwrap(), b"preserved\n");
}
