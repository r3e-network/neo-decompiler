//! Benchmarks for the decompiler.
//!
//! These benchmarks measure the performance of key decompiler operations.
//! Run with: `cargo bench`

#![allow(clippy::explicit_counter_loop)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use neo_decompiler::{
    decompiler::cfg::ssa::SsaBuilder,
    decompiler::cfg::CfgBuilder,
    instruction::{Instruction, OpCode, Operand},
    nef::NefParser,
    Decompiler,
};

fn write_varint(buffer: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buffer.push(value as u8),
        0xFD..=0xFFFF => {
            buffer.push(0xFD);
            buffer.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buffer.push(0xFE);
            buffer.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// Build a deterministic NEF containing `PUSH1; DROP` pairs and a final `RET`.
///
/// Keeping this fixture in memory makes the benchmark independent of the
/// optional TestingArtifacts checkout and measures only decompiler work.
fn synthetic_nef(pair_count: usize) -> Vec<u8> {
    let mut script = Vec::with_capacity(pair_count.saturating_mul(2).saturating_add(1));
    for _ in 0..pair_count {
        script.extend_from_slice(&[0x11, 0x45]);
    }
    script.push(0x40);

    let mut nef = Vec::with_capacity(4 + 64 + 5 + script.len() + 4);
    nef.extend_from_slice(b"NEF3");
    let mut compiler = [0_u8; 64];
    compiler[..9].copy_from_slice(b"criterion");
    nef.extend_from_slice(&compiler);
    nef.push(0); // empty source URL
    nef.push(0); // reserved byte
    nef.push(0); // no method tokens
    nef.extend_from_slice(&0_u16.to_le_bytes());
    write_varint(&mut nef, script.len() as u32);
    nef.extend_from_slice(&script);
    let checksum = NefParser::calculate_checksum(&nef);
    nef.extend_from_slice(&checksum.to_le_bytes());
    nef
}

/// Generate a simple sequence of instructions for testing.
fn simple_instructions(count: usize) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(count);

    for offset in 0..count {
        let opcode = match offset % 5 {
            0 => OpCode::Push1,
            1 => OpCode::Push2,
            2 => OpCode::Add,
            3 => OpCode::Dup,
            _ => OpCode::Drop,
        };
        instructions.push(Instruction::new(offset, opcode, None));
    }

    instructions
}

/// Generate instructions with branching for CFG testing.
fn branched_instructions(count: usize) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(count);
    let mut offset = 0;

    // Entry block
    instructions.push(Instruction::new(offset, OpCode::Push1, None));
    offset += 1;

    for _i in (0..count).step_by(3) {
        instructions.push(Instruction::new(offset, OpCode::Push1, None));
        offset += 1;
        instructions.push(Instruction::new(
            offset,
            OpCode::Jmpif,
            // Both paths preserve one accumulator value: skip the PUSH2 and
            // ADD together, rather than entering ADD with a missing operand.
            Some(Operand::Jump(4)),
        ));
        offset += 2;
        instructions.push(Instruction::new(offset, OpCode::Push2, None));
        offset += 1;
        instructions.push(Instruction::new(offset, OpCode::Add, None));
        offset += 1;
    }

    // Exit
    instructions.push(Instruction::new(offset, OpCode::Ret, None));

    instructions
}

fn bench_cfg_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfg_construction");

    for size in [10, 100, 1000] {
        let instructions = simple_instructions(size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let cfg = CfgBuilder::new(black_box(&instructions)).build();
                black_box(cfg)
            });
        });
    }

    group.finish();
}

fn bench_ssa_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("ssa_construction");

    for size in [10, 50, 100] {
        let instructions = branched_instructions(size);
        let cfg = CfgBuilder::new(&instructions).build();
        let probe = SsaBuilder::new(&cfg, &instructions).build();
        assert!(
            !probe.render().contains('?'),
            "SSA benchmark fixture must not contain missing stack values"
        );

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let ssa = SsaBuilder::new(black_box(&cfg), black_box(&instructions)).build();
                black_box(ssa)
            });
        });
    }

    group.finish();
}

fn bench_decompilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompilation");

    let decompiler = Decompiler::new();
    for pair_count in [16, 128, 512] {
        let nef = synthetic_nef(pair_count);
        let probe = decompiler
            .decompile_bytes(&nef)
            .expect("decompilation benchmark fixture must be valid");
        assert_eq!(probe.instructions.len(), pair_count * 2 + 1);
        group.bench_with_input(
            BenchmarkId::new("push_drop_pairs", pair_count),
            &pair_count,
            |b, _| {
                b.iter(|| {
                    let result = decompiler.decompile_bytes(black_box(&nef));
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_disassembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("disassembly");
    let decompiler = Decompiler::new();

    for pair_count in [64, 1_024, 8_192] {
        let nef = synthetic_nef(pair_count);
        let probe = decompiler
            .disassemble_bytes(&nef)
            .expect("disassembly benchmark fixture must be valid");
        assert_eq!(probe.instructions.len(), pair_count * 2 + 1);
        group.bench_with_input(
            BenchmarkId::from_parameter(pair_count),
            &pair_count,
            |b, _| {
                b.iter(|| {
                    let result = decompiler.disassemble_bytes(black_box(&nef));
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_cfg_construction,
    bench_ssa_construction,
    bench_decompilation,
    bench_disassembly
);
criterion_main!(benches);
