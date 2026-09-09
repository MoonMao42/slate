//! Recovery guidance describes only effects possible for the selected action.
pub(super) enum ActionEffect {
    ReadOnly,
    Configuration,
    Installation,
}

impl ActionEffect {
    pub(super) fn recovery_note(&self) -> &'static str {
        self.recovery_text().get(crate::cli::ui_language::current())
    }

    fn recovery_text(&self) -> crate::config::ui_language::Text {
        use crate::config::ui_language::Text;
        match self {
            Self::ReadOnly => Text {
                zh: "返回工具页面并刷新状态，不自动重试。此次为只读检查。",
                en: "Returning to the tool page and refreshing status; no automatic retry. This was a read-only check.",
            },
            Self::Configuration => Text {
                zh: "返回工具页面，不自动重试。已确认的部分改动可能保留，请先检查结果和恢复点。",
                en: "Returning to the tool page; no automatic retry. Partial changes may remain. Check the results and recovery point first.",
            },
            Self::Installation => Text {
                zh: "返回工具页面，不自动重试。已安装的文件或软件包可能保留；文件恢复不会卸载软件，请先检查安装结果。",
                en: "Returning to the tool page; no automatic retry. Installed files or packages may remain. File recovery does not uninstall software; check the installation first.",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_guidance_matches_action_effects_without_promising_rollback() {
        for effect in [
            ActionEffect::ReadOnly,
            ActionEffect::Configuration,
            ActionEffect::Installation,
        ] {
            assert!(effect.recovery_note().contains("不自动重试"));
        }
        let read = ActionEffect::ReadOnly.recovery_note();
        assert!(!read.contains("恢复"));
        assert!(!read.contains("安装"));
        let config = ActionEffect::Configuration.recovery_note();
        assert!(config.contains("部分改动可能保留"));
        assert!(config.contains("恢复点"));
        assert!(!config.contains("安装"));
        let install = ActionEffect::Installation.recovery_note();
        assert!(install.contains("可能保留"));
        assert!(install.contains("不会卸载软件"));
    }

    #[test]
    fn english_recovery_guidance_preserves_each_actions_limits() {
        use crate::config::ui_language::UiLanguage;
        let text = |effect: ActionEffect| effect.recovery_text().get(UiLanguage::English);
        for effect in [
            ActionEffect::ReadOnly,
            ActionEffect::Configuration,
            ActionEffect::Installation,
        ] {
            assert!(text(effect).contains("no automatic retry"));
        }
        let read = text(ActionEffect::ReadOnly);
        assert!(read.contains("read-only check") && !read.contains("Partial changes"));
        let config = text(ActionEffect::Configuration);
        assert!(config.contains("Partial changes may remain") && config.contains("recovery point"));
        let install = text(ActionEffect::Installation);
        assert!(install.contains("may remain") && install.contains("does not uninstall software"));
    }
}
