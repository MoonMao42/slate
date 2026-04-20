//! Picker live-preview sub-module (Phase 19).
//!
//! Houses pure block renderers (migrated from src/cli/demo.rs per D-07),
//! responsive fold composer, and the D-04 Hybrid starship fork. Pure
//! data-in / String-out except `starship_fork` which spawns a subprocess.

pub mod blocks;
pub(super) mod compose;
pub(super) mod starship_fork;
