//! Neovim colorscheme adapter — emits Lua files under the user's
//! `~/.config/nvim/` runtimepath. See Phase 17 plans 01-08.
//!
//! Delivered in waves:
//!   W1  — src/design/nvim_highlights.rs (role→group table)
//!   W2  — render_colorscheme + render_shim (this file)
//!   W3  — render_loader + write_state_file (this file)
//!   W4  — plugin groups + lualine_theme (this file)
//!   W5  — NvimAdapter trait impl + registry wiring (this file)

#[cfg(test)]
mod tests {
    // Wave 2+ fills these in.
}
