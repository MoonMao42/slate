pub mod brand;
pub mod design;
pub mod detection;
pub mod env;
pub mod opacity;
pub mod platform;
pub mod session;

#[cfg(test)]
#[path = "../tests/support/tree.rs"]
pub(crate) mod test_tree;

pub mod adapter;
pub mod cli;
pub mod config;
pub mod error;
mod lookup;
pub mod theme;
pub mod wcag;
