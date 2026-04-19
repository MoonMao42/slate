//! Compatibility shim — `Symbols` moved to `src/brand/symbols.rs` as part
//! of Phase 18 Wave 0 (D-21: `src/brand/` absorbs the design module).
//!
//! This re-export keeps every existing `use crate::design::symbols::Symbols`
//! in `src/cli/*` compiling byte-identical to its pre-Wave-0 shape. The
//! per-wave migration plans (18-02..07) rewrite those imports to
//! `use crate::brand::Symbols` as each surface is touched; Plan 18-08
//! deletes this shim once every caller has migrated.
//!
//! Tests live with the canonical definition in `src/brand/symbols.rs`.

pub use crate::brand::symbols::Symbols;
