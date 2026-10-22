#![no_main]

use libfuzzer_sys::fuzz_target;
use neo_decompiler::Disassembler;

fuzz_target!(|data: &[u8]| {
    let disassembler = Disassembler::new();
    let Ok(instructions) = disassembler.disassemble(data) else {
        // A script the disassembler rejects has nothing to check.
        return;
    };

    // Every instruction has to point inside the script it came from. An
    // instruction that points past the end is how a jump becomes a read
    // of whatever follows the container in memory.
    for instruction in &instructions {
        assert!(
            instruction.offset < data.len(),
            "the instruction at offset {} points past the end of a {} byte script",
            instruction.offset,
            data.len()
        );
    }

    // The instructions have to come out in order. A disassembler that
    // emits them out of order would make every later offset wrong.
    for pair in instructions.windows(2) {
        assert!(
            pair[0].offset < pair[1].offset,
            "the instructions are not in offset order: {} then {}",
            pair[0].offset,
            pair[1].offset
        );
    }
});
