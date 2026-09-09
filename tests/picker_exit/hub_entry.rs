use super::*;

#[test]
fn tools_entry_guidance_matches_escape_destination() {
    for from_hub in [false, true] {
        let home = TempDir::new().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let before = tree_snapshot::tree(home.path());
        let args: &[&str] = if from_hub { &[] } else { &["tools"] };
        let mut menu = PickerProcess::start_with_path(home.path(), args, Some(&bin));
        if from_hub {
            menu.wait_for_output("想调整什么？");
            let choice = menu_choice_keys(&mut menu, "想调整什么？", "工具配色");
            navigate(&mut menu, &choice, "选择工具或查看全部支持");
        } else {
            menu.wait_for_output("选择工具或查看全部支持");
        }
        let output = String::from_utf8_lossy(&menu.output);
        assert!(output.contains("浏览不修改设置"));
        assert!(!output.contains("Esc/Ctrl+C exits menus"));
        if from_hub {
            navigate(&mut menu, b"\x1b", "● 工具配色");
        }
        menu.finish(b"\x1b");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn tool_inventory_waits_for_return_and_preserves_selection_without_writes() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut menu = PickerProcess::start_with_path(home.path(), &["tools"], Some(&bin));
    menu.wait_for_output("选择工具或查看全部支持");
    for back in [b"\r".as_slice(), b"\x1b"] {
        let choice = tool_choice_keys(&mut menu, "查看工具总览");
        let offset = navigate(&mut menu, &choice, "● 返回工具菜单");
        let page = String::from_utf8_lossy(&menu.output[offset..]);
        let report = page.split_once("工具总览 · 未选择主题").unwrap().1;
        assert!(!report.contains("选择工具或查看全部支持"), "{report}");
        assert!(report.contains("仅检测结果，不代表配色已生效"));
        assert!(!report.contains("Discover one tool"));
        assert!(page.contains("\x1b[?1049h"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
        let offset = navigate(&mut menu, back, "● 查看工具总览");
        assert!(String::from_utf8_lossy(&menu.output[offset..]).contains("\x1b[?1049l"));
    }
    menu.finish(b"\x1b");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn status_menu_refresh_reads_external_changes_without_writes() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
    hub.wait_for_output("退出");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "检查配置");
    navigate(&mut hub, &keys, "● 返回主菜单");
    for theme in ["catppuccin-mocha", "unknown-fixture"] {
        fs::write(env.managed_file("current"), theme).unwrap();
        let before = tree_snapshot::tree(home.path());
        let keys = menu_choice_keys(&mut hub, "检查配置", "刷新检查");
        let offset = navigate(&mut hub, &keys, "● 刷新检查");
        let text = String::from_utf8_lossy(&hub.output[offset..]);
        assert!(text.contains(if theme == "catppuccin-mocha" {
            "Catppuccin Mocha"
        } else {
            "unknown-fixture"
        }));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    navigate(&mut hub, b"\x1b", "● 检查配置");
    hub.finish(b"\x1b");
    assert_eq!(
        fs::read(env.managed_file("current")).unwrap(),
        b"unknown-fixture"
    );
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn hub_restore_defers_sound_cache_until_consent_and_preserves_quiet() {
    for args in [vec![], vec!["--quiet"]] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        drop(slate_cli::config::ConfigWriteGuard::acquire(&env).unwrap());
        fs::write(env.zshrc_path(), "original\n").unwrap();
        slate_cli::config::begin_restore_point_baseline_with_env(&env).unwrap();
        fs::write(env.zshrc_path(), "personal current\n").unwrap();
        let before = tree_snapshot::tree(home.path());
        let mut menu = PickerProcess::start_with_path(home.path(), &args, Some(&bin));
        menu.wait_for_output("想调整什么？");
        let choice = menu_choice_keys(&mut menu, "想调整什么？", "恢复配置");
        navigate(&mut menu, &choice, "选择恢复点：");
        navigate(&mut menu, b"\r", "● 取消");
        assert!(String::from_utf8_lossy(&menu.output).contains("上述文件后来的修改会被覆盖或移除"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut menu, b"\x1b", "选择恢复点：");
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut menu, b"\r", "● 取消");
        navigate(&mut menu, b"y", "● 恢复配置");
        assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"original\n");
        assert_eq!(
            env.slate_cache_dir().join("sounds/hero.wav").is_file(),
            args.is_empty()
        );
        menu.finish(b"\x1b");
    }
}

#[test]
fn empty_restore_page_waits_for_back_and_retains_hub_selection() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut menu = PickerProcess::start_with_path(home.path(), &[], Some(&bin));
    for keys in [b"\r".as_slice(), b"\x1b"] {
        menu.wait_for_output("想调整什么？");
        let choice = menu_choice_keys(&mut menu, "想调整什么？", "恢复配置");
        navigate(&mut menu, &choice, "● 返回主菜单");
        assert!(String::from_utf8_lossy(&menu.output).contains("还没有恢复点"));
        assert!(menu.child.try_wait().unwrap().is_none());
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut menu, keys, "● 恢复配置");
    }
    menu.finish(b"\x1b");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

use super::recovery_tree as tree_snapshot;

fn navigate(child: &mut PickerProcess, keys: &[u8], ready: &str) -> usize {
    child.drain();
    let offset = child.output.len();
    child.terminal.write_all(keys).unwrap();
    child.wait_for_output_since(ready, offset);
    offset
}

fn wait_after_result(child: &mut PickerProcess, offset: usize, result: &str, next: &str) {
    let text = String::from_utf8_lossy(&child.output[offset..]);
    let after = offset + text.find(result).expect("result was observed") + result.len();
    child.wait_for_output_since(next, after);
}

fn tool_choice_keys(child: &mut PickerProcess, label: &str) -> Vec<u8> {
    let marker = "◆  选择工具或查看全部支持";
    child.wait_for_output(marker);
    for _ in 0..slate_cli::cli::tools::supported_tools().len() + 6 {
        child.drain();
        let start = String::from_utf8_lossy(&child.output)
            .rfind(marker)
            .unwrap();
        child.wait_for_output_since("● ", start);
        child.drain();
        let output = String::from_utf8_lossy(&child.output);
        let frame = output.rsplit_once(marker).unwrap().1;
        let selected = frame
            .lines()
            .find_map(|line| line.split_once("● ").map(|(_, row)| row))
            .expect("selected row");
        if selected.trim() == label || selected.starts_with(&format!("{label} (")) {
            return vec![b'\r'];
        }
        let offset = child.output.len();
        child.terminal.write_all(b"\x1b[B").unwrap();
        child.wait_for_output_since(marker, offset);
    }
    panic!("tool menu did not reach {label:?}");
}

fn menu_choice_keys(child: &mut PickerProcess, prompt: &str, label: &str) -> Vec<u8> {
    for _ in 0..128 {
        child.drain();
        // The prompt header can arrive in a separate PTY write from its rows.
        // Wait for a complete frame; the requested label may be offscreen.
        let marker = format!("◆  {prompt}");
        child.wait_for_output(&marker);
        let menu_start = String::from_utf8_lossy(&child.output)
            .rfind(&marker)
            .expect("tool menu was rendered")
            + marker.len();
        child.wait_for_output_since("└", menu_start);
        child.drain();
        let output = String::from_utf8_lossy(&child.output);
        let (_, menu) = output.rsplit_once(&marker).expect("tool menu was rendered");
        // Installed host tools can appear even with an isolated profile. Navigate
        // the observed rows instead of assuming the fixture's btop is first.
        let rows = menu
            .lines()
            .filter_map(|line| {
                let (_, row) = line.split_once("│  ")?;
                row.strip_prefix("● ")
                    .map(|label| (label, true))
                    .or_else(|| row.strip_prefix("○ ").map(|label| (label, false)))
            })
            .collect::<Vec<_>>();
        let index = rows
            .iter()
            .position(|(row, _)| row.trim() == label || row.starts_with(&format!("{label} (")));
        let Some(index) = index else {
            let offset = child.output.len();
            child.terminal.write_all(b"\x1b[B").unwrap();
            child.wait_for_output_since(&marker, offset);
            continue;
        };
        let selected = rows
            .iter()
            .position(|(_, selected)| *selected)
            .expect("selected row is visible");
        let mut keys = if index >= selected {
            b"\x1b[B".repeat(index - selected)
        } else {
            b"\x1b[A".repeat(selected - index)
        };
        keys.push(b'\r');
        return keys;
    }
    panic!("menu {prompt:?} did not reach {label:?}");
}

fn supported_tool_keys(id: &str) -> Vec<u8> {
    let ids = slate_cli::cli::tools::supported_tools();
    let index = ids.iter().position(|candidate| *candidate == id).unwrap();
    let mut keys = b"\x1b[B".repeat(index);
    keys.push(b'\r');
    keys
}

#[test]
fn tools_main_menu_scrolls_to_hidden_exit_without_writes_or_tool_launches() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for id in [
        "bat",
        "btop",
        "delta",
        "eza",
        "fastfetch",
        "lazygit",
        "starship",
        "tmux",
        "yazi",
        "zellij",
    ] {
        let path = bin.join(id);
        fs::write(
            &path,
            "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_TOOL\"\nexit 91\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = tree_snapshot::tree(home.path());
    let mut menu = PickerProcess::start_with_path(home.path(), &["tools"], Some(&bin));
    menu.wait_for_output("选择工具或查看全部支持");
    menu.wait_for_output("● ");
    menu.drain();
    let output = String::from_utf8_lossy(&menu.output);
    let frame = output.rsplit_once("◆  选择工具或查看全部支持").unwrap().1;
    let rows = frame
        .lines()
        .filter(|line| line.contains("● ") || line.contains("○ "))
        .count();
    assert!(
        rows > 0 && rows <= 8,
        "expected bounded visible menu: {frame}"
    );
    assert!(
        !frame.contains("退出工具菜单"),
        "exit should initially be offscreen"
    );
    let keys = tool_choice_keys(&mut menu, "Zellij");
    navigate(&mut menu, &keys, "返回工具菜单");
    let keys = menu_choice_keys(&mut menu, "Zellij · 选择操作", "返回工具菜单");
    let offset = navigate(&mut menu, &keys, "选择工具或查看全部支持");
    menu.wait_for_output_since("刚刚查看", offset);
    menu.drain();
    let output = String::from_utf8_lossy(&menu.output[offset..]);
    let frame = output.rsplit_once("◆  选择工具或查看全部支持").unwrap().1;
    let rows: Vec<_> = frame
        .lines()
        .filter(|line| line.contains("● ") || line.contains("○ "))
        .collect();
    assert!(
        rows.iter().any(|row| row.contains("● Zellij")),
        "last opened tool should be visible and selected: {frame}"
    );
    assert_eq!(rows.iter().filter(|row| row.contains("Zellij")).count(), 1);
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn detected_tool_shortcut_without_theme_opens_details_and_returns_readonly() {
    let home = TempDir::new().unwrap();
    let binary = home.path().join("bin/btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 91\n").unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "btop");
    let offset = navigate(&mut menu, &keys, "返回工具菜单");
    let page = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(page.contains("先选择主题"));
    assert!(!page.contains("同步已保存主题"));
    assert!(!page.contains("要进入全局主题预览吗？"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn discovery_reaches_offscreen_tools_without_a_theme_and_keeps_details_opt_in() {
    let home = TempDir::new().unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    let offset = navigate(&mut menu, &supported_tool_keys("yazi"), "返回工具列表");
    let details = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(!details.contains("完整说明："));
    assert!(details.contains("Slate 已保存主题：未选择"));
    assert!(!details.contains("Open Guided Setup"));
    assert!(!details.contains("同步已保存主题"));
    if details.contains("安装此工具") {
        for cancel in [b"\r".as_slice(), b"\x1b".as_slice()] {
            let keys = menu_choice_keys(&mut menu, "Yazi · 选择操作", "安装此工具");
            let offset = navigate(&mut menu, &keys, "安装此工具及所需依赖？");
            menu.wait_for_output_since("● 取消", offset);
            assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
            let returned = navigate(&mut menu, cancel, "返回工具列表");
            assert!(
                !String::from_utf8_lossy(&menu.output[returned..]).contains("Operation cancelled")
            );
            assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
        }
    }
    let keys = menu_choice_keys(&mut menu, "Yazi · 选择操作", "查看详细信息");
    let expanded = navigate(&mut menu, &keys, "浏览目录与预览文件");
    wait_after_result(&mut menu, expanded, "浏览目录与预览文件", "● 返回工具页面");
    navigate(&mut menu, b"\x1b", "返回工具列表");
    let keys = menu_choice_keys(&mut menu, "Yazi · 选择操作", "返回工具列表");
    let returned = navigate(&mut menu, &keys, "全部支持的工具");
    menu.wait_for_output_since("刚刚查看", returned);
    menu.drain();
    let output = String::from_utf8_lossy(&menu.output[returned..]);
    let frame = output.rsplit_once("◆  全部支持的工具").unwrap().1;
    let rows: Vec<_> = frame
        .lines()
        .filter(|line| line.contains("● ") || line.contains("○ "))
        .collect();
    assert!(
        rows.iter().any(|row| row.contains("● Yazi")),
        "returned tool should be visible: {frame}"
    );
    assert_eq!(rows.iter().filter(|row| row.contains("Yazi")).count(), 1);
    assert!(rows.len() <= 8);
    // Returning selects Yazi in its original position, rather than moving it
    // to the top. Home must still select the original first adapter.
    navigate(&mut menu, b"\x1b[H", "● Ghostty");
    let back = menu_choice_keys(&mut menu, "全部支持的工具", "返回工具菜单");
    navigate(&mut menu, &back, "选择工具或查看全部支持");
    assert_eq!(tool_choice_keys(&mut menu, "浏览全部工具"), b"\r");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn tool_action_failures_return_to_refreshed_details_without_retry_or_writes() {
    for action in ["预览同步改动", "同步已保存主题"] {
        verify_tool_action_failure(action);
    }
}

fn verify_tool_action_failure(action: &str) {
    let (home, env) = queued_picker_fixture();
    let binary = env.user_local_bin().join("btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 91\n").unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&env.user_local_bin()));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "btop");
    navigate(&mut menu, &keys, "返回工具菜单");
    let mut keys = menu_choice_keys(&mut menu, "btop · 选择操作", action);
    if action == "同步已保存主题" {
        navigate(&mut menu, &keys, "确认同步上述工具的配色？");
        keys = b"\x1b[D\r".to_vec();
    }
    // Another terminal invalidates the page's earlier readiness observation.
    fs::write(env.managed_file("current"), "PRIVATE_UNKNOWN_THEME").unwrap();
    let before = tree_snapshot::tree(home.path());
    let offset = navigate(&mut menu, &keys, "工具操作已停止");
    wait_after_result(&mut menu, offset, "工具操作已停止", "返回工具菜单");
    let output = String::from_utf8_lossy(&menu.output[offset..]);
    let (_, output) = output.split_once("工具操作已停止").unwrap();
    assert!(output.contains("不自动重试"));
    assert!(!output.contains("软件包"));
    assert!(!output.contains("卸载软件"));
    assert_eq!(
        output.contains("部分改动可能保留"),
        action == "同步已保存主题"
    );
    assert!(output.contains("先选择主题"));
    assert!(!output.contains("同步已保存主题"));
    assert!(!output.contains("PRIVATE_UNKNOWN_THEME"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn discovery_refreshes_sync_actions_after_prerequisites_are_added_elsewhere() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&env.user_local_bin()));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    let offset = navigate(&mut menu, &supported_tool_keys("btop"), "返回工具列表");
    assert!(!String::from_utf8_lossy(&menu.output[offset..]).contains("同步已保存主题"));
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    fs::create_dir_all(env.user_local_bin()).unwrap();
    let binary = env.user_local_bin().join("btop");
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "刷新检测结果");
    let offset = navigate(&mut menu, &keys, "Slate 已保存主题：Nord");
    wait_after_result(&mut menu, offset, "Slate 已保存主题：Nord", "返回工具列表");
    assert!(String::from_utf8_lossy(&menu.output[offset..]).contains("同步已保存主题"));
    menu.finish_with_code(b"\x03", 130);
    assert!(!env.slate_cache_dir().exists());
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    assert!(!slate_cli::adapter::BtopAdapter::config_path(&env).exists());
}

#[test]
fn tool_details_return_refreshes_external_prerequisites_without_launching_or_syncing() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    fs::create_dir_all(env.user_local_bin()).unwrap();
    let binary = env.user_local_bin().join("btop");
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&env.user_local_bin()));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    navigate(&mut menu, &supported_tool_keys("btop"), "返回工具列表");
    for available in [true, false] {
        let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "查看详细信息");
        navigate(&mut menu, &keys, "● 返回工具页面");
        if available {
            fs::write(
                &binary,
                "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
            )
            .unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        } else {
            fs::rename(&binary, env.user_local_bin().join("btop.disabled")).unwrap();
            fs::rename(
                env.managed_file("current"),
                env.managed_file("current.saved"),
            )
            .unwrap();
        }
        let before = tree_snapshot::tree(home.path());
        let returned = navigate(&mut menu, b"\x1b", "● 查看详细信息");
        let status = if available {
            "命令可找到"
        } else {
            "Slate 已保存主题：未选择"
        };
        let returned_page = String::from_utf8_lossy(&menu.output[returned..]);
        assert!(
            returned_page.contains(status),
            "expected {status}: {returned_page}"
        );
        if !available {
            // Another installation outside PATH can still be detected.
            assert!(!returned_page.contains("命令可找到"));
        }
        // Details stays selected; a second Enter must not install or sync.
        assert_eq!(
            menu_choice_keys(&mut menu, "btop · 选择操作", "查看详细信息"),
            b"\r"
        );
        let frame = navigate(&mut menu, b"\x1b[H", "btop · 选择操作");
        menu.wait_for_output_since("└", frame);
        let output = String::from_utf8_lossy(&menu.output[frame..]);
        assert_eq!(output.contains("同步已保存主题"), available, "{output}");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "查看详细信息");
    navigate(&mut menu, &keys, "● 返回工具页面");
    menu.finish_with_code(b"\x03", 130);
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    assert!(!env.slate_cache_dir().exists());
    assert!(!slate_cli::adapter::BtopAdapter::config_path(&env).exists());
}

#[test]
fn tools_theme_checks_work_without_a_saved_theme_and_cancel_without_writes() {
    let home = TempDir::new().unwrap();
    let mut menu = PickerProcess::start_with_args(home.path(), &["tools"]);
    menu.wait_for_output("选择工具或查看全部支持");
    // Checks remain reachable alongside discovery when no theme is saved.
    let keys = tool_choice_keys(&mut menu, "检查配色配置");
    menu.terminal.write_all(&keys).unwrap();
    menu.wait_for_output("选择要检查的配色配置");
    menu.finish_with_code(b"\x03", 130);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    let mut menu = PickerProcess::start_with_args(home.path(), &["tools"]);
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "检查配色配置");
    menu.terminal.write_all(&keys).unwrap();
    menu.wait_for_output("选择要检查的配色配置");
    let keys = menu_choice_keys(&mut menu, "选择要检查的配色配置", "Starship Prompt");
    let offset = navigate(
        &mut menu,
        &keys,
        "选中的配置文件不存在；无法确认 Slate 配色或布局。",
    );
    wait_after_result(
        &mut menu,
        offset,
        "选中的配置文件不存在；无法确认 Slate 配色或布局。",
        "● 返回检查列表",
    );
    navigate(&mut menu, b"\x1b", "选择要检查的配色配置");
    let keys = menu_choice_keys(&mut menu, "选择要检查的配色配置", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert!(String::from_utf8_lossy(&menu.output)
        .contains("选中的配置文件不存在；无法确认 Slate 配色或布局。"));
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn prompt_catalog_starts_at_each_saved_style_without_writes() {
    for (style, label) in slate_cli::config::prompt::PromptStyle::ALL
        .into_iter()
        .zip([
            "彩虹分段",
            "简洁双行",
            "紧凑单行",
            "经典双行",
            "专注单行",
            "分支单行",
        ])
    {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("current"), "nord\n").unwrap();
        fs::write(
            env.managed_file("config.toml"),
            format!("[prompt]\nstyle = '{}'\n", style.id()),
        )
        .unwrap();
        let before = tree_snapshot::tree(home.path());
        let mut menu = PickerProcess::start_with_args(home.path(), &["prompt"]);
        menu.wait_for_output("返回上级");
        let keys = menu_choice_keys(&mut menu, "选择提示符样式", &format!("{label} · 已保存"));
        assert_eq!(keys, b"\r", "saved style must be initially selected");
        menu.finish(b"\x1b");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn prompt_first_run_can_compare_layouts_and_decline_theme_preview_without_writes() {
    let home = TempDir::new().unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
    hub.wait_for_output("退出");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "命令提示符");
    navigate(&mut hub, &keys, "返回上级");
    for (label, hint) in [
        ("简洁双行", "下一行输入；含命令耗时"),
        ("紧凑单行", "节省纵向空间"),
        ("彩虹分段", "推荐图标字体"),
        ("经典双行", "默认仅 SSH 显示主机"),
        ("专注单行", "不显示 Git 或时钟"),
        ("分支单行", "不显示 Git 改动统计或耗时"),
    ] {
        let keys = menu_choice_keys(&mut hub, "选择提示符样式", label);
        let offset = navigate(&mut hub, &keys, "样式预览");
        hub.wait_for_output_since("返回样式列表", offset);
        let page = String::from_utf8_lossy(&hub.output[offset..]);
        assert!(page.contains(hint), "{page}");
        assert!(page.contains("仅样式示意，并非实时提示符"), "{page}");
        assert!(page.contains("先选择主题"), "{page}");
        assert!(!page.contains("查看改动并确认"), "{page}");
        assert_eq!(tree_snapshot::tree(home.path()), before);
        for cancel in [b"\r".as_slice(), b"\x1b".as_slice()] {
            let keys = menu_choice_keys(&mut hub, "样式预览", "先选择主题");
            let offset = navigate(&mut hub, &keys, "要进入全局主题预览吗？");
            hub.wait_for_output_since("● 暂不进入", offset);
            let consent = String::from_utf8_lossy(&hub.output[offset..]);
            assert!(consent.contains("预览会临时修改检测到的工具配置"));
            assert!(consent.contains("本次不保存刚才选择的提示符样式"));
            navigate(&mut hub, cancel, "样式预览");
            assert_eq!(tree_snapshot::tree(home.path()), before);
        }
        let keys = menu_choice_keys(&mut hub, "样式预览", "返回样式列表");
        navigate(&mut hub, &keys, "选择提示符样式");
        assert_eq!(menu_choice_keys(&mut hub, "选择提示符样式", label), b"\r");
    }
    let keys = menu_choice_keys(&mut hub, "选择提示符样式", "返回上级");
    navigate(&mut hub, &keys, "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn prompt_browser_refreshes_theme_and_keeps_the_page_after_declining_application() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let binary = home.path().join("bin/starship");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["prompt"], Some(&home.path().join("bin")));
    menu.wait_for_output("返回上级");
    navigate(&mut menu, b"\x1b[B\r", "样式预览");
    // Simulate a theme selected in another terminal while the example is open.
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let keys = menu_choice_keys(&mut menu, "样式预览", "刷新已保存状态");
    navigate(&mut menu, &keys, "查看改动并确认");
    menu.wait_for_output("● 刷新已保存状态");
    let keys = menu_choice_keys(&mut menu, "样式预览", "查看改动并确认");
    let offset = navigate(&mut menu, &keys, "保存这个提示符样式？");
    assert!(String::from_utf8_lossy(&menu.output[offset..]).contains("简洁双行 · nord"));
    navigate(&mut menu, b"\r", "样式预览"); // default No stays here
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "样式预览", "返回上级");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn prompt_examples_remain_read_only_with_unsafe_theme_or_broken_personal_configs() {
    for theme in ["known", "unknown", "symlink", "fifo"] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        let current = env.managed_file("current");
        match theme {
            "known" => fs::write(&current, "nord\n").unwrap(),
            "unknown" => fs::write(&current, "PRIVATE_UNKNOWN_THEME").unwrap(),
            "symlink" => std::os::unix::fs::symlink(home.path().join("absent"), &current).unwrap(),
            "fifo" => {
                let path = std::ffi::CString::new(current.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        for path in [
            env.managed_file("config.toml"),
            env.xdg_config_home().join("starship.toml"),
        ] {
            let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        }
        fs::create_dir_all(env.slate_cache_dir()).unwrap();
        fs::write(
            env.slate_cache_dir().join("preview-session.json"),
            "PRIVATE_PENDING_RECOVERY",
        )
        .unwrap();
        let before = tree_snapshot::tree(home.path());
        let mut menu = PickerProcess::start_with_path(
            home.path(),
            &["prompt"],
            Some(&home.path().join("bin")),
        );
        menu.wait_for_output("返回上级");
        let offset = navigate(&mut menu, b"\x1b[B\r", "样式预览");
        menu.wait_for_output_since("返回上级", offset);
        let page = String::from_utf8_lossy(&menu.output[offset..]);
        assert!(page.contains("仅样式示意，并非实时提示符"));
        assert!(!page.contains("PRIVATE_"));
        assert_eq!(
            page.contains("查看改动并确认"),
            theme == "known",
            "{theme}: {page}"
        );
        if matches!(theme, "fifo" | "symlink") {
            assert!(page.contains("已保存主题无法安全读取"));
            assert!(!page.contains("先选择主题"));
        }
        let keys = menu_choice_keys(&mut menu, "样式预览", "返回上级");
        menu.finish(&keys);
        assert_eq!(tree_snapshot::tree(home.path()), before, "{theme}");
    }
}

#[test]
fn prompt_theme_preview_cancel_restores_files_and_returns_to_the_same_style() {
    let (home, env) = queued_picker_fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    let preferences = fs::read(env.managed_file("config.toml")).unwrap();
    let binary = home.path().join("bin/starship");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["prompt"], Some(&home.path().join("bin")));
    menu.wait_for_output("返回上级");
    navigate(&mut menu, b"\x1b[B\x1b[B\r", "样式预览");
    let keys = menu_choice_keys(&mut menu, "样式预览", "先选择主题");
    navigate(&mut menu, &keys, "要进入全局主题预览吗？");
    navigate(&mut menu, b"\x1b[D\r", "s 保存配对");
    let offset = navigate(&mut menu, b"q", "样式预览");
    let page = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(page.contains("紧凑单行"));
    assert!(page.contains("先选择主题"));
    assert!(!page.contains("查看改动并确认"));
    assert!(!env.managed_file("current").exists());
    assert!(!env.xdg_config_home().join("starship.toml").exists());
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert_eq!(
        fs::read(env.managed_file("config.toml")).unwrap(),
        preferences
    );
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
    let keys = menu_choice_keys(&mut menu, "样式预览", "返回上级");
    menu.finish(&keys);
}

#[test]
fn tool_theme_entry_declines_readonly_and_returns_after_canceling_preview() {
    let (home, env) = queued_picker_fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    let binary = home.path().join("bin/btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 91\n").unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    navigate(&mut menu, &supported_tool_keys("btop"), "返回工具列表");
    navigate(&mut menu, b"\r", "要进入全局主题预览吗？");
    navigate(&mut menu, b"\r", "返回工具列表"); // default No
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "先选择主题");
    let offset = navigate(&mut menu, &keys, "要进入全局主题预览吗？");
    menu.wait_for_output_since("● 暂不进入", offset);
    let consent = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(consent.contains("预览会临时修改检测到的工具配置"));
    assert!(consent.contains("本次不安装工具，也不运行完整设置"));
    navigate(&mut menu, b"\x1b", "返回工具列表");
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "先选择主题");
    navigate(&mut menu, &keys, "要进入全局主题预览吗？");
    navigate(&mut menu, b"\x1b[D\r", "s 保存配对");
    let offset = navigate(&mut menu, b"q", "返回工具列表");
    let page = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(page.contains("先选择主题"));
    assert!(!page.contains("同步已保存主题"));
    assert!(!env.managed_file("current").exists());
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
    menu.finish_with_code(b"\x03", 130);
}

#[test]
fn tool_theme_entry_rechecks_unreadable_state_after_confirmation() {
    let (home, env) = queued_picker_fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    navigate(&mut menu, &supported_tool_keys("btop"), "返回工具列表");
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "先选择主题");
    navigate(&mut menu, &keys, "要进入全局主题预览吗？");
    fs::create_dir(env.managed_file("current")).unwrap();
    let before = tree_snapshot::tree(home.path());
    let offset = navigate(&mut menu, b"\x1b[D\r", "no preview was opened");
    wait_after_result(&mut menu, offset, "no preview was opened", "返回工具列表");
    menu.drain();
    let page = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(page.contains("不自动重试"));
    let refreshed = page.rsplit_once("btop · 选择操作").unwrap().1;
    assert!(!refreshed.contains("先选择主题"));
    assert!(!refreshed.contains("同步已保存主题"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具列表");
    navigate(&mut menu, &keys, "全部支持的工具");
    let keys = menu_choice_keys(&mut menu, "全部支持的工具", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn tool_theme_entry_saves_then_refreshes_actions_and_keeps_theme_when_leaving() {
    let (home, env) = queued_picker_fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    let binary = home.path().join("bin/btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "浏览全部工具");
    navigate(&mut menu, &keys, "全部支持的工具");
    navigate(&mut menu, &supported_tool_keys("btop"), "返回工具列表");
    navigate(&mut menu, b"\r", "要进入全局主题预览吗？");
    navigate(&mut menu, b"\x1b[D\r", "s 保存配对");
    let offset = navigate(&mut menu, b"\r", "返回工具列表");
    let page = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(page.contains("同步已保存主题"));
    assert!(page.contains("● 检查配色配置"));
    assert!(
        !page.contains("info: "),
        "routine notices cluttered the menu: {page}"
    );
    assert!(!page.contains("Saved; reopen btop."));
    assert!(!page.contains("warning: btop:"));
    assert!(!page.contains("Theme files saved; reopen btop to use them."));
    assert!(!page.contains("先选择主题"));
    assert!(!page.contains("确认同步上述工具的配色？"));
    let saved = fs::read_to_string(env.managed_file("current")).unwrap();
    assert!(slate_cli::theme::ThemeRegistry::new()
        .unwrap()
        .get(saved.trim())
        .is_some());
    let registry = slate_cli::theme::ThemeRegistry::new().unwrap();
    let saved_name = &registry.get(saved.trim()).unwrap().name;
    assert!(page.contains(&format!("Slate 已保存主题：{saved_name}")));
    assert!(slate_cli::adapter::BtopAdapter::theme_path(&env).exists());
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    let before_leave = tree_snapshot::tree(home.path());
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具列表");
    navigate(&mut menu, &keys, "全部支持的工具");
    let back = menu_choice_keys(&mut menu, "全部支持的工具", "返回工具菜单");
    navigate(&mut menu, &back, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        saved
    );
    assert_eq!(tree_snapshot::tree(home.path()), before_leave);
}

#[test]
fn prompt_theme_entry_rechecks_unreadable_state_after_confirmation() {
    let (home, env) = queued_picker_fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["prompt"], Some(&home.path().join("bin")));
    menu.wait_for_output("返回上级");
    navigate(&mut menu, b"\r", "样式预览");
    let keys = menu_choice_keys(&mut menu, "样式预览", "先选择主题");
    navigate(&mut menu, &keys, "要进入全局主题预览吗？");
    fs::write(env.managed_file("current"), [0xff, 0xfe]).unwrap();
    let before = tree_snapshot::tree(home.path());
    let offset = navigate(&mut menu, b"\x1b[D\r", "no preview was opened");
    wait_after_result(&mut menu, offset, "no preview was opened", "◆  样式预览");
    menu.wait_for_output_since("○ 返回上级", offset);
    let keys = menu_choice_keys(&mut menu, "样式预览", "返回上级");
    menu.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn hub_prompt_entry_and_declined_style_leave_files_unchanged() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    hub.terminal.write_all(b"\x1b[B\x1b[B\x1b[B\r").unwrap();
    hub.wait_for_output("返回上级");
    hub.finish_with_code(b"\x03", 130);
    assert!(!env.slate_cache_dir().exists());
    let mut prompt = PickerProcess::start_with_args(home.path(), &["prompt", "minimal"]);
    prompt.wait_for_output("保存这个提示符样式？");
    prompt.finish(b"\r");
    assert!(!env.slate_cache_dir().exists());
    assert!(!env.xdg_config_home().join("starship.toml").exists());
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
}

#[test]
fn workspace_theme_checks_are_reachable_from_check_menu_and_empty_tool_details() {
    for (id, label) in [
        ("btop", "btop"),
        ("starship", "Starship Prompt"),
        ("ghostty", "Ghostty"),
        ("yazi", "Yazi"),
        ("zellij", "Zellij"),
        ("lazygit", "Lazygit"),
        ("eza", "eza"),
        ("fastfetch", "Fastfetch"),
    ] {
        let home = TempDir::new().unwrap();
        let mut menu =
            PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
        menu.wait_for_output("选择工具或查看全部支持");
        let keys = tool_choice_keys(&mut menu, "检查配色配置");
        navigate(&mut menu, &keys, "选择要检查的配色配置");
        let keys = menu_choice_keys(&mut menu, "选择要检查的配色配置", label);
        let scope = if id == "ghostty" {
            "完整文件检查：slate doctor ghostty --files-only".to_string()
        } else {
            format!("完整结果与补充说明：slate doctor {id}")
        };
        let offset = navigate(&mut menu, &keys, &scope);
        wait_after_result(&mut menu, offset, &scope, "● 返回检查列表");
        navigate(&mut menu, b"\r", "选择要检查的配色配置");
        // The title may arrive before the rows in a separate PTY write.
        // Do not assert the selected row until the refreshed frame is complete.
        let text = String::from_utf8_lossy(&menu.output[offset..]);
        let frame_start = offset + text.rfind("◆  选择要检查的配色配置").unwrap();
        menu.wait_for_output_since("└", frame_start);
        menu.drain();
        let output = String::from_utf8_lossy(&menu.output[offset..]);
        let frame = output.rsplit_once("◆  选择要检查的配色配置").unwrap().1;
        assert!(frame.contains(&format!("● {label} ")), "{frame}");
        // Enter repeats the selected read-only check, rather than running btop.
        let repeated = navigate(&mut menu, b"\r", &scope);
        wait_after_result(&mut menu, repeated, &scope, "● 返回检查列表");
        navigate(&mut menu, b"\x1b", "选择要检查的配色配置");
        let keys = menu_choice_keys(&mut menu, "选择要检查的配色配置", "返回工具菜单");
        navigate(&mut menu, &keys, "选择工具或查看全部支持");
        let keys = tool_choice_keys(&mut menu, "浏览全部工具");
        navigate(&mut menu, &keys, "全部支持的工具");
        let offset = navigate(&mut menu, &supported_tool_keys(id), "返回工具列表");
        assert!(!String::from_utf8_lossy(&menu.output[offset..]).contains("同步已保存主题"));
        let tool_label = slate_cli::cli::tool_selection::ToolCatalog::get_tool(id)
            .unwrap()
            .label;
        let prompt = format!("{tool_label} · 选择操作");
        let keys = menu_choice_keys(&mut menu, &prompt, "检查配色配置");
        let offset = navigate(&mut menu, &keys, &scope);
        wait_after_result(&mut menu, offset, &scope, "● 返回工具页面");
        navigate(&mut menu, b"\x1b", &prompt);
        menu.finish_with_code(b"\x03", 130);
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    }
}

#[test]
fn hub_tools_entry_and_declined_sync_are_read_only() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let binary = env.user_local_bin().join("btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 91\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    hub.terminal.write_all(b"\x1b[B\r").unwrap();
    hub.wait_for_output("选择工具或查看全部支持");
    let _ = tool_choice_keys(&mut hub, "返回首页");
    assert!(String::from_utf8_lossy(&hub.output).contains("引导设置"));
    hub.finish_with_code(b"\x03", 130);
    assert!(!env.slate_cache_dir().exists());
    // Direct sync defaults to No: opening/reviewing/declining creates no lock.
    let mut sync = PickerProcess::start_with_args(home.path(), &["tools", "sync", "btop"]);
    sync.wait_for_output("确认同步上述工具的配色？");
    sync.finish(b"\r");
    assert!(!env.slate_cache_dir().exists());
    assert!(!slate_cli::adapter::BtopAdapter::config_path(&env).exists());
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
}

#[test]
fn font_browser_leaves_writer_lock_available_and_can_exit_while_another_writer_holds_it() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["font"], Some(&home.path().join("bin")));
    menu.wait_for_output("保留当前字体");
    assert!(!env.slate_cache_dir().exists());
    let _other_writer = slate_cli::config::ConfigWriteGuard::acquire(&env)
        .expect("font browser must not hold the writer lock");
    let before = tree_snapshot::tree(home.path());
    menu.finish(b"\x1b");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn font_catalog_download_requires_confirmation_and_declines_return_to_selection() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let config_before = tree_snapshot::tree(env.config_dir());
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["font"], Some(&home.path().join("bin")));
    menu.wait_for_output("保留当前字体");
    assert!(
        !env.slate_cache_dir().exists(),
        "browsing must not create a writer lock"
    );
    let before = tree_snapshot::tree(home.path());
    // Font discovery may find host-installed families even in a private profile.
    let page = String::from_utf8_lossy(&menu.output);
    let label = [
        "JetBrains Mono Nerd Font (下载)",
        "Fira Code Nerd Font (下载)",
        "Iosevka Term Nerd Font (下载)",
        "Hack Nerd Font (下载)",
    ]
    .into_iter()
    .find(|label| page.contains(label))
    .expect("an uninstalled catalog font for the consent fixture");
    for cancel in [b"\r".as_slice(), b"\x1b".as_slice()] {
        let keys = menu_choice_keys(&mut menu, "选择字体：", label);
        let offset = navigate(&mut menu, &keys, "暂不下载");
        menu.wait_for_output_since("确认下载并使用", offset);
        assert!(String::from_utf8_lossy(&menu.output[offset..]).contains("● 暂不下载"));
        let offset = navigate(&mut menu, b"\x1b[B\r", "字体配置预览");
        menu.wait_for_output_since("仅预览：未安装、未写文件", offset);
        menu.wait_for_output_since("● 暂不下载", offset);
        menu.wait_for_output_since("确认下载并使用", offset);
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut menu, cancel, "选择字体：");
        assert_eq!(menu_choice_keys(&mut menu, "选择字体：", label), b"\r");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    menu.finish(b"\x1b");
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert_eq!(tree_snapshot::tree(env.config_dir()), config_before);
}

#[test]
fn hub_font_status_and_empty_restore_return_without_changing_preferences() {
    for (action, ready) in [
        ("更换字体", "保留当前字体"),
        ("检查配置", "已保存的配置"),
        ("恢复配置", "还没有恢复点"),
    ] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("current"), "nord\n").unwrap();
        let before = tree_snapshot::tree(env.config_dir());
        let mut hub =
            PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
        hub.wait_for_output("退出");
        let keys = menu_choice_keys(&mut hub, "想调整什么？", action);
        navigate(&mut hub, &keys, ready);
        if action == "更换字体" {
            let keys = menu_choice_keys(&mut hub, "选择字体：", "保留当前字体");
            navigate(&mut hub, &keys, "想调整什么？");
        } else {
            hub.wait_for_output("● 返回主菜单");
            navigate(&mut hub, b"\x1b", "想调整什么？");
        }
        let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
        hub.finish(&keys);
        assert_eq!(tree_snapshot::tree(env.config_dir()), before, "{action}");
    }
}

#[test]
fn every_supported_tool_can_open_details_and_return_without_profile_writes() {
    for id in slate_cli::cli::tools::supported_tools() {
        let home = TempDir::new().unwrap();
        let mut menu =
            PickerProcess::start_with_path(home.path(), &["tools"], Some(&home.path().join("bin")));
        menu.wait_for_output("选择工具或查看全部支持");
        let keys = tool_choice_keys(&mut menu, "浏览全部工具");
        navigate(&mut menu, &keys, "全部支持的工具");
        navigate(&mut menu, &supported_tool_keys(id), "返回工具列表");
        let label =
            slate_cli::cli::tool_selection::ToolCatalog::get_tool(id).map_or(id, |tool| tool.label);
        let prompt = format!("{label} · 选择操作");
        let keys = menu_choice_keys(&mut menu, &prompt, "查看详细信息");
        let offset = navigate(&mut menu, &keys, "完整说明：");
        wait_after_result(&mut menu, offset, "完整说明：", "● 返回工具页面");
        let report = String::from_utf8_lossy(&menu.output[offset..]);
        let report = report.split_once("完整说明：").unwrap().1;
        assert!(!report.contains(&prompt), "{report}");
        let returned = navigate(&mut menu, b"\r", "返回工具列表");
        menu.wait_for_output_since("● 查看详细信息", returned);
        let keys = menu_choice_keys(&mut menu, &prompt, "返回工具列表");
        navigate(&mut menu, &keys, "全部支持的工具");
        menu.finish_with_code(b"\x03", 130);
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0, "{id}");
    }
}

#[test]
fn tool_sync_confirmation_rejects_an_external_config_edit() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let binary = env.user_local_bin().join("btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_TOOL\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let config = slate_cli::adapter::BtopAdapter::config_path(&env);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "color_theme=personal\n").unwrap();
    let mut menu = PickerProcess::start_with_path(
        home.path(),
        &["tools", "sync", "btop"],
        Some(&env.user_local_bin()),
    );
    menu.wait_for_output("确认同步上述工具的配色？");
    menu.wait_for_output("● 暂不同步");
    fs::write(&config, "# PRIVATE_EXTERNAL_EDIT\ncolor_theme=personal\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    menu.finish_with_code(b"\x1b[B\r", 1);
    let output = String::from_utf8_lossy(&menu.output);
    assert!(output.contains("configuration files changed after review"));
    assert!(!output.contains("PRIVATE_EXTERNAL_EDIT"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn hub_about_shows_compiled_capabilities_and_returns_without_profile_writes() {
    let home = TempDir::new().unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
    hub.wait_for_output("退出");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "关于 Slate");
    let offset = navigate(&mut hub, &keys, "返回首页");
    hub.wait_for_output_since("诊断详情", offset);
    let page = String::from_utf8_lossy(&hub.output[offset..]);
    for text in ["主题    ", "工具    ", "提示符  ", "内置支持数量"] {
        assert!(page.contains(text), "missing {text}: {page}");
    }
    for text in [
        "Source tag:",
        "Executable:",
        "Cargo profile",
        "Try this exact build",
    ] {
        assert!(
            !page.contains(text),
            "diagnostics leaked into overview: {page}"
        );
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut hub, "关于 Slate", "诊断详情");
    let offset = navigate(&mut hub, &keys, "返回关于");
    let page = String::from_utf8_lossy(&hub.output[offset..]);
    for text in [
        "源码标识  fnv1a64-v1-",
        "程序路径",
        "当前运行的 Slate",
        "不代表已同步远端",
        "完整报告：slate about",
    ] {
        assert!(page.contains(text), "missing {text}: {page}");
    }
    assert!(!page.contains("Cargo profile") && !page.contains("Prompt styles:"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    navigate(&mut hub, b"\r", "返回首页");
    navigate(&mut hub, b"\r", "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let binary = std::env::var_os("SLATE_PICKER_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_slate").into());
    let output = Command::new(binary)
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .args(["about"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8_lossy(&output.stdout);
    for text in [
        "Source tag: fnv1a64-v1-",
        "Executable:",
        "Built in:",
        "rainbow, minimal, compact, classic, focus",
        "not installed tools or live theme status",
    ] {
        assert!(report.contains(text), "missing {text}: {report}");
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn hub_escape_returns_through_browsing_pages_without_cancel_errors_or_writes() {
    let (home, env) = queued_picker_fixture();
    let before = tree_snapshot::tree(home.path());
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
    hub.wait_for_output("退出");
    for (entry, page) in [
        ("终端偏好", "终端偏好"),
        ("自动换色：关", "自动换色"),
        ("关于 Slate", "关于 Slate"),
    ] {
        let keys = menu_choice_keys(&mut hub, "想调整什么？", entry);
        navigate(&mut hub, &keys, "返回首页");
        if page == "关于 Slate" {
            let keys = menu_choice_keys(&mut hub, page, "诊断详情");
            navigate(&mut hub, &keys, "返回关于");
            navigate(&mut hub, b"\x1b", "返回首页");
        }
        navigate(&mut hub, b"\x1b", "想调整什么？");
        assert_eq!(menu_choice_keys(&mut hub, "想调整什么？", entry), b"\r");
    }
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "命令提示符");
    navigate(&mut hub, &keys, "返回上级");
    for style in [
        "彩虹分段",
        "简洁双行",
        "紧凑单行",
        "经典双行",
        "专注单行",
        "分支单行",
    ] {
        let keys = menu_choice_keys(&mut hub, "选择提示符样式", style);
        let offset = navigate(&mut hub, &keys, "◆  样式预览");
        let preview = String::from_utf8_lossy(&hub.output[offset..]);
        assert!(preview.contains(style), "{preview}");
        assert!(!preview.contains("Esc 返回样式列表"), "{preview}");
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut hub, b"\x1b", "选择提示符样式");
        assert_eq!(menu_choice_keys(&mut hub, "选择提示符样式", style), b"\r");
    }
    navigate(&mut hub, b"\x1b", "想调整什么？");
    assert_eq!(
        menu_choice_keys(&mut hub, "想调整什么？", "命令提示符"),
        b"\r"
    );
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "工具配色");
    navigate(&mut hub, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut hub, "浏览全部工具");
    navigate(&mut hub, &keys, "全部支持的工具");
    navigate(&mut hub, &supported_tool_keys("btop"), "返回工具列表");
    navigate(&mut hub, b"\x1b", "全部支持的工具");
    navigate(&mut hub, b"\x1b", "选择工具或查看全部支持");
    navigate(&mut hub, b"\x1b", "想调整什么？");
    hub.finish(b"\x1b");
    let output = String::from_utf8_lossy(&hub.output);
    assert!(!output.contains("Operation cancelled"), "{output}");
    assert!(!output.contains("User cancelled"), "{output}");
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "catppuccin-mocha"
    );
}

#[test]
fn hub_entry_first_run_preview_is_optional_and_cancel_creates_nothing() {
    let home = TempDir::new().unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    let output = String::from_utf8_lossy(&hub.output);
    for label in ["预览主题", "工具配色", "终端偏好", "检查配置", "关于 Slate"] {
        assert!(output.contains(label), "{output}");
    }
    assert!(output.contains("还没有可用的已保存主题"));
    assert!(!output.contains("↑↓ 选择 · Enter 打开 · Esc 退出"));
    assert!(!output.contains("也可以按 Esc"));
    assert!(output.contains("先预览配色，再确认保存"));
    assert!(!output.contains("Catppuccin Mocha"));
    hub.finish_with_code(b"\x03", 130);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn hub_entry_configured_first_action_opens_picker_and_cancel_restores() {
    let (home, env) = queued_picker_fixture();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    assert!(String::from_utf8_lossy(&hub.output).contains("切换主题"));
    hub.terminal.write_all(b"\r").unwrap();
    hub.wait_for_output("s 保存配对");
    assert!(String::from_utf8_lossy(&hub.output).contains("深暖摩卡"));
    assert!(!String::from_utf8_lossy(&hub.output).contains("/ search"));
    navigate(&mut hub, b"q", "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
    assert_eq!(
        fs::read(env.managed_file("current")).unwrap(),
        b"catppuccin-mocha"
    );
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert!(!env.managed_file("managed/ghostty/theme.conf").exists());
}

#[test]
fn hub_entry_unknown_theme_and_broken_preferences_keep_inspection_available() {
    let (home, env) = queued_picker_fixture();
    fs::write(env.managed_file("current"), b"retired-theme").unwrap();
    fs::write(env.managed_file("config.toml"), b"[PRIVATE_BROKEN\n").unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    let output = String::from_utf8_lossy(&hub.output);
    assert!(!output.contains("还没有可用的已保存主题"));
    assert!(output.contains("检查配置"));
    assert!(output.contains("自动换色：需检查"));
    assert!(output.contains("已保存主题无法识别"));
    assert!(output.contains("提示符样式无法读取或识别"));
    assert!(output.contains("自动换色设置无法读取"));
    assert!(!output.contains("Saved theme is unknown"));
    assert!(!output.contains("Catppuccin Mocha"));
    assert!(!output.contains("PRIVATE_BROKEN"));
    // Select 检查配置 (seventh row) even though settings cannot be parsed.
    let offset = navigate(
        &mut hub,
        b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r",
        "已保存的配置",
    );
    hub.wait_for_output_since("● 返回主菜单", offset);
    let summary = String::from_utf8_lossy(&hub.output[offset..]);
    assert!(summary.contains("已保存主题无法识别"));
    assert!(summary.contains("请检查 config.toml"));
    assert!(!summary.contains("Saved theme is unknown"));
    assert!(!summary.contains("availability only"));
    assert!(!summary.contains("PRIVATE_BROKEN"));
    navigate(&mut hub, b"\x1b[B\x1b[B\r", "● 返回检查摘要");
    navigate(&mut hub, b"\x1b", "● 查看详细诊断");
    let refreshed = navigate(&mut hub, b"\x1b[A\r", "已保存的配置");
    wait_after_result(&mut hub, refreshed, "已保存的配置", "● 刷新检查");
    navigate(&mut hub, b"\x1b", "想调整什么？");
    hub.finish_with_code(b"\x03", 130);
    assert!(String::from_utf8_lossy(&hub.output).contains("availability only"));
    assert_eq!(
        fs::read(env.managed_file("current")).unwrap(),
        b"retired-theme"
    );
    assert_eq!(
        fs::read(env.managed_file("config.toml")).unwrap(),
        b"[PRIVATE_BROKEN\n"
    );
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn hub_entry_unreadable_records_are_not_presented_as_unset() {
    for field in ["current", "current-font", "current-opacity"] {
        let (home, env) = queued_picker_fixture();
        let path = env.managed_file(field);
        if path.is_file() {
            fs::remove_file(&path).unwrap();
        }
        fs::create_dir(&path).unwrap();
        let before = tree_snapshot::tree(home.path());
        let mut hub = PickerProcess::start_with_args(home.path(), &[]);
        hub.wait_for_output("想调整什么？");
        let output = String::from_utf8_lossy(&hub.output);
        match field {
            "current" => {
                assert!(output.contains("主题记录无法读取"));
                assert!(!output.contains("还没有可用的已保存主题"));
            }
            "current-font" => {
                assert!(output.contains("字体    需检查"));
                assert!(!output.contains("字体    未设置"));
            }
            _ => {
                assert!(output.contains("透明度  需检查"));
                assert!(!output.contains("透明度  未设置"));
            }
        }
        hub.finish_with_code(b"\x03", 130);
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn hub_preferences_and_auto_pages_can_go_back_without_creating_a_profile() {
    let home = TempDir::new().unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    navigate(
        &mut hub,
        b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r",
        "命令提示符：开",
    );
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    navigate(&mut hub, b"\x1b", "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "自动换色：关");
    navigate(&mut hub, &keys, "选择深浅主题");
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    let keys = menu_choice_keys(&mut hub, "自动换色", "检查自动换色");
    let offset = navigate(&mut hub, &keys, "自动换色诊断");
    // The outgoing menu also contains this selection. Wait past the doctor
    // report so Escape cannot arrive during cooked-mode report output.
    wait_after_result(&mut hub, offset, "自动换色诊断", "● 返回自动换色");
    navigate(&mut hub, b"\x1b", "● 检查自动换色");
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    navigate(&mut hub, b"\x1b", "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "工具配色");
    navigate(&mut hub, &keys, "选择工具或查看全部支持");
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    let keys = tool_choice_keys(&mut hub, "返回首页");
    navigate(&mut hub, &keys, "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn tool_sync_menu_success_is_compact_and_keeps_recovery_guidance() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let binary = env.user_local_bin().join("btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&env.user_local_bin()));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "btop");
    navigate(&mut menu, &keys, "预览同步改动");
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "同步已保存主题");
    navigate(&mut menu, &keys, "● 暂不同步");
    let offset = navigate(&mut menu, b"\x1b[B\r", "已保存 1 个工具的配色");
    let receipt = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(receipt.contains("btop · 配置已保存"));
    assert!(receipt.contains("恢复预览：slate restore"));
    assert!(receipt.contains("退出时写回旧配色"));
    assert!(!receipt.contains("Next steps for applied tools"));
    assert!(!receipt.contains("✓ btop"));
    assert!(!receipt.contains("Restore point:"));
    assert!(slate_cli::adapter::BtopAdapter::config_path(&env).is_file());
    wait_after_result(
        &mut menu,
        offset,
        "已保存 1 个工具的配色",
        "btop · 选择操作",
    );
    // Finishing a write must not leave another write preselected. Pressing
    // Enter now checks the saved files, without another snapshot or tool run.
    assert_eq!(
        menu_choice_keys(&mut menu, "btop · 选择操作", "检查配色配置"),
        b"\r"
    );
    let saved_tree = tree_snapshot::tree(home.path());
    let offset = navigate(&mut menu, b"\r", "完整结果与补充说明：slate doctor btop");
    wait_after_result(
        &mut menu,
        offset,
        "完整结果与补充说明：slate doctor btop",
        "● 返回工具页面",
    );
    navigate(&mut menu, b"\r", "btop · 选择操作");
    assert_eq!(tree_snapshot::tree(home.path()), saved_tree);
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
}

#[test]
fn tool_page_can_preview_decline_sync_and_return_without_native_probes_or_writes() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let binary = env.user_local_bin().join("btop");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut menu =
        PickerProcess::start_with_path(home.path(), &["tools"], Some(&env.user_local_bin()));
    menu.wait_for_output("选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "btop");
    navigate(&mut menu, &keys, "预览同步改动");
    assert!(!env.slate_cache_dir().exists());
    // With a known theme and a file checker, Enter defaults to inspection,
    // not a sync or setup action.
    menu.wait_for_output("只读检查配色文件 · 不启动工具");
    let offset = navigate(&mut menu, b"\r", "完整结果与补充说明：slate doctor btop");
    wait_after_result(
        &mut menu,
        offset,
        "完整结果与补充说明：slate doctor btop",
        "● 返回工具页面",
    );
    navigate(&mut menu, b"\x1b", "btop · 选择操作");
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "预览同步改动");
    let offset = navigate(&mut menu, &keys, "未修改文件或进程。");
    wait_after_result(&mut menu, offset, "未修改文件或进程。", "● 返回工具页面");
    let report = String::from_utf8_lossy(&menu.output[offset..]);
    assert!(!report
        .split_once("未修改文件或进程。")
        .unwrap()
        .1
        .contains("btop · 选择操作"));
    navigate(&mut menu, b"\x1b", "btop · 选择操作");
    assert_eq!(
        menu_choice_keys(&mut menu, "btop · 选择操作", "预览同步改动"),
        b"\r"
    );
    menu.wait_for_output("查看可能修改的文件 · 不写入、不运行工具");
    navigate(&mut menu, b"\r", "● 返回工具页面");
    navigate(&mut menu, b"\r", "btop · 选择操作");
    // Inspecting technical details should preserve focus when the page returns.
    let detail_keys = menu_choice_keys(&mut menu, "btop · 选择操作", "查看详细信息");
    let detail_offset = navigate(&mut menu, &detail_keys, "只读查看，未修改文件或启动工具。");
    wait_after_result(
        &mut menu,
        detail_offset,
        "只读查看，未修改文件或启动工具。",
        "● 返回工具页面",
    );
    navigate(&mut menu, b"\x1b", "btop · 选择操作");
    assert_eq!(
        menu_choice_keys(&mut menu, "btop · 选择操作", "查看详细信息"),
        b"\r"
    );
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "同步已保存主题");
    navigate(&mut menu, &keys, "确认同步上述工具的配色？");
    navigate(&mut menu, b"\r", "btop · 选择操作"); // default No
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "同步已保存主题");
    navigate(&mut menu, &keys, "确认同步上述工具的配色？");
    navigate(&mut menu, b"\x1b", "btop · 选择操作"); // Esc also declines.
    let keys = menu_choice_keys(&mut menu, "btop · 选择操作", "返回工具菜单");
    navigate(&mut menu, &keys, "选择工具或查看全部支持");
    let keys = tool_choice_keys(&mut menu, "退出工具菜单");
    menu.finish(&keys);
    assert!(!env.slate_cache_dir().exists());
    assert!(!slate_cli::adapter::BtopAdapter::config_path(&env).exists());
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
}

#[test]
fn hub_refreshes_saved_prompt_after_apply_and_keeps_it_when_leaving() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&bin));
    hub.wait_for_output("退出");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "命令提示符");
    navigate(&mut hub, &keys, "返回上级");
    let keys = menu_choice_keys(&mut hub, "选择提示符样式", "简洁双行");
    navigate(&mut hub, &keys, "◆  样式预览");
    let keys = menu_choice_keys(&mut hub, "样式预览", "查看改动并确认");
    navigate(&mut hub, &keys, "保存这个提示符样式？");
    let before = tree_snapshot::tree(home.path());
    // Enter alone must decline. Only selecting Save may write the reviewed plan.
    navigate(&mut hub, b"\r", "◆  样式预览");
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let keys = menu_choice_keys(&mut hub, "样式预览", "查看改动并确认");
    navigate(&mut hub, &keys, "保存这个提示符样式？");
    let keys = menu_choice_keys(&mut hub, "保存这个提示符样式？", "确认保存");
    navigate(&mut hub, &keys, "提示符  简洁双行");
    assert_eq!(
        menu_choice_keys(&mut hub, "想调整什么？", "命令提示符"),
        b"\r"
    );
    let saved = tree_snapshot::tree(home.path());
    // The saved style is also selected when reopening its catalog.
    navigate(&mut hub, b"\r", "选择提示符样式");
    assert_eq!(
        menu_choice_keys(&mut hub, "选择提示符样式", "简洁双行 · 已保存"),
        b"\r"
    );
    navigate(&mut hub, b"\r", "◆  样式预览");
    let keys = menu_choice_keys(&mut hub, "样式预览", "查看改动并确认");
    navigate(&mut hub, &keys, "保存这个提示符样式？");
    let keys = menu_choice_keys(&mut hub, "保存这个提示符样式？", "确认保存");
    let offset = navigate(&mut hub, &keys, "提示符  简洁双行");
    assert!(String::from_utf8_lossy(&hub.output[offset..]).contains("无需修改：简洁双行"));
    assert_eq!(tree_snapshot::tree(home.path()), saved);
    navigate(&mut hub, b"\r", "选择提示符样式");
    navigate(&mut hub, b"\x1b", "想调整什么？");
    let keys = menu_choice_keys(&mut hub, "想调整什么？", "退出");
    hub.finish(&keys);
    assert_eq!(tree_snapshot::tree(home.path()), saved);
    let output = String::from_utf8_lossy(&hub.output);
    assert!(output.contains("\x1b[?1049h"));
    assert_eq!(
        output.matches("\x1b[?1049h").count(),
        output.matches("\x1b[?1049l").count()
    );
    for receipt in ["查看恢复方案：", "已保存：简洁双行", "无需修改：简洁双行"]
    {
        let offset = output.find(receipt).expect("missing save receipt");
        let preceding = &output[..offset];
        assert!(
            preceding.rfind("\x1b[?1049l") > preceding.rfind("\x1b[?1049h"),
            "receipt must survive leaving the browser: {receipt}"
        );
    }
    let settings = fs::read_to_string(env.managed_file("config.toml")).unwrap();
    assert!(settings.contains("minimal"));
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    assert!(env.xdg_config_home().join("starship.toml").exists());
}

#[test]
fn hub_rejects_a_stale_toggle_instead_of_inverting_a_new_preference() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    navigate(
        &mut hub,
        b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r",
        "命令提示符：开",
    );
    assert!(!env.slate_cache_dir().exists());
    fs::write(env.managed_file("config.toml"), "[tools]\nstarship=false\n").unwrap();
    navigate(&mut hub, b"\r", "● 终端偏好");
    assert!(String::from_utf8_lossy(&hub.output).contains("返回首页，不自动重试"));
    hub.finish(b"\x1b");
    assert!(String::from_utf8_lossy(&hub.output).contains("preference changed"));
    assert_eq!(
        fs::read(env.managed_file("config.toml")).unwrap(),
        b"[tools]\nstarship=false\n"
    );
    assert!(!env.managed_file("managed/shell/env.zsh").exists());
    assert!(!env.slate_cache_dir().join("sounds").exists());
}

#[test]
fn hub_shell_toggle_refreshes_and_releases_writer_before_next_choice() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("config.toml"), "[sound]\nenabled=false\n").unwrap();
    let mut hub = PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
    hub.wait_for_output("退出");
    navigate(
        &mut hub,
        b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r",
        "命令提示符：开",
    );
    let offset = navigate(&mut hub, b"\r", "● 返回首页");
    let output = String::from_utf8_lossy(&hub.output[offset..]);
    assert!(output.contains("命令提示符：关"));
    assert!(output.contains("设置已保存，新开终端标签页后生效。"));
    assert!(!output.contains("Open a new terminal tab"));
    assert!(output.contains("slate restore"));
    assert!(output.contains("已创建恢复点 · 查看改动："));
    assert!(!output.contains("Pre-config recovery point:"));
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
    // Another invocation need not wait while the user reads the menu.
    drop(slate_cli::config::ConfigWriteGuard::acquire(&env).unwrap());
    let after_toggle = tree_snapshot::tree(home.path());
    navigate(&mut hub, b"\r", "想调整什么？");
    hub.finish_interrupted(b"\x03");
    assert_eq!(tree_snapshot::tree(home.path()), after_toggle);
    let settings: toml::Value = fs::read_to_string(env.managed_file("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(settings["tools"]["starship"].as_bool(), Some(false));
    assert!(env.managed_file("managed/shell/env.zsh").exists());
}

#[test]
fn shell_preference_hints_explain_next_action_without_toggling() {
    for enabled in [true, false] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(
            env.managed_file("config.toml"),
            format!("[tools]\nstarship={enabled}\nzsh_highlighting={enabled}\n"),
        )
        .unwrap();
        if enabled {
            fs::write(env.managed_file("autorun-fastfetch"), "").unwrap();
        }
        let before = tree_snapshot::tree(home.path());
        let mut hub =
            PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
        hub.wait_for_output("退出");
        let keys = menu_choice_keys(&mut hub, "想调整什么？", "终端偏好");
        navigate(
            &mut hub,
            &keys,
            if enabled {
                "关闭 Slate 的 Starship 自动加载，保留提示符配置"
            } else {
                "开启 Starship 自动加载，显示路径和 Git 等信息"
            },
        );
        navigate(
            &mut hub,
            b"\x1b[B",
            if enabled {
                "关闭 Slate 加载的命令着色，不卸载高亮插件"
            } else {
                "开启命令着色，需要已安装 Zsh 高亮插件"
            },
        );
        navigate(
            &mut hub,
            b"\x1b[B",
            if enabled {
                "关闭自动展示，不卸载 Fastfetch，仍可手动运行"
            } else {
                "开启终端启动时的系统信息展示，需要已安装 Fastfetch"
            },
        );
        assert_eq!(tree_snapshot::tree(home.path()), before);
        navigate(&mut hub, b"\x1b", "● 终端偏好");
        hub.finish(b"\x1b");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn hub_other_shell_toggles_offer_safe_return_and_keep_actionable_warnings() {
    for label in ["语法高亮：开", "启动信息：关"] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("config.toml"), "[sound]\nenabled=false\n").unwrap();
        let mut hub =
            PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
        hub.wait_for_output("退出");
        let keys = menu_choice_keys(&mut hub, "想调整什么？", "终端偏好");
        navigate(&mut hub, &keys, "命令提示符：开");
        let keys = menu_choice_keys(&mut hub, "终端偏好", label);
        let offset = navigate(&mut hub, &keys, "● 返回首页");
        let output = String::from_utf8_lossy(&hub.output[offset..]);
        assert!(output.contains("设置已保存，新开终端标签页后生效。"));
        assert!(output.contains("已创建恢复点 · 查看改动："));
        assert!(!output.contains("Pre-config recovery point:"));
        let config: toml::Value = fs::read_to_string(env.managed_file("config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        if label == "语法高亮：开" {
            assert_eq!(config["tools"]["zsh_highlighting"].as_bool(), Some(false));
            assert!(output.contains("slate restore"));
            assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
        } else {
            assert!(env.managed_file("autorun-fastfetch").exists());
            assert!(output.contains("fastfetch is not on PATH"));
            assert!(output.contains("slate restore"));
            assert_eq!(
                slate_cli::config::list_restore_points_with_env(&env)
                    .unwrap()
                    .len(),
                1
            );
        }
        assert_ne!(
            config
                .get("tools")
                .and_then(|tools| tools.get("starship"))
                .and_then(toml::Value::as_bool),
            Some(false)
        );
        let before_return = tree_snapshot::tree(home.path());
        navigate(&mut hub, b"\r", "想调整什么？");
        hub.finish(b"\x1b");
        assert_eq!(tree_snapshot::tree(home.path()), before_return);
    }
}

#[test]
fn hub_shell_toggles_unsafe_target_preserve_preferences_and_existing_files() {
    for label in ["命令提示符：开", "语法高亮：开", "启动信息：关"] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("config.toml"), "[sound]\nenabled=false\n").unwrap();
        fs::create_dir_all(env.managed_file("managed/shell/env.fish")).unwrap();
        fs::write(
            env.managed_file("managed/shell/env.zsh"),
            "# personal generated fixture\n",
        )
        .unwrap();
        drop(slate_cli::config::ConfigWriteGuard::acquire(&env).unwrap());
        let before = tree_snapshot::tree(home.path());
        let mut hub =
            PickerProcess::start_with_path(home.path(), &[], Some(&home.path().join("bin")));
        hub.wait_for_output("退出");
        let keys = menu_choice_keys(&mut hub, "想调整什么？", "终端偏好");
        navigate(&mut hub, &keys, "命令提示符：开");
        let keys = menu_choice_keys(&mut hub, "终端偏好", label);
        navigate(&mut hub, &keys, "● 终端偏好");
        assert!(String::from_utf8_lossy(&hub.output).contains("终端偏好未完成"));
        let inspect = menu_choice_keys(&mut hub, "想调整什么？", "检查配置");
        navigate(&mut hub, &inspect, "● 返回主菜单");
        navigate(&mut hub, b"\x1b", "● 检查配置");
        hub.finish(b"\x1b");
        assert!(!String::from_utf8_lossy(&hub.output).contains("设置已保存"));
        let after = tree_snapshot::tree(home.path());
        let changes: Vec<_> = before
            .keys()
            .chain(after.keys())
            .filter(|path| before.get(*path) != after.get(*path))
            .collect();
        assert!(changes.is_empty(), "changed paths: {changes:?}");
    }
}

#[test]
fn hub_rechecks_recovery_before_returning_from_a_subpage() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut hub = PickerProcess::start_with_args(home.path(), &[]);
    hub.wait_for_output("退出");
    navigate(&mut hub, b"\x1b[B\r", "选择工具或查看全部支持");
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    let record = env.slate_cache_dir().join("preview-session.json");
    fs::write(&record, b"unreadable recovery fixture").unwrap();
    let keys = tool_choice_keys(&mut hub, "返回首页");
    let offset = navigate(&mut hub, &keys, "上次预览尚未结束，请先检查恢复方案");
    assert!(!String::from_utf8_lossy(&hub.output[offset..]).contains("想调整什么？"));
    hub.finish_with_code(b"\x03", 130);
    assert_eq!(fs::read(record).unwrap(), b"unreadable recovery fixture");
    assert!(!env.config_dir().exists());
    let before = tree_snapshot::tree(home.path());
    let mut reopened = PickerProcess::start_with_args(home.path(), &[]);
    reopened.wait_for_output("上次预览尚未结束，请先检查恢复方案");
    reopened.finish(b"\x1b");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}
