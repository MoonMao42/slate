//! A cache command is a post-install outcome, never a reason to redownload or
//! delete successfully published fonts. Native font rendering is a separate gate.
use super::{backend, user_font_dir_for_backend, FontPlatformBackend};
use crate::{
    env::SlateEnv,
    platform::process_output::{self, Completion, Limits},
};
use std::{fs, io, path::PathBuf, process::Command, time::Duration};

const REFRESH_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(30),
    max_output: 64 * 1024,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[must_use = "Cache refresh is separate from file installation; report its actual outcome"]
pub enum FontCacheRefresh {
    /// No install was attempted; never infer a refresh from executable presence.
    #[default]
    NotRequested,
    NotNeeded,
    /// The command exited successfully, not proof of font matching or rendering.
    Refreshed,
    MissingDependency,
    Failed,
    CouldNotStart,
    TimedOut,
    OutputLimit,
    UnsafeDirectory,
}

impl FontCacheRefresh {
    pub fn needs_attention(self) -> bool {
        !matches!(self, Self::NotRequested | Self::NotNeeded | Self::Refreshed)
    }
}

/// Pure presentation: it consumes the result of this operation, does no PATH
/// detection or cache work, and never infers prior success for installed fonts.
pub fn activation_hint(refresh: FontCacheRefresh) -> &'static str {
    activation_hint_in(refresh, crate::config::ui_language::UiLanguage::English)
}

pub fn activation_hint_in(
    refresh: FontCacheRefresh,
    language: crate::config::ui_language::UiLanguage,
) -> &'static str {
    if language == crate::config::ui_language::UiLanguage::Chinese {
        return match refresh {
            FontCacheRefresh::NotRequested => "本次未刷新字体缓存。所选字体未显示时，可新开终端窗口。",
            FontCacheRefresh::NotNeeded => "macOS 无需刷新 fontconfig。新字体未显示时，可新开终端窗口。",
            FontCacheRefresh::Refreshed => "fontconfig 缓存命令已完成，未验证实际显示；字形异常时可新开终端窗口。",
            FontCacheRefresh::MissingDependency => "字体文件已安装，但未找到 fc-cache。请安装 fontconfig 并刷新用户字体目录，无需重新下载字体。",
            FontCacheRefresh::Failed => "字体文件已保留，但 fontconfig 缓存命令失败。请检查 fontconfig 并手动刷新用户字体目录；可能保留部分缓存改动。",
            FontCacheRefresh::CouldNotStart => "字体文件已保留，但缓存命令未能启动或捕获结果。请检查 fc-cache 后重试刷新；可能保留部分缓存改动。",
            FontCacheRefresh::TimedOut => "字体文件已保留，但 fontconfig 缓存刷新超时。请检查 fontconfig 并手动刷新用户字体目录；可能保留部分缓存改动。",
            FontCacheRefresh::OutputLimit => "字体文件已保留，但缓存命令输出超限。请检查 fontconfig 后重试刷新；原始输出已省略，可能保留部分缓存改动。",
            FontCacheRefresh::UnsafeDirectory => "字体文件已安装，但用户字体目录已缺失、成为链接或不安全。请先检查目录；未运行缓存命令，也未删除字体。",
        };
    }
    match refresh {
        FontCacheRefresh::NotRequested => "No font-cache refresh was requested. Open a new terminal window if the selected font does not appear.",
        FontCacheRefresh::NotNeeded => "No fontconfig refresh is needed on macOS. Open a new terminal window if the new font does not appear immediately.",
        FontCacheRefresh::Refreshed => "The fontconfig cache command completed successfully; font rendering is not verified. Open a new terminal window if glyphs still look wrong.",
        FontCacheRefresh::MissingDependency => "Font files were installed, but `fc-cache` was not found. Install fontconfig and refresh the user-font directory; the font files do not need downloading again.",
        FontCacheRefresh::Failed => "Font files were installed, but the fontconfig cache command failed. Check fontconfig and refresh the user-font directory manually; installed fonts were kept and partial cache writes may remain.",
        FontCacheRefresh::CouldNotStart => "Font files were installed, but the fontconfig cache command could not be started or captured. Check `fc-cache` and retry the cache refresh; installed fonts were kept and partial cache writes may remain.",
        FontCacheRefresh::TimedOut => "Font files were installed, but the fontconfig cache refresh timed out. Check fontconfig and refresh the user-font directory manually; installed fonts were kept and partial cache writes may remain.",
        FontCacheRefresh::OutputLimit => "Font files were installed, but the fontconfig cache command exceeded its output limit. Check fontconfig and retry the cache refresh; native output was omitted, installed fonts were kept and partial cache writes may remain.",
        FontCacheRefresh::UnsafeDirectory => "Font files were installed, but the user-font directory is now missing, linked or unsafe to refresh. Inspect that directory before retrying; Slate did not run the cache command or remove fonts.",
    }
}

pub fn refresh_font_cache(env: &SlateEnv) -> FontCacheRefresh {
    refresh_with(
        backend(),
        env,
        || {
            crate::detection::command_in_actual_path("fc-cache")
                .or_else(|| crate::detection::command_path("fc-cache"))
        },
        REFRESH_LIMITS,
    )
}

#[cfg(test)]
#[test]
fn activation_hint_languages_preserve_cache_outcomes() {
    use crate::config::ui_language::UiLanguage;
    let outcomes = [
        FontCacheRefresh::NotRequested,
        FontCacheRefresh::NotNeeded,
        FontCacheRefresh::Refreshed,
        FontCacheRefresh::MissingDependency,
        FontCacheRefresh::Failed,
        FontCacheRefresh::CouldNotStart,
        FontCacheRefresh::TimedOut,
        FontCacheRefresh::OutputLimit,
        FontCacheRefresh::UnsafeDirectory,
    ];
    let mut chinese = std::collections::HashSet::new();
    for outcome in outcomes {
        let en = activation_hint_in(outcome, UiLanguage::English);
        assert_eq!(en, activation_hint(outcome));
        let zh = activation_hint_in(outcome, UiLanguage::Chinese);
        assert_ne!(zh, en);
        assert!(chinese.insert(zh));
        if outcome.needs_attention() {
            assert!(zh.contains("字体文件已"));
        }
        if matches!(
            outcome,
            FontCacheRefresh::Failed
                | FontCacheRefresh::CouldNotStart
                | FontCacheRefresh::TimedOut
                | FontCacheRefresh::OutputLimit
        ) {
            assert!(zh.contains("部分缓存改动"));
        }
    }
    assert!(
        activation_hint_in(FontCacheRefresh::Refreshed, UiLanguage::Chinese)
            .contains("未验证实际显示")
    );
    assert!(
        activation_hint_in(FontCacheRefresh::UnsafeDirectory, UiLanguage::Chinese)
            .contains("未运行缓存命令")
    );
}

fn refresh_with(
    backend: FontPlatformBackend,
    env: &SlateEnv,
    resolve: impl FnOnce() -> Option<PathBuf>,
    limits: Limits,
) -> FontCacheRefresh {
    if backend != FontPlatformBackend::Fontconfig {
        return FontCacheRefresh::NotNeeded;
    }
    let Ok(directory) = refresh_directory(env) else {
        return FontCacheRefresh::UnsafeDirectory;
    };
    let Some(binary) = resolve() else {
        return FontCacheRefresh::MissingDependency;
    };
    let Ok(binary) = std::path::absolute(binary) else {
        return FontCacheRefresh::CouldNotStart;
    };
    let (Ok(home), Ok(config), Ok(cache), Ok(data)) = (
        std::path::absolute(env.home()),
        std::path::absolute(env.xdg_config_home()),
        std::path::absolute(env.cache_dir()),
        std::path::absolute(env.xdg_data_home()),
    ) else {
        return FontCacheRefresh::CouldNotStart;
    };
    let mut command = Command::new(binary);
    // Directory arguments limit the requested scan, not fontconfig's native
    // cache/config effects. Keep the selected executable and native config trusted.
    // --error-on-no-fonts prevents an empty/unsupported font set being reported
    // as refreshed. No shell interpolation or second PATH lookup is involved.
    command
        .args(["--force", "--error-on-no-fonts", "--"])
        .arg(directory)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_CACHE_HOME", cache)
        .env("XDG_DATA_HOME", data)
        .env_remove("FC_DEBUG")
        .env_remove("FC_DEBUG_MATCH_FILTER");
    match process_output::capture(&mut command, limits) {
        Ok(output) => match output.completion {
            Completion::Exited(status) if status.success() => FontCacheRefresh::Refreshed,
            Completion::Exited(_) => FontCacheRefresh::Failed,
            Completion::TimedOut => FontCacheRefresh::TimedOut,
            Completion::OutputLimit => FontCacheRefresh::OutputLimit,
        },
        Err(_) => FontCacheRefresh::CouldNotStart,
    }
}

fn refresh_directory(env: &SlateEnv) -> io::Result<PathBuf> {
    let requested = user_font_dir_for_backend(env, FontPlatformBackend::Fontconfig);
    let expected = super::install_directory(env, FontPlatformBackend::Fontconfig)?;
    // Publication already checked these ancestors. Check again before asking a
    // native scanner to follow them; this does not exclude a concurrent rename.
    if !fs::symlink_metadata(&requested)?.is_dir() || fs::canonicalize(&requested)? != expected {
        return Err(io::Error::other(
            "font directory changed or contains a link",
        ));
    }
    Ok(expected)
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
