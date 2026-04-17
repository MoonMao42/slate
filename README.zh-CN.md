<p align="center">
  <img
    width="180"
    src="./assets/logo-icon.svg"
    alt="slate logo"
  />
</p>

<h1 align="center">slate</h1>

<p align="center">
  为 macOS 和 Linux 准备的一键终端配置：主题、提示符、字体、周边工具一次性调成同一套。
</p>

<p align="center">
  <a href="./README.md">English</a> · 简体中文
</p>

<p align="center">
  <a href="https://github.com/MoonMao42/slate/releases"><img src="https://img.shields.io/github/v/release/MoonMao42/slate?style=flat-square&color=585b70" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-585b70?style=flat-square" alt="macOS and Linux" />
  <img src="https://img.shields.io/badge/built_with-Rust-585b70?style=flat-square&logo=rust&logoColor=white" alt="Built with Rust" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme live preview" width="700" />
  <br />
  <sub>实时预览，切换主题时整套终端风格一起更新。</sub>
</p>

## 为什么做这个

我一直没找到一款真正顺手的终端美化工具。每次想把终端弄漂亮一点，就得去翻别人的 dotfile 仓库、到处抄配置、叠一堆插件。折腾半天，环境可能一团糟还恢复不过来，必须得去研究到底动了什么。

所以我写了 slate：一条命令把终端、提示符、字体、CLI 工具统一调成一套风格；所有 slate 写的东西都放在它自己管的文件里，想卸载就 `slate clean`，是真的干净。

## 安装

```bash
# macOS · Homebrew
brew install MoonMao42/homebrew-tap/slate-cli

# macOS 或 Linux · 一键脚本
curl -fsSL https://raw.githubusercontent.com/MoonMao42/slate/main/install.sh | sh

# Rust 用户
cargo install slate-cli
```

然后运行 `slate setup`。

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub><code>slate setup</code> 一键配置。</sub>
</p>

## 它做了什么

- 一套配色同步到 Ghostty、Kitty、Alacritty、Starship、bat、delta、eza、lazygit、fastfetch、tmux、zsh-syntax-highlighting。
- 🌓 自动跟随系统深浅色：macOS 走原生 watcher，Linux 优先走 XDG Desktop Portal（GNOME 可退回 `gsettings`）。
- 不改你的 dotfile：slate 写进自己管的 include 文件里，每次改动前先快照，一条命令可回滚。

<p align="center">
  <img src="./assets/fastfetch-preview.png" alt="fastfetch themed output" width="600" />
  <br />
  <sub>终端、提示符、系统信息、常用 CLI，全部共用同一套配色。</sub>
</p>

## 自动深浅色

```
浅色模式 → 浅色主题 + 匹配的提示符、语法高亮、工具配色
深色模式 → 深色主题 + 匹配的提示符、语法高亮、工具配色
```

从主菜单里开启（`slate` → Auto-Theme）。每个主题家族自带深浅色配对，也可以在主菜单里自己重新配对。

## 支持情况

官方构建目标：`x86_64-apple-darwin`、`aarch64-apple-darwin`、`x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`。Linux 主要在 Debian/Ubuntu + GNOME 上验证。

| 终端 | 状态 | 说明 |
|------|------|------|
| Ghostty | 最推荐 | 完整支持——热重载、透明度、watcher 自动拉起 |
| Kitty | 完整 | 通过 remote control 实时推送 |
| Alacritty | 完整 | 行内预览与热重载 |
| Terminal.app | 部分 | 仅 macOS；不支持 live preview、不支持透明度、字体无法自动更换 |
| 其他 | 尽力而为 | Shell 与 CLI 工具层主题通用；终端自身视觉效果看其能力 |

Shell：`zsh`、`bash`、`fish`。`zsh` 已在本机验证；`bash` 与 `fish` 已接入，尚待更大范围测试。

<details>
<summary><strong>全部命令</strong></summary>

```bash
slate                         # 交互式主菜单
slate setup                   # 引导式配置
slate setup --quick           # 非交互、默认值
slate setup --only starship   # 单独重配某个工具
slate theme                   # 带实时预览的主题选择器
slate theme <name>            # 按名称应用
slate theme --auto            # 跟随系统深浅色
slate font                    # Nerd Font 选择器
slate config set opacity frosted  # 透明度：solid / frosted / clear
slate config set sound off    # 反馈音开关
slate export                  # 把当前配置导成 URI
slate import <uri>            # 从 URI 恢复配置
slate share                   # 截取带水印的终端图
slate status                  # 查看当前配置
slate list                    # 列出所有主题
slate restore                 # 选一个快照回滚
slate restore --list          # 列出所有回滚点
slate clean                   # 清除 slate 写下的一切
```

</details>

<details>
<summary><strong>工作原理</strong></summary>

slate 通过独立的 include 文件跟你现有的配置共存，不会替换你的 dotfile：

```text
~/.config/slate/config.toml        # 偏好（主题、字体、开关）
~/.config/slate/auto.toml          # 深浅色配对
~/.config/slate/managed/<tool>/*   # slate 自管的生成物
~/.config/<tool>/...               # 你自己的文件，原样不动
```

Ghostty 用 `config-file = ...`；Kitty/Alacritty 用 `include`/`import`；zsh 则是 `.zshrc` 里一段带明确 START/END 标记的代码块。slate 的文件归 slate 管，你自己的文件永远不动。

</details>

## 主题

共 18 款，来自 8 个家族：Catppuccin · Tokyo Night · Rosé Pine · Kanagawa · Everforest · Dracula · Nord · Gruvbox。

## 许可

MIT。

## 致谢

站在一堆很棒的项目之上：
[Ghostty](https://ghostty.org/) · [Kitty](https://sw.kovidgoyal.net/kitty/) · [Alacritty](https://github.com/alacritty/alacritty) · [Starship](https://github.com/starship/starship) · [bat](https://github.com/sharkdp/bat) · [delta](https://github.com/dandavison/delta) · [eza](https://github.com/eza-community/eza) · [lazygit](https://github.com/jesseduffield/lazygit) · [fastfetch](https://github.com/fastfetch-cli/fastfetch) · [tmux](https://github.com/tmux/tmux) · [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) · [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts)。
