//! P12-1 bench: dsh_format 缓存 vs 不缓存
//!
//! 跑法: `cargo bench --package ma-harness-conformance --bench dsh_cache`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ma_harness_conformance::dsh_format::{parse_dsh_jsonl, DshFixtureCache};
use std::io::Write;

fn generate_fixture_jsonl(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        writeln!(
            s,
            r#"{{"name":"f{i}","input":{{"session_id":"s{i}","messages":[{{"role":"user","content":"hello {i}"}}],"events":[]}},"expected_output":{{"events":[{{"type":"UserInput","data":{{"content":"hello {i}"}}}}],"messages":[]}}}}"#
        )
        .unwrap();
    }
    s
}

fn bench_parse_no_cache(c: &mut Criterion) {
    let jsonl = generate_fixture_jsonl(100);
    c.bench_function("parse_dsh_jsonl 100 fixtures (no cache)", |b| {
        b.iter(|| {
            let fixtures = parse_dsh_jsonl(black_box(&jsonl)).unwrap();
            black_box(fixtures);
        });
    });
}

fn bench_parse_with_cache(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bench.jsonl");
    std::fs::write(&path, generate_fixture_jsonl(100)).unwrap();
    let cache = DshFixtureCache::new();

    c.bench_function("DshFixtureCache::from_jsonl_cached 100 fixtures (cache hit)", |b| {
        b.iter(|| {
            let fixtures = cache.from_jsonl_cached(black_box(&path)).unwrap();
            black_box(fixtures);
        });
    });
}

criterion_group!(benches, bench_parse_no_cache, bench_parse_with_cache);
criterion_main!(benches);
