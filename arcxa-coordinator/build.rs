// Build script for compiling protobuf definitions
//
// This compiles both client and server gRPC services:
// - Client: model_service, workflow_state (coordinator connects TO these)
// - Server: coordinator_service (shards connect TO coordinator)

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = PathBuf::from("../proto");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    // ========================================================================
    // CLIENT SERVICES (Coordinator connects TO external services)
    // ========================================================================

    // Model Service - ML model inference (coordinator -> model service)
    compile_client_proto(&proto_dir, "model_service.proto", "model_service")?;

    // Workflow State Service - Shard persistence (coordinator -> shard)
    compile_client_proto(&proto_dir, "workflow_state.proto", "workflow_state")?;

    // ========================================================================
    // SERVER SERVICES (External services connect TO coordinator)
    // ========================================================================

    // Coordinator Service - Shard auto-registration and management
    // Shards connect to coordinator for registration, heartbeat, deregistration
    compile_server_proto(
        &proto_dir,
        &out_dir,
        "coordinator_service.proto",
        "coordinator_service",
    )?;

    Ok(())
}

/// Compile a proto file as client-only (coordinator connects to this service)
fn compile_client_proto(
    proto_dir: &PathBuf,
    proto_file: &str,
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = proto_dir.join(proto_file);

    println!("cargo:rerun-if-changed={}", proto_path.display());

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile(
            &[proto_path.to_str().unwrap()],
            &[proto_dir.to_str().unwrap()],
        )
        .map_err(|e| {
            format!(
                "Failed to compile {} proto ({}): {}",
                service_name, proto_file, e
            )
        })?;

    eprintln!("✓ Compiled {} client proto", service_name);

    Ok(())
}

/// Compile a proto file as server (external services connect to coordinator)
///
/// Includes file descriptor set for reflection support (useful for debugging
/// with tools like grpcurl and for runtime introspection).
fn compile_server_proto(
    proto_dir: &PathBuf,
    out_dir: &PathBuf,
    proto_file: &str,
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = proto_dir.join(proto_file);
    let descriptor_path = out_dir.join(format!("{}_descriptor.bin", service_name));

    println!("cargo:rerun-if-changed={}", proto_path.display());

    tonic_build::configure()
        .build_server(true)
        .build_client(true) // Also build client for testing
        .file_descriptor_set_path(&descriptor_path)
        .compile(
            &[proto_path.to_str().unwrap()],
            &[proto_dir.to_str().unwrap()],
        )
        .map_err(|e| {
            format!(
                "Failed to compile {} proto ({}): {}",
                service_name, proto_file, e
            )
        })?;

    eprintln!(
        "✓ Compiled {} server proto (descriptor: {})",
        service_name,
        descriptor_path.display()
    );

    Ok(())
}
