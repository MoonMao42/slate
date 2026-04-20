//! D-04 Hybrid starship fork + fallback for Tab full-preview mode.
//!
//! Per-subprocess `.env("STARSHIP_CONFIG", managed_path)` (NOT
//! `std::env::set_var` — V12 security). `which::which` probe first;
//! fallback to self-drawn SAMPLE_TOKENS on any error path (D-04 locked).
//!
//! Filled in Plan 19-06 (Wave 3).

#[cfg(test)]
mod tests {
    // Populated by Plan 19-06 (Wave 3).
}
