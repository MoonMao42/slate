---
sketch: 001
name: role-differentiation
question: "不靠颜色,文字角色之间靠什么区分?"
winner: "B"
tags: [typography, roles, brand]
---

# Sketch 001: Role Differentiation

## Design Question

在 charm warmth + restrained 调性下,文字角色(命令键 / 路径 / 快捷键 / 代码 / 品牌强调 / 状态)如何在**不增加颜色维度**的前提下做到一眼可分?

颜色已经被锁在两条线上:① 品牌色(粉) ② 状态严重性(绿/黄/红)。其余所有区分必须靠**字体处理**、**容器**、或**字符前缀** —— 这是这次 sketch 的取舍。

## How to View

```
open .planning/sketches/001-role-differentiation/index.html
```

## Variants

- **A: Typography-led** — 排版承担一切。粗体 + 品牌色给命令,斜体 + 点状下划线给路径,粗体 + 字距给快捷键,斜体 + 弱化色给代码。无容器、无前缀字符。最接近经典 Unix CLI 密度。
- **B: Pill-led** — 容器承担区分。命令是 brand-tinted pill,快捷键是 bordered keycap,代码是 inline-code 块。每个角色边界清晰,扫描最快,但视觉重量重。
- **C: Glyph-led** — Unicode 字符承担语义。命令前 ▸,路径前 ↳,代码用 ‹ › 包裹,快捷键本身就是符号。无背景容器,密度接近 plain text,符号本身做"角色标签"。

## What to Look For

- 5 个真实场景在三个变体里**完全相同**——比的就是同一行内容,角色处理差异感。
- 注意 setup tail 第一段,密度差异最明显。
- error path 里 `slate theme dracola` 那一行,看代码 / 命令 / 快捷键三种角色叠加时是否互相打架。
- "config file callout" 是 BRAND-01 列出来的"quoted code"角色——评估三种处理在多行代码场景的可读性。
