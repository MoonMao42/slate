//! RollbackGuard: RAII Drop guard that restores managed/* to the original
//! theme + opacity when the picker exits without committing (D-11). Pairs
//! with `std::panic::set_hook` because `Cargo.toml:67 panic = "abort"`
//! means Drop does NOT run on panic in release builds (RESEARCH Pitfall 1).
//!
//! Filled in Plan 19-03 (Wave 1).

#[cfg(test)]
mod tests {
    // Populated by Plan 19-03 (Wave 1).
}
