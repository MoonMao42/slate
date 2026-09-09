//! Build bounded header/body/footer sections before writing any terminal bytes.
use super::*;

pub(super) struct Frame {
    header: Vec<String>,
    body: Vec<String>,
    footer: Vec<String>,
    pub(super) viewport: Option<scroll::Viewport>,
}

impl Frame {
    pub(super) fn write<W: io::Write>(&self, out: &mut W, cols: u16, rows: u16) -> Result<()> {
        queue_io(queue!(
            out,
            SetAttribute(Attribute::Reset),
            ResetColor,
            Clear(ClearType::All),
            MoveTo(0, 0)
        ))?;
        let width = cols.saturating_sub(1) as usize;
        let mut written = 0;
        for section in [&self.header, &self.body, &self.footer] {
            // A vertically cropped multiline prompt may not reach its closing
            // SGR reset. Do not carry that style into the next section/footer.
            queue_io(queue!(out, SetAttribute(Attribute::Reset), ResetColor))?;
            for line in section.iter().flat_map(|line| line.split('\n')) {
                let line = line.strip_suffix('\r').unwrap_or(line);
                if written >= rows as usize || width == 0 {
                    break;
                }
                if written > 0 {
                    queue_io(queue!(out, Print("\r\n")))?;
                }
                queue_io(queue!(out, Print(scroll::clip_line(line, width))))?;
                written += 1;
            }
        }
        // No newline on the last row: advancing there would scroll the header
        // off screen. SGR is retained across body lines, reset on section exit.
        queue_io(queue!(out, SetAttribute(Attribute::Reset), ResetColor))
    }
}

fn muted(roles: Option<&Roles<'_>>, text: &str) -> String {
    roles.map_or_else(|| text.to_owned(), |r| r.path(text))
}

fn captured_line(render: impl FnOnce(&mut Vec<u8>) -> Result<()>) -> Result<String> {
    let mut bytes = Vec::new();
    render(&mut bytes)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| crate::error::SlateError::Internal("Invalid picker display text".into()))?;
    Ok(text.trim_end_matches(['\r', '\n']).to_owned())
}

fn compact(state: &PickerState, flash: Option<&str>, rows: u16) -> Result<Frame> {
    let body = vec![format!("› {}", state.get_current_theme()?.name)];
    let mut footer = Vec::new();
    if rows >= 2 {
        footer.push(tr("Esc 取消", "Esc cancel").into());
    }
    if rows >= 4 {
        footer.push(
            flash
                .unwrap_or(tr("↑↓ 主题 · Enter 保存", "↑↓ theme · Enter save"))
                .into(),
        );
    }
    Ok(Frame {
        header: Vec::new(),
        body,
        footer,
        viewport: None,
    })
}

pub(super) fn list(
    state: &PickerState,
    flash: Option<&str>,
    cols: u16,
    rows: u16,
    roles: Option<&Roles<'_>>,
    supports_opacity: bool,
) -> Result<Frame> {
    if rows < 6 {
        return compact(state, flash, rows);
    }
    let logo = roles.map_or_else(|| "✦ slate".into(), Roles::logo);
    let header = vec![if cols >= 60 {
        format!(
            "  {logo}   {}",
            muted(
                roles,
                tr(
                    "主题与透明度 · Tab 预览",
                    "theme + opacity picker · Tab ▸ preview"
                )
            )
        )
    } else {
        format!("{logo} · {}", tr("主题", "Themes"))
    }];

    let mut footer = Vec::new();
    if rows >= 8 {
        let position = state.selected_theme_index() + 1;
        let counter = format!("  {position}/{}", state.theme_ids().len());
        footer.push(muted(
            roles,
            if rows < 12 {
                flash.unwrap_or(&counter)
            } else {
                &counter
            },
        ));
    }
    if rows >= 24 && cols >= 60 && state.has_selection() {
        let theme = state.get_current_theme()?;
        footer.extend(
            compose::compose_mini(&theme.palette, roles)
                .trim_end_matches('\n')
                .lines()
                .map(|line| format!("  {line}")),
        );
    }
    if rows >= 12 && supports_opacity && state.has_selection() {
        let effective = get_effective_opacity_for_rendering(state);
        if cols >= 70 {
            footer.push(captured_line(|out| {
                queue_io(queue!(out, Print(tr("  透明度：  ", "  Opacity:  "))))?;
                for slot in [
                    OpacityPreset::Solid,
                    OpacityPreset::Frosted,
                    OpacityPreset::Clear,
                ] {
                    render_opacity_slot(out, slot, effective)?;
                    queue_io(queue!(out, Print("  ")))?;
                }
                Ok(())
            })?);
        } else {
            footer.push(muted(
                roles,
                &format!(
                    "{}{} · ←→",
                    tr("透明度：", "Opacity: "),
                    opacity_to_label(effective)
                ),
            ));
        }
    }
    let (primary, extra) = if cols < 40 {
        (
            tr("Enter 保存 · Esc 取消", "Enter save · Esc cancel"),
            tr("↑↓ 主题 · Tab 预览", "↑↓ theme · Tab preview"),
        )
    } else {
        (
            if cols >= 70 && supports_opacity {
                tr(
                    "↑↓/jk 主题 · ←→/hl 透明度 · Enter 保存 · Esc 取消",
                    "↑↓/jk theme · ←→/hl opacity · Enter save · Esc cancel",
                )
            } else {
                tr(
                    "↑↓ 主题 · Enter 保存 · Esc 取消",
                    "↑↓ theme · Enter save · Esc cancel",
                )
            },
            tr("Tab 预览 · s 保存配对", "Tab preview · s save-auto"),
        )
    };
    let extra = if rows < 8 {
        flash.unwrap_or(extra)
    } else {
        extra
    };
    footer.extend([muted(roles, primary), muted(roles, extra)]);
    if rows >= 12 {
        if let Some(flash) = flash {
            footer.push(muted(roles, flash));
        }
    }
    let budget = (rows as usize).saturating_sub(header.len() + footer.len());
    let body = list_body(state, cols, budget, roles)?;
    Ok(Frame {
        header,
        body,
        footer,
        viewport: None,
    })
}

fn list_body(
    state: &PickerState,
    cols: u16,
    budget: usize,
    roles: Option<&Roles<'_>>,
) -> Result<Vec<String>> {
    if budget == 0 {
        return Ok(Vec::new());
    }
    if !state.has_selection() {
        return Ok(["  No themes available."]
            .into_iter()
            .take(budget)
            .map(str::to_owned)
            .collect());
    }
    let registry = ThemeRegistry::new()?;
    let themes: Vec<_> = state
        .theme_ids()
        .iter()
        .map(|id| registry.get(id).expect("picker catalog ID must resolve"))
        .collect();
    let cursor = state.selected_theme_index();
    let count = budget.min(themes.len());
    let mut start = cursor.saturating_sub(count / 2).min(themes.len() - count);
    let mut end = start + count;
    // The first visible family needs a heading, including when its earlier
    // variants were scrolled off screen. Budget headings as well as theme rows.
    let cost = |start: usize, end: usize| {
        end - start
            + 1
            + themes[start..end]
                .windows(2)
                .filter(|pair| pair[0].family != pair[1].family)
                .count()
    };
    while end - start > 1 && cost(start, end) > budget {
        if cursor - start > end - 1 - cursor {
            start += 1;
        } else {
            end -= 1;
        }
    }
    let mut body = Vec::new();
    let mut family = None;
    for (index, theme) in themes.iter().enumerate().take(end).skip(start) {
        if budget >= 2 && family != Some(theme.family.as_str()) {
            body.push(captured_line(|out| {
                queue_family_heading(out, roles, &theme.family)
            })?);
            family = Some(theme.family.as_str());
        }
        body.push(captured_line(|out| {
            queue_variant_row(out, theme, index == cursor, cols, roles)
        })?);
    }
    Ok(body)
}

pub(super) fn full(
    state: &PickerState,
    flash: Option<&str>,
    cols: u16,
    rows: u16,
    roles: Option<&Roles<'_>>,
    prompt: Option<&str>,
) -> Result<Frame> {
    if rows < 7 {
        return compact(state, flash, rows);
    }
    let theme = state.get_current_theme()?;
    let header = vec![
        if cols >= 60 {
            let logo = roles.map_or_else(|| "✦ slate".into(), Roles::logo);
            format!(
                "  {logo}   {}",
                muted(roles, tr("预览 · Tab 返回", "preview · Tab to return"))
            )
        } else {
            muted(roles, tr("预览 · Tab 返回", "Preview · Tab return"))
        },
        format!("› {}", theme.name),
    ];
    let mut footer = vec![
        String::new(), // Filled from the measured viewport below.
        muted(
            roles,
            tr("Enter 保存 · Esc 取消", "Enter save · Esc cancel"),
        ),
        muted(roles, tr("↑↓ 主题 · Tab 返回", "↑↓ theme · Tab return")),
    ];
    if let Some(flash) = flash {
        if rows >= 12 {
            footer.push(muted(roles, flash));
        } else {
            // Feedback replaces secondary help, never confirmation/cancel or
            // preview rows, when the window cannot afford a separate line.
            footer[2] = muted(roles, flash);
        }
    }
    let budget = (rows as usize).saturating_sub(header.len() + footer.len());
    // Every block remains reachable, regardless of terminal height.
    let document = compose::compose_full_in(
        &theme.palette,
        compose::FoldTier::Large,
        roles,
        prompt,
        crate::cli::ui_language::current(),
    );
    let view = scroll::Viewport::new(state.preview_scroll, budget, document.lines().count());
    let mut lines = document.split_inclusive('\n');
    let skipped_bytes: usize = lines.by_ref().take(view.start).map(str::len).sum();
    let mut body: Vec<_> = lines
        .take(budget)
        .map(|line| format!("  {}", line.trim_end_matches(['\r', '\n'])))
        .collect();
    if let Some(first) = body.first_mut() {
        // Carry only SGR into the first visible line, never skipped text or
        // terminal actions. Frame::write resets again before the sticky footer.
        first.insert_str(0, &scroll::style_prefix(&document[..skipped_bytes]));
    }
    footer[0] = muted(roles, &view.label(cols));
    Ok(Frame {
        header,
        body,
        footer,
        viewport: Some(view),
    })
}
