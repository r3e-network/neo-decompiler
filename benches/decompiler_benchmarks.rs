//! Performance benchmarks for the Neo N3 decompiler
//!
//! Run with: cargo bench

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use neo_decompiler::*;
use std::time::Duration;

// Simple NEF data for benchmarking
struct BenchmarkNef {
    magic: [u8; 4],
    compiler: [u8; 64],
    source_url: String,
    tokens: Vec<u8>,
    bytecode: Vec<u8>,
    checksum: u32,
}

impl BenchmarkNef {
    fn minimal() -> Self {
        Self {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"test-compiler-v1.0";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: "https://example.com/test".to_string(),
            tokens: vec![],
            bytecode: vec![
                0x11, 0x12, 0x93, 0x41, // PUSH1, PUSH2, ADD, RET
            ],
            checksum: 0x12345678,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut nef_data = Vec::new();
        nef_data.extend_from_slice(&self.magic);
        nef_data.extend_from_slice(&self.compiler);

        let source_bytes = self.source_url.as_bytes();
        nef_data.extend_from_slice(&(source_bytes.len() as u16).to_le_bytes());
        nef_data.extend_from_slice(source_bytes);

        nef_data.push(0); // Reserved

        nef_data.extend_from_slice(&(self.tokens.len() as u16).to_le_bytes());
        nef_data.extend_from_slice(&self.tokens);

        nef_data.extend_from_slice(&[0, 0]); // Reserved

        nef_data.extend_from_slice(&(self.bytecode.len() as u32).to_le_bytes());
        nef_data.extend_from_slice(&self.bytecode);

        nef_data.extend_from_slice(&self.checksum.to_le_bytes());

        nef_data
    }
}

/// Benchmark NEF parsing
fn bench_nef_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("nef_parsing");

    let sample = BenchmarkNef::minimal();
    let nef_bytes = sample.to_bytes();
    let parser = NEFParser::new();

    group.throughput(Throughput::Bytes(nef_bytes.len() as u64));
    group.bench_function("parse_minimal", |b| {
        b.iter(|| {
            let _ = parser.parse(&nef_bytes);
        });
    });

    group.finish();
}

/// Benchmark disassembly performance
fn bench_disassembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("disassembly");

    let config = DecompilerConfig::default();
    let disassembler = Disassembler::new(&config);

    let bytecode = vec![0x11, 0x12, 0x93, 0x41]; // Simple sequence

    group.throughput(Throughput::Bytes(bytecode.len() as u64));
    group.bench_function("disassemble_simple", |b| {
        b.iter(|| {
            let _ = disassembler.disassemble(&bytecode);
        });
    });

    group.finish();
}

/// Benchmark complete decompilation pipeline
fn bench_end_to_end_decompilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");
    group.measurement_time(Duration::from_secs(10));

    let sample = BenchmarkNef::minimal();
    let nef_bytes = sample.to_bytes();
    let config = DecompilerConfig::default();

    group.throughput(Throughput::Bytes(nef_bytes.len() as u64));
    group.bench_function("decompile_minimal", |b| {
        b.iter(|| {
            let mut decompiler = Decompiler::new(config.clone());
            let _ = decompiler.decompile(&nef_bytes, None);
        });
    });

    group.finish();
}

/// Benchmark configuration serialization
fn bench_configuration(c: &mut Criterion) {
    let mut group = c.benchmark_group("configuration");

    let config = DecompilerConfig::default();

    group.bench_function("create_default", |b| {
        b.iter(|| DecompilerConfig::default());
    });

    group.bench_function("serialize_toml", |b| {
        b.iter(|| toml::to_string(&config));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_nef_parsing,
    bench_disassembly,
    bench_end_to_end_decompilation,
    bench_configuration
);
criterion_main!(benches);
