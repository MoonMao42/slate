# Sketch Manifest

## Design Direction

slate CLI 文字角色系统(BRAND-01/02)走 **charm warmth + restrained** 路线:整体调性温暖、有设计感(参考 charm.sh / gum / glow / lazygit 家族),但色彩密度克制——以品牌色(**lavender `#7287fd`**,语义匹配 slate 石板调)为主轴,代码 / 路径 / 快捷键 / 命令键之间靠**容器(pill)** 区分而非靠颜色。状态严重性(success / warn / error)是唯一允许多色的维度。

## Reference Points

- charm.sh / gum / glow — 主气质、字符画框
- cliclack — slate 当前已使用的输出库
- lazygit — 状态色克制度
- gum (charmbracelet) — restrained palette 的标杆

## Sketches

| # | Name | Design Question | Winner | Tags |
|---|------|----------------|--------|------|
| 001 | role-differentiation | 不靠颜色,文字角色之间靠什么区分? | **B: Pill-led** | typography, roles |
| 002 | accent-placement | 那 1-2 个颜色应该落在哪里? | **B: Medium** (lavender 在命令 + 主题 + ★) | color, brand |
| 003 | header-receipt | 头部 / 完成回执 / 错误框 怎么组合? | **B: Tree** (◆ ┃ └ charm.sh 叙事) | banner, layout |

## Final Direction

底层 = `pill-led` 角色区分 + `medium` lavender 强调密度 + `tree` 高光叙事。

**Brand color:** `#7287fd` (catppuccin lavender — 语义匹配 slate 石板调)
**Role surface:**

| 角色 | 视觉处理 |
|------|---------|
| command | `bg: rgba(114,135,253,0.14)` + `fg: lavender` + `radius: 3px` pill |
| path | dim italic,无容器 |
| shortcut | bordered keycap pill,默认前景色 |
| code | `bg: #313244` inline-code pill |
| brand | lavender 文字 |
| status | severity 三色(✓ 绿 / ⚠ 黄 / ✗ 红) |

**Composition:** 多步骤流程用 `◆ heading + ┃ ├─ └─` 树枝。单行命令保持 inline。完成回执用 `└─ ★ ...` 树尾收。
