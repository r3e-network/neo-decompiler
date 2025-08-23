//! Analysis passes framework

// Module stubs for analysis passes
// Complete analysis framework implementation

pub mod cfg;
pub mod effects;
pub mod types;

// CFG demonstration module
pub mod cfg_demo;

// Re-export key types
pub use cfg::*;
pub use cfg_demo::*;
pub use effects::*;
pub use types::*;
