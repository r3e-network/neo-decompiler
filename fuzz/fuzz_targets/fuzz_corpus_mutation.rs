//! Fuzz with mutated real contracts.
//!
//! The other targets build inputs from scratch: raw bytes, a synthetic
//! opcode program, or a typed IR tree. Those inputs exercise the decompiler
//! on shapes nobody wrote, but they miss the shapes real contracts have --
//! nested try/catch/finally, long switch chains, deeply nested loops, wide
//! phi merges, and the specific operand patterns the Neo compiler emits.
//!
//! This target starts from real contracts. The corpus is the pinned devpack
//! set, 103 contracts that together cover the language surface, plus the
//! committed edge-case fixtures. Each input selects a contract and applies
//! mutations that keep the NEF container parseable while changing the
//! script, so the decompiler always reaches the interesting stages on
//! realistic code.
//!
//! The mutations are chosen to break specific things:
//!
//! * Byte flips inside the script corrupt operands, including jump offsets,
//!   so the CFG builder sees edges that point anywhere.
//! * Operand truncation removes bytes so instructions run past the end of the
//!   script.
//! * Chunk duplication repeats a region, creating loops and duplicated
//!   blocks.
//! * Chunk deletion removes a region, which can delete a jump target and
//!   leave the CFG with a dangling edge.
//!
//! The container is rebuilt with the crate's own serializer and the checksum
//! is recomputed, so the NEF parser accepts the file and the mutation reaches
//! the decompiler instead of being rejected at the door. Using the crate's
//! serializer rather than a hand-written layout is deliberate: a separate
//! implementation of the container format drifts from the parser, and a
//! drifted layout turns the target into a no-op that mutates the wrong bytes
//! and then fails to parse.
//!
//! The invariants: no panic, and brace-balanced rendered output.
#![no_main]

#[path = "support/corpus.rs"]
mod corpus;

use corpus::contracts;
use libfuzzer_sys::fuzz_target;
use neo_decompiler::decompiler::ir::brace_balanced;
use neo_decompiler::nef::NefParser;
use neo_decompiler::{Decompiler, OutputFormat};

/// Reads decisions from the fuzzer input, cycling when it is exhausted.
struct Decisions<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decisions<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn byte(&mut self) -> u8 {
        if self.data.is_empty() {
            return 0;
        }
        let value = self.data[self.pos % self.data.len()];
        self.pos += 1;
        value
    }

    fn usize(&mut self, modulo: usize) -> usize {
        if modulo == 0 {
            return 0;
        }
        (self.byte() as usize) % modulo
    }
}

/// Apply a mutation to the script bytes.
///
/// Every branch is a no-op on an empty script. A script can become empty
/// through truncation, and a later mutation in the same input must not
/// index into it.
fn mutate_script(script: &mut Vec<u8>, decisions: &mut Decisions<'_>) {
    if script.is_empty() {
        return;
    }

    let mutation_count = 1 + decisions.usize(6);
    for _ in 0..mutation_count {
        if script.is_empty() {
            break;
        }
        match decisions.usize(5) {
            // Byte flips: corrupt an operand or a jump offset.
            0 => {
                let count = 1 + decisions.usize(4);
                for _ in 0..count {
                    let index = decisions.usize(script.len());
                    script[index] ^= 1 << decisions.usize(8);
                }
            }
            // Overwrite with an arbitrary byte.
            1 => {
                let index = decisions.usize(script.len());
                script[index] = decisions.byte();
            }
            // Truncate the script.
            2 => {
                let keep = decisions.usize(script.len());
                script.truncate(keep);
            }
            // Duplicate a chunk.
            3 => {
                let start = decisions.usize(script.len());
                let len = 1 + decisions.usize(16);
                let end = (start + len).min(script.len());
                let chunk = script[start..end].to_vec();
                let insert_at = decisions.usize(script.len() + 1);
                let insert_at = insert_at.min(script.len());
                script.splice(insert_at..insert_at, chunk);
            }
            // Delete a chunk: this can remove a jump target and leave the
            // CFG builder with a dangling edge.
            _ => {
                let start = decisions.usize(script.len());
                let len = 1 + decisions.usize(16);
                let end = (start + len).min(script.len());
                script.drain(start..end);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let contracts = contracts();
    if contracts.is_empty() {
        // No corpus is available. The target has nothing to mutate, and
        // asserting here would fail every run on a machine without the
        // artifacts rather than reporting a defect.
        return;
    }
    report_corpus_once(contracts.len());
    let mut decisions = Decisions::new(data);

    // Pick a seed contract. The first bytes choose it, so a campaign walks
    // the whole corpus rather than revisiting one contract.
    let seed = &contracts[decisions.usize(contracts.len())];

    // Parse the seed with the crate's own parser. A seed the parser rejects
    // is not interesting: the mutation would never reach the decompiler.
    let Ok(mut nef) = NefParser::new().parse(&seed.nef) else {
        return;
    };

    // Mutate the script in place.
    mutate_script(&mut nef.script, &mut decisions);

    // An empty script is rejected by the parser, so the mutation is a no-op
    // for that case.
    if nef.script.is_empty() {
        return;
    }

    // Rebuild the container. The serializer recomputes the checksum, so the
    // parser accepts the result and the mutation reaches the decompiler.
    let bytes = nef.to_bytes();
    if NefParser::new().parse(&bytes).is_err() {
        return;
    }

    // The full pipeline, including the manifest-aware analysis, must not
    // panic on the mutated contract.
    let decompiler = Decompiler::new();
    let manifest = seed.parsed_manifest();
    if let Ok(decompilation) =
        decompiler.decompile_bytes_with_manifest(&bytes, manifest, OutputFormat::All)
    {
        if let Some(csharp) = decompilation.csharp.as_deref() {
            assert!(
                brace_balanced(csharp),
                "unbalanced braces after mutating a real contract:\n{csharp}"
            );
        }
    }
});

/// Report the corpus size once per run.
///
/// A campaign that loaded an empty corpus would otherwise look exactly like
/// a clean one, so the size is written to the log where it is visible.
fn report_corpus_once(size: usize) {
    use std::sync::Once;
    static REPORTED: Once = Once::new();
    REPORTED.call_once(|| {
        eprintln!("corpus size: {size} contracts");
    });
}
