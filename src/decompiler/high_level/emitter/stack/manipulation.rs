//! Stack manipulation opcode handlers.
//!
//! These helpers mutate the emitter value stack to track VM stack operations
//! such as `DUP`, `DROP`, `PICK`, and reversal opcodes.

mod basic;
mod indexed;
mod reorder;
mod reverse;
