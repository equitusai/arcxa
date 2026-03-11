//! Benchmark: String Interning Performance and Memory Impact
//!
//! Measures the performance and memory improvements from string interning
//! in the workflow module.
//!
//! Run with: cargo bench --bench string_interning_benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use graphica_coordinator::workflows::domain::{Action, Condition, Route, Workflow};
use graphica_coordinator::workflows::utils::intern;
use std::time::Duration;

/// Benchmark: Creating workflows with repeated string values
///
/// This simulates a realistic workflow creation pattern where IDs and field names
/// are frequently repeated across routes and actions.
fn benchmark_workflow_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_creation");

    // Create workflows with varying numbers of routes
    for num_routes in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_routes),
            num_routes,
            |b, &num_routes| {
                b.iter(|| {
                    let routes: Vec<Route> = (0..num_routes)
                        .map(|i| {
                            Route::new(
                                format!("route_{}", i % 10), // Repeat route IDs (simulates common patterns)
                                format!("Route {}", i % 10),
                                Condition::equals("status", "active"), // Repeated field name
                                vec![
                                    Action::Log {
                                        level: intern("info"), // Repeated level
                                        message: format!("Processing record {}", i),
                                    },
                                    Action::SetField {
                                        field: intern("processed"), // Repeated field name
                                        value: serde_json::json!(true),
                                    },
                                ],
                            )
                        })
                        .collect();

                    black_box(Workflow::new("wf_benchmark", "Benchmark Workflow", routes))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: String interning vs regular String allocation
///
/// Compares the performance of creating interned strings vs regular strings
fn benchmark_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    // Benchmark interning the same string repeatedly
    group.bench_function("intern_repeated", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                black_box(intern("workflow_id"));
            }
        });
    });

    // Benchmark creating regular strings repeatedly
    group.bench_function("string_repeated", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                black_box("workflow_id".to_string());
            }
        });
    });

    // Benchmark cloning interned strings
    group.bench_function("clone_interned", |b| {
        let atom = intern("workflow_id");
        b.iter(|| {
            for _ in 0..1000 {
                black_box(atom.clone());
            }
        });
    });

    // Benchmark cloning regular strings
    group.bench_function("clone_string", |b| {
        let s = "workflow_id".to_string();
        b.iter(|| {
            for _ in 0..1000 {
                black_box(s.clone());
            }
        });
    });

    group.finish();
}

/// Benchmark: Equality comparison performance
///
/// Compares the performance of equality checks between Atoms vs Strings
fn benchmark_equality_checks(c: &mut Criterion) {
    let mut group = c.benchmark_group("equality_checks");

    // Atom equality (pointer comparison)
    group.bench_function("atom_equality", |b| {
        let atom1 = intern("workflow_id_12345");
        let atom2 = intern("workflow_id_12345");
        b.iter(|| {
            for _ in 0..10000 {
                black_box(atom1 == atom2);
            }
        });
    });

    // String equality (byte-by-byte comparison)
    group.bench_function("string_equality", |b| {
        let s1 = "workflow_id_12345".to_string();
        let s2 = "workflow_id_12345".to_string();
        b.iter(|| {
            for _ in 0..10000 {
                black_box(s1 == s2);
            }
        });
    });

    group.finish();
}

/// Benchmark: Route evaluation with interned strings
///
/// Measures the performance improvement in condition evaluation
fn benchmark_route_evaluation(c: &mut Criterion) {
    use graphica_coordinator::workflows::engine::ConditionEvaluator;
    use serde_json::json;

    let mut group = c.benchmark_group("route_evaluation");

    // Create a workflow with multiple routes
    let routes: Vec<Route> = (0..50)
        .map(|i| {
            Route::new(
                format!("route_{}", i),
                format!("Route {}", i),
                Condition::And(Box::new(vec![
                    Condition::equals("tier", "premium"),
                    Condition::equals("status", "active"),
                    Condition::GreaterThan {
                        field: "value".to_string(),
                        value: json!(100),
                    },
                ])),
                vec![Action::Log {
                    level: intern("info"),
                    message: "Matched route".to_string(),
                }],
            )
        })
        .collect();

    let workflow = Workflow::new("wf_eval", "Evaluation Benchmark", routes);

    group.bench_function("evaluate_50_routes", |b| {
        let input = json!({
            "tier": "premium",
            "status": "active",
            "value": 150
        });

        b.iter(|| {
            for route in &workflow.routes {
                black_box(ConditionEvaluator::evaluate(&route.condition, &input).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Memory footprint comparison
///
/// Creates many workflows to measure memory usage
fn benchmark_memory_footprint(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_footprint");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("create_1000_workflows", |b| {
        b.iter(|| {
            let workflows: Vec<Workflow> = (0..1000)
                .map(|i| {
                    let routes = vec![Route::new(
                        format!("route_{}", i % 100), // Repeat every 100
                        "Standard Route",
                        Condition::equals("type", "standard"),
                        vec![
                            Action::Log {
                                level: intern("info"),
                                message: format!("Processing {}", i),
                            },
                            Action::SetField {
                                field: intern("processed"),
                                value: serde_json::json!(true),
                            },
                        ],
                    )];
                    Workflow::new(
                        format!("wf_{}", i % 100), // Repeat every 100
                        "Standard Workflow",
                        routes,
                    )
                })
                .collect();

            black_box(workflows)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_workflow_creation,
    benchmark_string_operations,
    benchmark_equality_checks,
    benchmark_route_evaluation,
    benchmark_memory_footprint
);

criterion_main!(benches);
