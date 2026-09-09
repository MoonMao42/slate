//! Capture the actual tool routes shown for confirmation. This is not a package
//! version/executable pin, filesystem transaction or sandbox for native installers.
use super::{BrewKind, ToolCatalog, ToolMetadata};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    platform::packages::{self, InstallContext, ToolInstallRoute},
};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct PlannedToolInstall {
    pub metadata: ToolMetadata,
    pub route: ToolInstallRoute,
}

#[derive(Clone, Debug)]
pub(crate) struct InstallPlan {
    tools: Vec<PlannedToolInstall>,
    local_binary: PathBuf,
}

fn changed(reason: impl std::fmt::Display) -> SlateError {
    SlateError::InvalidConfig(format!(
        "Tool installation plan changed: {reason}; review setup again before retrying"
    ))
}

impl InstallPlan {
    pub(crate) fn capture(ids: &[String], env: &SlateEnv, context: InstallContext) -> Result<Self> {
        let mut tools: Vec<PlannedToolInstall> = Vec::new();
        for id in ids {
            let metadata = ToolCatalog::get_tool(id)
                .filter(|tool| tool.installable)
                .ok_or_else(|| {
                    SlateError::InvalidConfig(format!(
                        "No installable catalog tool '{}'; review setup again",
                        id.escape_default()
                    ))
                })?;
            if tools.iter().any(|tool| tool.metadata.id == metadata.id) {
                continue;
            }
            let route = context.route(id).map_err(|unavailable| {
                SlateError::InvalidConfig(format!(
                    "Cannot automatically install '{}': {}",
                    id.escape_default(),
                    unavailable.reason(),
                ))
            })?;
            tools.push(PlannedToolInstall { metadata, route });
        }
        Ok(Self {
            tools,
            local_binary: env.user_local_bin().join("starship"),
        })
    }

    pub(crate) fn verify_selection(&self, selected: &[ToolMetadata], env: &SlateEnv) -> Result<()> {
        if self.local_binary != env.user_local_bin().join("starship") {
            return Err(changed(
                "user-local destination differs from the reviewed profile",
            ));
        }
        if selected.len() != self.tools.len()
            || selected.iter().zip(&self.tools).any(|(selected, planned)| {
                selected.id != planned.metadata.id
                    || selected.brew_package != planned.metadata.brew_package
                    || selected.brew_kind != planned.metadata.brew_kind
            })
        {
            return Err(changed(
                "selected tools or packages differ from the reviewed actions",
            ));
        }
        Ok(())
    }

    pub(crate) fn verify_context(&self, context: InstallContext) -> Result<()> {
        for tool in &self.tools {
            self.verify_tool(tool.metadata.id, context)?;
        }
        Ok(())
    }

    pub(crate) fn verify_tool(&self, id: &str, context: InstallContext) -> Result<()> {
        let tool = self.tool(id)?;
        if context.route(id) != Ok(tool.route) {
            return Err(changed(format!(
                "installation route for {} is no longer the reviewed route",
                tool.metadata.label
            )));
        }
        Ok(())
    }

    pub(crate) fn tool(&self, id: &str) -> Result<&PlannedToolInstall> {
        self.tools
            .iter()
            .find(|tool| tool.metadata.id == id)
            .ok_or_else(|| changed("tool is missing from reviewed actions"))
    }

    pub(crate) fn description(&self, id: &str) -> Option<String> {
        self.description_in(id, crate::config::ui_language::UiLanguage::English)
    }

    pub(crate) fn description_in(
        &self,
        id: &str,
        language: crate::config::ui_language::UiLanguage,
    ) -> Option<String> {
        let chinese = language == crate::config::ui_language::UiLanguage::Chinese;
        let tool = self.tool(id).ok()?;
        Some(match tool.route {
            ToolInstallRoute::Homebrew => format!(
                "Homebrew {} ({})",
                match tool.metadata.brew_kind {
                    BrewKind::Formula => "formula",
                    BrewKind::Cask => "cask",
                },
                tool.metadata.brew_package
            ),
            ToolInstallRoute::Apt if chinese => format!(
                "apt 软件包（{}；需要管理员权限）",
                packages::apt::package_name(id)?
            ),
            ToolInstallRoute::Apt => format!(
                "apt package ({}; administrator access)",
                packages::apt::package_name(id)?
            ),
            ToolInstallRoute::UserLocalStarship if chinese => format!(
                "用户目录中的可执行文件 {:?}（需要 curl）",
                self.local_binary
            ),
            ToolInstallRoute::UserLocalStarship => format!(
                "user-local executable at {:?} (requires curl)",
                self.local_binary
            ),
        })
    }

    pub(crate) fn fallback_description(&self) -> Option<String> {
        self.fallback_description_in(crate::config::ui_language::UiLanguage::English)
    }

    pub(crate) fn fallback_description_in(
        &self,
        language: crate::config::ui_language::UiLanguage,
    ) -> Option<String> {
        if language == crate::config::ui_language::UiLanguage::Chinese {
            return self.tools.iter().any(|tool| tool.metadata.id == "starship" && tool.route == ToolInstallRoute::Homebrew).then(|| format!(
                "Starship：已知的 Homebrew 权限或 brew 缺失错误可能改用 {:?}；安装结果不明确时会停止设置。", self.local_binary));
        }
        self.tools.iter().any(|tool| tool.metadata.id == "starship" && tool.route == ToolInstallRoute::Homebrew).then(|| format!(
            "Starship: known Homebrew permission/missing-brew failures may fall back to {:?}; unconfirmed installer outcomes stop setup.", self.local_binary,
        ))
    }
}

#[cfg(test)]
mod tests;
