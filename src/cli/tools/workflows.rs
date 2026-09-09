//! Optional discovery groups; never installation bundles or sync selections.
use super::*;

pub(super) struct Group {
    pub label: &'static str,
    english_label: &'static str,
    hint: &'static str,
    english_hint: &'static str,
    tools: &'static [&'static str],
}

impl Group {
    pub fn menu_label(&self) -> &'static str {
        tr(self.label, self.english_label)
    }

    pub fn tool_ids(&self) -> &'static [&'static str] {
        self.tools
    }

    pub fn contains(&self, id: &str) -> bool {
        self.tools.contains(&id)
    }
}

static GROUPS: [Group; 5] = [
    Group {
        label: "终端窗口",
        english_label: "Terminal Windows",
        hint: "调整窗口配色 · Ghostty、Alacritty、Kitty",
        english_hint: "Window colors · Ghostty, Alacritty, Kitty",
        tools: &["ghostty", "alacritty", "kitty"],
    },
    Group {
        label: "命令提示符与 Shell",
        english_label: "Prompt and Shell",
        hint: "提示符、文件列表、启动信息与输入着色 · Starship、eza、Fastfetch、Zsh",
        english_hint: "Prompt, file listings and shell colors · Starship, eza, Fastfetch, Zsh",
        tools: &["starship", "eza", "fastfetch", "zsh-syntax-highlighting"],
    },
    Group {
        label: "文件与系统",
        english_label: "Files and System",
        hint: "查看文件、浏览目录与监控资源 · bat、Yazi、btop",
        english_hint: "Files, directories and resources · bat, Yazi, btop",
        tools: &["bat", "yazi", "btop"],
    },
    Group {
        label: "开发工具",
        english_label: "Development Tools",
        hint: "比较代码、管理 Git 与编辑代码 · Delta、Lazygit、Neovim、OpenCode",
        english_hint: "Diffs, Git and editing · Delta, Lazygit, Neovim, OpenCode",
        tools: &["delta", "lazygit", "nvim", "opencode"],
    },
    Group {
        label: "分屏会话",
        english_label: "Sessions and Panes",
        hint: "管理会话、标签页与分屏 · tmux、Zellij",
        english_hint: "Sessions, tabs and panes · tmux, Zellij",
        tools: &["tmux", "zellij"],
    },
];

pub(super) fn browse(env: &SlateEnv) -> Result<()> {
    browse_with(choose, |group| super::browse_group(env, Some(group)))
}

fn browse_with(
    mut choose: impl FnMut(Option<&'static Group>) -> Result<Option<&'static Group>>,
    mut open: impl FnMut(&'static Group) -> Result<()>,
) -> Result<()> {
    let mut previous = None;
    while let Some(group) = choose(previous)? {
        open(group)?;
        previous = Some(group);
    }
    Ok(())
}

fn choose(previous: Option<&Group>) -> Result<Option<&'static Group>> {
    crate::cli::file_output::write_output(tr(
        "先选用途，再查看工具。浏览不会安装软件或应用配色。\n",
        "Browse by workflow; nothing is installed or applied.\n",
    ))?;
    let mut menu =
        crate::cli::menu::select(tr("你想改善哪一部分？", "Choose a Workflow")).escape_value(None);
    if let Some(index) = previous.and_then(|previous| {
        GROUPS
            .iter()
            .position(|group| std::ptr::eq(group, previous))
    }) {
        menu = menu.initial_value(Some(index));
    }
    for (index, group) in GROUPS.iter().enumerate() {
        menu = menu.item(
            Some(index),
            group.menu_label(),
            tr(group.hint, group.english_hint),
        );
    }
    let selected = menu
        .item(None, tr("返回工具菜单", "Back to Tools"), "")
        .interact()
        .map_err(input_error)?;
    Ok(selected.map(|index| &GROUPS[index]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_return_keeps_context_and_stops_on_cancel_or_failure() {
        let mut choices = [Some(&GROUPS[4]), Some(&GROUPS[0]), None].into_iter();
        let mut previous_labels = Vec::new();
        let mut opened = Vec::new();
        browse_with(
            |previous| {
                previous_labels.push(previous.map(|group| group.label));
                Ok(choices.next().unwrap())
            },
            |group| {
                opened.push(group.label);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(opened, [GROUPS[4].label, GROUPS[0].label]);
        assert_eq!(
            previous_labels,
            [None, Some(GROUPS[4].label), Some(GROUPS[0].label)]
        );
        assert!(matches!(
            browse_with(
                |_| Err(SlateError::UserCancelled),
                |_| panic!("opened after cancel")
            ),
            Err(SlateError::UserCancelled)
        ));
        let mut attempts = 0;
        assert!(browse_with(
            |_| {
                attempts += 1;
                Ok(Some(&GROUPS[0]))
            },
            |_| Err(SlateError::ConfigurationBusy)
        )
        .is_err());
        assert_eq!(
            attempts, 1,
            "failed group must not be retried automatically"
        );
    }

    #[test]
    fn workflow_groups_cover_every_adapter_exactly_once_without_action_ids() {
        let supported = supported_tools();
        let mut grouped = Vec::new();
        for group in &GROUPS {
            assert!(!group.tools.is_empty());
            assert!(group.tools.len() < 8, "group plus Back fits the menu");
            for id in group.tools {
                assert!(supported.contains(id), "unknown adapter: {id}");
                assert!(!grouped.contains(id), "duplicate adapter: {id}");
                assert!(group.contains(id));
                grouped.push(*id);
            }
            for action in ["setup", "install", "sync", "back", "workflows"] {
                assert!(!group.contains(action));
            }
        }
        grouped.sort();
        let mut expected = supported;
        expected.sort();
        assert_eq!(grouped, expected);
    }
}
