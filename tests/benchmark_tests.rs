//! Performance benchmarks for the Neo N3 decompiler
//!
//! Comprehensive benchmarks to measure performance characteristics and
//! identify bottlenecks in the decompilation pipeline.

use crate::common::*;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use neo_decompiler::*;
use std::time::Duration;

/// Benchmark basic NEF parsing
fn bench_nef_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("nef_parsing");

    // Different sized NEF files
    let test_cases = vec![
        ("minimal", SampleNefData::minimal()),
        ("control_flow", SampleNefData::with_control_flow()),
    ];

    for (name, sample_data) in test_cases {
        let nef_bytes = sample_data.to_bytes();
        let parser = NEFParser::new();

        group.throughput(Throughput::Bytes(nef_bytes.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse", name), &nef_bytes, |b, data| {
            b.iter(|| {
                let _ = parser.parse(data).unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark manifest parsing
fn bench_manifest_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("manifest_parsing");

    let test_cases = vec![
        ("simple", SampleManifest::simple_contract()),
        ("nep17", SampleManifest::nep17_token()),
    ];

    for (name, sample_manifest) in test_cases {
        let json_str = sample_manifest.to_json();
        let parser = ManifestParser::new();

        group.throughput(Throughput::Bytes(json_str.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse", name), &json_str, |b, json| {
            b.iter(|| {
                let _ = parser.parse(json).unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark disassembly performance
fn bench_disassembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("disassembly");

    let config = DecompilerConfig::default();
    let disassembler = Disassembler::new(&config);

    // Create various bytecode samples
    let bytecode_samples = vec![
        ("simple", vec![0x11, 0x12, 0x93, 0x41]), // PUSH1, PUSH2, ADD, RET
        ("medium", create_medium_bytecode()),
        ("complex", create_complex_bytecode()),
    ];

    for (name, bytecode) in bytecode_samples {
        group.throughput(Throughput::Bytes(bytecode.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("disassemble", name),
            &bytecode,
            |b, code| {
                b.iter(|| {
                    let _ = disassembler.disassemble(code).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark IR lifting performance
fn bench_ir_lifting(c: &mut Criterion) {
    let mut group = c.benchmark_group("ir_lifting");

    let config = DecompilerConfig::default();
    let disassembler = Disassembler::new(&config);
    let lifter = IRLifter::new(&config);

    let bytecode_samples = vec![
        ("simple", vec![0x11, 0x12, 0x93, 0x41]),
        ("medium", create_medium_bytecode()),
        ("complex", create_complex_bytecode()),
    ];

    for (name, bytecode) in bytecode_samples {
        let instructions = disassembler.disassemble(&bytecode).unwrap();

        group.throughput(Throughput::Elements(instructions.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("lift", name),
            &instructions,
            |b, instrs| {
                b.iter(|| {
                    let _ = lifter.lift_to_ir(instrs).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark complete decompilation pipeline
fn bench_end_to_end_decompilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    let test_cases = vec![
        ("minimal_contract", SampleNefData::minimal(), None),
        (
            "simple_with_manifest",
            SampleNefData::minimal(),
            Some(SampleManifest::simple_contract()),
        ),
        (
            "nep17_contract",
            SampleNefData::with_control_flow(),
            Some(SampleManifest::nep17_token()),
        ),
    ];

    for (name, sample_nef, sample_manifest) in test_cases {
        let nef_bytes = sample_nef.to_bytes();
        let manifest_json = sample_manifest.map(|m| m.to_json());

        let config = DecompilerConfig::default();

        group.throughput(Throughput::Bytes(nef_bytes.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("decompile", name),
            &(nef_bytes, manifest_json),
            |b, (nef_data, manifest)| {
                b.iter(|| {
                    let mut decompiler = Decompiler::new(config.clone());
                    let _ = decompiler.decompile(nef_data, manifest.as_deref()).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark pseudocode generation
fn bench_pseudocode_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("pseudocode_generation");

    let config = DecompilerConfig::default();
    let disassembler = Disassembler::new(&config);
    let lifter = IRLifter::new(&config);
    let generator = PseudocodeGenerator::new(&config);

    // Pre-create IR functions for benchmarking
    let test_cases = vec![
        ("simple", vec![0x11, 0x12, 0x93, 0x41]),
        ("medium", create_medium_bytecode()),
        ("complex", create_complex_bytecode()),
    ];

    for (name, bytecode) in test_cases {
        let instructions = disassembler.disassemble(&bytecode).unwrap();
        let ir_function = lifter.lift_to_ir(&instructions).unwrap();

        group.bench_with_input(BenchmarkId::new("generate", name), &ir_function, |b, ir| {
            b.iter(|| {
                let _ = generator.generate(ir).unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark configuration loading and validation
fn bench_configuration(c: &mut Criterion) {
    let mut group = c.benchmark_group("configuration");

    let config = DecompilerConfig::default();
    let toml_str = toml::to_string(&config).unwrap();
    let json_str = serde_json::to_string(&config).unwrap();

    group.bench_function("create_default", |b| {
        b.iter(|| DecompilerConfig::default());
    });

    group.bench_function("serialize_toml", |b| {
        b.iter(|| toml::to_string(&config).unwrap());
    });

    group.bench_function("deserialize_toml", |b| {
        b.iter(|| {
            let _: DecompilerConfig = toml::from_str(&toml_str).unwrap();
        });
    });

    group.bench_function("serialize_json", |b| {
        b.iter(|| serde_json::to_string(&config).unwrap());
    });

    group.bench_function("deserialize_json", |b| {
        b.iter(|| {
            let _: DecompilerConfig = serde_json::from_str(&json_str).unwrap();
        });
    });

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");

    // Create progressively larger contracts
    let large_bytecode_samples = vec![
        ("small_100", create_bytecode_of_size(100)),
        ("medium_1k", create_bytecode_of_size(1000)),
        ("large_10k", create_bytecode_of_size(10000)),
    ];

    for (name, bytecode) in large_bytecode_samples {
        let config = DecompilerConfig::default();

        group.throughput(Throughput::Bytes(bytecode.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("full_pipeline", name),
            &bytecode,
            |b, code| {
                b.iter(|| {
                    // Simulate full decompilation pipeline
                    let parser = NEFParser::new();
                    let disassembler = Disassembler::new(&config);
                    let lifter = IRLifter::new(&config);
                    let generator = PseudocodeGenerator::new(&config);

                    // Create minimal NEF with this bytecode
                    let mut sample = SampleNefData::minimal();
                    sample.bytecode = code.clone();
                    let nef_bytes = sample.to_bytes();

                    let nef_file = parser.parse(&nef_bytes).unwrap();
                    let instructions = disassembler.disassemble(&nef_file.bytecode).unwrap();
                    let ir_function = lifter.lift_to_ir(&instructions).unwrap();
                    let _ = generator.generate(&ir_function).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent processing
fn bench_concurrent_decompilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");

    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();

    // Benchmark sequential vs concurrent processing
    group.bench_function("sequential_3_contracts", |b| {
        b.iter(|| {
            for _ in 0..3 {
                let config = DecompilerConfig::default();
                let mut decompiler = Decompiler::new(config);
                let _ = decompiler.decompile(&nef_bytes, None).unwrap();
            }
        });
    });

    group.bench_function("concurrent_3_contracts", |b| {
        b.iter(|| {
            use std::sync::Arc;
            use std::thread;

            let nef_data = Arc::new(nef_bytes.clone());
            let handles: Vec<_> = (0..3)
                .map(|_| {
                    let data = Arc::clone(&nef_data);
                    thread::spawn(move || {
                        let config = DecompilerConfig::default();
                        let mut decompiler = Decompiler::new(config);
                        decompiler.decompile(&data, None).unwrap()
                    })
                })
                .collect();

            for handle in handles {
                let _ = handle.join().unwrap();
            }
        });
    });

    group.finish();
}

// Helper functions to create test bytecode

fn create_medium_bytecode() -> Vec<u8> {
    let mut bytecode = Vec::new();

    // Create a sequence with loops and conditionals
    bytecode.extend_from_slice(&[
        0x10, 0x11, 0x12, 0x13, 0x14, // PUSH0 through PUSH4
        0x93, 0x94, 0x95, // ADD, SUB, MUL
        0x15, 0x9F, // PUSH3, GT
        0x2C, 0x05, // JMP_IF 5 bytes ahead
        0x0C, 0x04, 0x54, 0x65, 0x73, 0x74, // PUSHDATA1 "Test"
        0x2B, 0x03, // JMP 3 bytes ahead
        0x0C, 0x04, 0x46, 0x61, 0x69, 0x6C, // PUSHDATA1 "Fail"
        0x8A, // SIZE
        0x62, 0x7D, 0xF6, 0xE2, // SYSCALL System.Runtime.Log
        0x41, // RET
    ]);

    // Repeat pattern to make it larger
    let pattern = bytecode.clone();
    for _ in 1..5 {
        bytecode.extend_from_slice(&pattern);
    }

    bytecode
}

fn create_complex_bytecode() -> Vec<u8> {
    let mut bytecode = Vec::new();

    // Create a complex contract with multiple functions and control flow
    bytecode.extend_from_slice(&[
        // Main entry point
        0x0C, 0x04, 0x6D, 0x61, 0x69, 0x6E, // PUSHDATA1 "main"
        0x10, // PUSH0
        0x15, // PUSH5
        // Loop structure
        0x6B, // DUP
        0x11, // PUSH1
        0x94, // SUB
        0x6B, // DUP
        0x10, // PUSH0
        0x9F, // GT
        0x2C, 0x0F, // JMP_IF 15 bytes ahead
        // Loop body
        0x6B, // DUP
        0x0C, 0x05, 0x49, 0x74, 0x65, 0x6D, 0x00, // PUSHDATA1 "Item"
        0x8C, // CAT
        0x62, 0x7D, 0xF6, 0xE2, // SYSCALL System.Runtime.Log
        0x2B, 0xF0, // JMP back (-16 bytes)
        // Exit loop
        0x75, // DROP
        // Function call simulation
        0x10, 0x11, 0x12, // PUSH0, PUSH1, PUSH2
        0x0C, 0x08, 0x73, 0x75, 0x62, 0x66, 0x75, 0x6E, 0x63, 0x74, // "subfunct"
        0x14, // PUSH4
        0x8A, // SIZE
        // Conditional execution
        0x13, // PUSH3
        0x9F, // GT
        0x2C, 0x08, // JMP_IF 8 bytes
        0x0C, 0x05, 0x45, 0x72, 0x72, 0x6F, 0x72, // PUSHDATA1 "Error"
        0x3A, // THROW
        // Success path
        0x0C, 0x07, 0x53, 0x75, 0x63, 0x63, 0x65, 0x73, 0x73, // "Success"
        0x41, // RET
    ]);

    // Add more complexity by repeating and varying the pattern
    let base_pattern = bytecode.clone();
    for i in 1..10 {
        let mut variant = base_pattern.clone();
        // Modify some bytes to create variants
        if variant.len() > 10 {
            variant[5] = (variant[5] as u8).wrapping_add(i as u8);
            variant[10] = (variant[10] as u8).wrapping_add((i * 2) as u8);
        }
        bytecode.extend_from_slice(&variant);
    }

    bytecode
}

fn create_bytecode_of_size(target_size: usize) -> Vec<u8> {
    let mut bytecode = Vec::new();
    let base_pattern = vec![
        0x10, 0x11, 0x93, 0x12, 0x94, 0x13, 0x95, 0x41, // Basic arithmetic + RET
    ];

    while bytecode.len() < target_size {
        bytecode.extend_from_slice(&base_pattern);

        // Add some variation
        if bytecode.len() % 100 == 0 {
            bytecode.extend_from_slice(&[
                0x0C, 0x04, 0x54, 0x65, 0x73, 0x74, // PUSHDATA1 "Test"
                0x8A, // SIZE
                0x75, // DROP
            ]);
        }
    }

    // Trim to exact size
    bytecode.truncate(target_size);

    // Ensure it ends properly
    if !bytecode.is_empty() {
        bytecode[bytecode.len() - 1] = 0x41; // RET
    }

    bytecode
}

// Configure criterion groups
criterion_group!(
    benches,
    bench_nef_parsing,
    bench_manifest_parsing,
    bench_disassembly,
    bench_ir_lifting,
    bench_end_to_end_decompilation,
    bench_pseudocode_generation,
    bench_configuration,
    bench_memory_usage,
    bench_concurrent_decompilation
);

criterion_main!(benches);
