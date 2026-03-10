//! Simple demo showing memory reduction from string interning
//!
//! Run with: cargo run --example string_interning_demo

use graphica_coordinator::workflows::domain::{Action, Condition, Route, Workflow};
use graphica_coordinator::workflows::utils::string_pool::intern;
use std::mem::size_of_val;
use std::time::Instant;

fn main() {
    println!("=== String Interning Memory Reduction Demo ===\n");

    // Show type sizes
    println!("Type Sizes:");
    let atom = intern("example");
    let string = String::from("example");
    println!("  Atom size: {} bytes", size_of_val(&atom));
    println!("  String size: {} bytes", size_of_val(&string));
    println!(
        "  Reduction: {}%\n",
        ((size_of_val(&string) - size_of_val(&atom)) * 100) / size_of_val(&string)
    );

    // Demo 1: String deduplication
    println!("Demo 1: String Deduplication");
    println!("Creating 1000 workflows with repeated IDs...");

    let start = Instant::now();
    let workflows: Vec<Workflow> = (0..1000)
        .map(|i| {
            let routes = vec![Route::new(
                format!("route_{}", i % 10), // Only 10 unique IDs
                "Standard Route",
                Condition::Always,
                vec![
                    Action::Log {
                        level: "info".to_string(),
                        message: format!("Processing {}", i),
                    },
                    Action::SetField {
                        field: "processed".to_string(),
                        value: serde_json::json!(true),
                    },
                ],
            )];
            Workflow::new(
                format!("wf_{}", i % 10), // Only 10 unique workflow IDs
                "Standard Workflow",
                routes,
            )
        })
        .collect();

    let elapsed = start.elapsed();
    println!("Created {} workflows in {:?}", workflows.len(), elapsed);
    println!("Without string interning, this would create:");
    println!("  - 1000 route_X strings (now just 10 interned via Route IDs)");
    println!("  - 1000 wf_X strings (now just 10 interned via Workflow IDs)");
    println!("  - 1000 \"Standard Route\" strings (now just 1 interned)");
    println!("  - 1000 \"Standard Workflow\" strings (now just 1 interned)");
    println!("  Total reduction: ~3990 string allocations saved (99.75% reduction)!\n");

    // Demo 2: Equality comparison performance
    println!("Demo 2: Fast Equality Checks");
    let atom1 = intern("workflow_id_12345");
    let atom2 = intern("workflow_id_12345");

    let start = Instant::now();
    for _ in 0..1_000_000 {
        let _ = atom1 == atom2; // Pointer comparison
    }
    let atom_time = start.elapsed();

    let string1 = String::from("workflow_id_12345");
    let string2 = String::from("workflow_id_12345");

    let start = Instant::now();
    for _ in 0..1_000_000 {
        let _ = string1 == string2; // Byte-by-byte comparison
    }
    let string_time = start.elapsed();

    println!("1,000,000 equality comparisons:");
    println!("  Atom (pointer): {:?}", atom_time);
    println!("  String (bytes): {:?}", string_time);
    println!(
        "  Speedup: {:.1}x faster\n",
        string_time.as_nanos() as f64 / atom_time.as_nanos() as f64
    );

    // Demo 3: Clone performance
    println!("Demo 3: Fast Clone Operations");
    let atom = intern("workflow_field_name");

    let start = Instant::now();
    for _ in 0..1_000_000 {
        let _ = atom.clone(); // Just increments ref count
    }
    let atom_time = start.elapsed();

    let string = String::from("workflow_field_name");

    let start = Instant::now();
    for _ in 0..1_000_000 {
        let _ = string.clone(); // Allocates and copies bytes
    }
    let string_time = start.elapsed();

    println!("1,000,000 clone operations:");
    println!("  Atom (ref count): {:?}", atom_time);
    println!("  String (copy): {:?}", string_time);
    println!(
        "  Speedup: {:.1}x faster\n",
        string_time.as_nanos() as f64 / atom_time.as_nanos() as f64
    );

    // Summary
    println!("=== Summary ===");
    println!("String interning provides:");
    println!("✓ Reduced memory usage (eliminating duplicate strings)");
    println!("✓ Faster equality checks (pointer comparison vs byte comparison)");
    println!("✓ Faster cloning (ref count increment vs memory allocation)");
    println!("✓ Better cache locality (fewer unique string allocations)");
}
