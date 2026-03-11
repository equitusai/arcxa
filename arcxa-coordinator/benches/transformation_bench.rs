//! Performance benchmarks for the transformation engine

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use graphica_coordinator::mapping::loader::transformation::*;
use std::collections::HashMap;

/// Create test data for benchmarking
fn create_test_data(size: usize) -> Vec<HashMap<String, String>> {
    (0..size)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("first_name".to_string(), format!("  John{:04}  ", i));
            row.insert("last_name".to_string(), format!("  Doe{:04}  ", i));
            row.insert("email".to_string(), format!("JOHN{}@EXAMPLE.COM", i));
            row.insert("age".to_string(), (20 + i % 50).to_string());
            row.insert(
                "salary".to_string(),
                format!("{:.2}", 50000.0 + (i as f64 * 1000.0)),
            );
            row.insert("date".to_string(), "2024-01-15".to_string());
            row
        })
        .collect()
}

/// Benchmark simple transformations
fn bench_simple_transformations(c: &mut Criterion) {
    let engine = TransformationEngine::new();
    let test_data = create_test_data(1);
    let row = &test_data[0];

    let mut group = c.benchmark_group("simple_transformations");

    // UPPER transformation
    group.bench_function("upper", |b| {
        b.iter(|| engine.execute(black_box("UPPER({first_name})"), black_box(row)))
    });

    // TRIM transformation
    group.bench_function("trim", |b| {
        b.iter(|| engine.execute(black_box("TRIM({first_name})"), black_box(row)))
    });

    // Nested transformation
    group.bench_function("upper_trim", |b| {
        b.iter(|| engine.execute(black_box("UPPER(TRIM({first_name}))"), black_box(row)))
    });

    group.finish();
}

/// Benchmark complex transformations
fn bench_complex_transformations(c: &mut Criterion) {
    let engine = TransformationEngine::new();
    let test_data = create_test_data(1);
    let row = &test_data[0];

    let mut group = c.benchmark_group("complex_transformations");

    // CONCAT with multiple fields
    group.bench_function("concat", |b| {
        b.iter(|| {
            engine.execute(
                black_box("CONCAT(TRIM({first_name}), ' ', TRIM({last_name}))"),
                black_box(row),
            )
        })
    });

    // CASE expression
    group.bench_function("case", |b| {
        b.iter(|| {
            engine.execute(
                black_box("CASE WHEN {age} < '30' THEN 'Young' WHEN {age} < '50' THEN 'Middle' ELSE 'Senior' END"),
                black_box(row),
            )
        })
    });

    // Complex nested expression
    group.bench_function("complex_nested", |b| {
        b.iter(|| {
            engine.execute(
                black_box("UPPER(CONCAT(TRIM({first_name}), ' ', TRIM({last_name}), ' - ', LOWER({email})))"),
                black_box(row),
            )
        })
    });

    group.finish();
}

/// Benchmark batch processing
fn bench_batch_processing(c: &mut Criterion) {
    let engine = TransformationEngine::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("batch_processing");

    for size in [100, 1000, 10000] {
        let test_data = create_test_data(size);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &test_data,
            |b, data| {
                b.iter(|| {
                    for row in data {
                        let _ = engine.execute("UPPER(TRIM({first_name}))", black_box(row));
                    }
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("parallel", size), &test_data, |b, data| {
            b.to_async(&runtime).iter(|| async {
                engine
                    .execute_batch("UPPER(TRIM({first_name}))", black_box(data.clone()))
                    .await
            })
        });
    }

    group.finish();
}

/// Benchmark type conversions
fn bench_type_conversions(c: &mut Criterion) {
    let engine = TransformationEngine::new();
    let test_data = create_test_data(1);
    let row = &test_data[0];

    let mut group = c.benchmark_group("type_conversions");

    // CAST to INTEGER
    group.bench_function("cast_integer", |b| {
        b.iter(|| engine.execute(black_box("CAST({age} AS INTEGER)"), black_box(row)))
    });

    // CAST to FLOAT
    group.bench_function("cast_float", |b| {
        b.iter(|| engine.execute(black_box("CAST({salary} AS FLOAT)"), black_box(row)))
    });

    // CAST to DATE
    group.bench_function("cast_date", |b| {
        b.iter(|| engine.execute(black_box("CAST({date} AS DATE)"), black_box(row)))
    });

    group.finish();
}

/// Benchmark parser performance
fn bench_parser(c: &mut Criterion) {
    let parser = ExpressionParser::new();

    let mut group = c.benchmark_group("parser");

    // Simple expression
    group.bench_function("simple", |b| {
        b.iter(|| parser.parse(black_box("UPPER({field})")))
    });

    // Nested expression
    group.bench_function("nested", |b| {
        b.iter(|| {
            parser.parse(black_box(
                "UPPER(TRIM(COALESCE({field1}, {field2}, 'default')))",
            ))
        })
    });

    // Complex expression with CASE
    group.bench_function("complex", |b| {
        b.iter(|| {
            parser.parse(black_box(
                "CASE WHEN {age} < 30 THEN CONCAT('Young: ', UPPER({name})) \
                 WHEN {age} < 50 THEN CONCAT('Middle: ', {name}) \
                 ELSE CONCAT('Senior: ', LOWER({name})) END",
            ))
        })
    });

    group.finish();
}

/// Benchmark plan cache effectiveness
fn bench_plan_cache(c: &mut Criterion) {
    let engine = TransformationEngine::new();
    let test_data = create_test_data(1);
    let row = &test_data[0];

    let mut group = c.benchmark_group("plan_cache");

    // First execution (cache miss)
    let expression = "UPPER(TRIM(CONCAT({first_name}, ' ', {last_name})))";

    // Warm up cache
    let _ = engine.execute(expression, row);

    // Benchmark cached execution
    group.bench_function("cached", |b| {
        b.iter(|| engine.execute(black_box(expression), black_box(row)))
    });

    // Benchmark uncached execution with different expressions each time
    group.bench_function("uncached", |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let unique_expr = format!(
                "UPPER(TRIM(CONCAT({{first_name}}, ' {}', {{last_name}})))",
                counter
            );
            engine.execute(black_box(&unique_expr), black_box(row))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_transformations,
    bench_complex_transformations,
    bench_batch_processing,
    bench_type_conversions,
    bench_parser,
    bench_plan_cache
);
criterion_main!(benches);
