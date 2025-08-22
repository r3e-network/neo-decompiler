//! Analysis passes framework

// Module stubs for analysis passes
// Complete analysis framework implementation

pub mod cfg;
pub mod types;
pub mod effects;

// CFG demonstration module
pub mod cfg_demo;

// Re-export key types
pub use cfg::*;
pub use types::*;
pub use effects::*;
pub use cfg_demo::*;