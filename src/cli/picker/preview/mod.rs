//! Picker live-preview sub-module.
//! Houses pure block renderers (migrated from src/cli/demo.rs per),
//! responsive fold composer, and the Hybrid starship fork. Pure
//! data-in / String-out except `starship_fork` which spawns a subprocess.

pub mod blocks;
pub(super) mod compose;
// promoted from `pub(super)` → `pub` so the integration test
// suite (`tests/picker_starship_fork_fixture.rs`) can call
// `fork_starship_prompt` + match on `StarshipForkError` without going
// through a shim. Surface area is narrow (one fn + one enum); 
// may consume it too.
pub mod starship_fork;
