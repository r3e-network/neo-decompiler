//! Control flow lifting helpers for the high-level emitter.
//!
//! Neo VM scripts are stack based, but we still try to reconstruct structured
//! control-flow patterns (loops, if/else blocks, and TRY/CATCH/FINALLY shapes)
//! from the linear instruction stream.

mod branches;
mod jumps;
mod loops;
mod targets;
mod try_blocks;
