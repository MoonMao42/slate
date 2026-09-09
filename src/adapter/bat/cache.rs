//! Run the detected binary with the same asset/cache paths as the writer.
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
};
use std::{path::Path, process::Command, time::Duration};

pub(super) const BUILD_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(30),
    max_output: 256 * 1024,
};

pub(super) struct CacheBuild(Command);

fn failure(details: impl Into<String>) -> SlateError {
    SlateError::ConfigWriteError(
        "bat cache --build".into(),
        format!("{}. Theme files may already have been written; the external cache is not rolled back. Inspect bat's cache/config directories and retry the theme apply.", details.into()),
    )
}

impl CacheBuild {
    pub fn prepare(binary: &Path, config_dir: &Path, env: &SlateEnv) -> Result<Self> {
        env.validate_bat_paths()?;
        // Bare/relative PATH entries must not trigger a second PATH search.
        let binary = std::path::absolute(binary)?;
        let mut command = Command::new(binary);
        command
            .env("HOME", env.home())
            .env("XDG_CONFIG_HOME", env.xdg_config_home())
            .env("XDG_CACHE_HOME", env.cache_dir())
            .env("BAT_CONFIG_DIR", config_dir)
            .env("BAT_CACHE_PATH", env.bat_cache_dir())
            .env("BAT_CONFIG_PATH", env.bat_config_path())
            .env_remove("BAT_THEME")
            .env_remove("BAT_OPTS")
            .args(["cache", "--build"]);
        Ok(Self(command))
    }

    pub fn run(&mut self, limits: Limits) -> Result<()> {
        let out = process_output::capture(&mut self.0, limits).map_err(|error| {
            failure(format!(
                "Could not run the detected bat executable: {}",
                error.kind()
            ))
        })?;
        match out.completion {
            Completion::TimedOut => Err(failure(format!(
                "Cache rebuild exceeded its {} ms post-spawn deadline",
                limits.timeout.as_millis()
            ))),
            Completion::OutputLimit => Err(failure(format!(
                "Cache rebuild exceeded its {} byte output limit",
                limits.max_output
            ))),
            Completion::Exited(status) if !status.success() => Err(failure(format!(
                "Cache rebuild exited with {status}; native output omitted"
            ))),
            Completion::Exited(_) => {
                // Upstream's no-build-assets binary prints this message and
                // exits ZERO. Do not report that known no-op as an applied cache.
                let unavailable = b"bat has been built without the 'build-assets' feature";
                if [&out.stdout, &out.stderr]
                    .iter()
                    .any(|bytes| bytes.windows(unavailable.len()).any(|w| w == unavailable))
                {
                    Err(failure(
                        "This bat build does not support rebuilding custom assets",
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn bat_cache_bounded_failures_do_not_replay_native_output() {
        for (body, expected) in [
            ("printf 'PRIVATE\\033[2J'; printf 'PRIVATE' >&2; exit 7", "exit status: 7"),
            ("while :; do :; done", "deadline"),
            ("i=0; while [ \"$i\" -lt 100 ]; do printf 'PRIVATEPRIVATEPRIVATE'; i=$((i+1)); done", "output limit"),
            ("printf \"bat has been built without the 'build-assets' feature. PRIVATE\\n\"", "does not support"),
        ] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let bin = td.path().join("fake-bat");
            fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
            let mut build = CacheBuild::prepare(&bin, env.bat_config_dir(), &env).unwrap();
            let error = build.run(Limits { timeout: Duration::from_secs(2), max_output: 1024 }).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("PRIVATE") && !error.contains('\x1b'), "{error}");
            assert!(!env.bat_config_dir().exists());
            assert!(!env.bat_cache_dir().exists());
        }
    }

    #[test]
    fn bat_cache_spawn_errors_are_fatal_without_raw_paths() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let mut build = CacheBuild::prepare(
            &td.path().join("PRIVATE-missing"),
            env.bat_config_dir(),
            &env,
        )
        .unwrap();
        let error = build.run(BUILD_LIMITS).unwrap_err().to_string();
        assert!(error.contains("Could not run"));
        assert!(!error.contains("PRIVATE"));
        assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    }
}
