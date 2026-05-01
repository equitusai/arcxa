fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files for graphica-core
    // Generate file descriptor set for reflection support
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Compile graphica-core's own proto files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("graphica_descriptor.bin"))
        .compile(
            &[
                "proto/lineage.proto",
                "proto/quality.proto",
                "proto/shard_service.proto",
            ],
            &["proto"],
        )?;

    println!("cargo:rerun-if-changed=proto/lineage.proto");
    println!("cargo:rerun-if-changed=proto/quality.proto");
    println!("cargo:rerun-if-changed=proto/shard_service.proto");
    println!("cargo:rerun-if-changed=proto");

    // Compile shared coordinator_service proto for shard auto-registration
    // This allows shards to connect to the coordinator
    // Use CARGO_MANIFEST_DIR to build absolute path (works in CI/CD)
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let shared_proto_dir = manifest_dir.parent().unwrap().join("proto");

    if !shared_proto_dir.exists() {
        panic!(
            "Shared proto directory not found at: {:?}",
            shared_proto_dir
        );
    }

    let coordinator_proto = shared_proto_dir.join("coordinator_service.proto");
    if !coordinator_proto.exists() {
        panic!(
            "coordinator_service.proto not found at: {:?}",
            coordinator_proto
        );
    }

    tonic_build::configure()
        .build_server(false) // Shards only need client
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("coordinator_descriptor.bin"))
        .compile(
            &[coordinator_proto.to_str().unwrap()],
            &[shared_proto_dir.to_str().unwrap()],
        )?;

    println!("cargo:rerun-if-changed={}", coordinator_proto.display());

    // Compile shared prediction_service proto for ML model invocation
    let migration_evidence_proto = shared_proto_dir.join("migration_evidence_service.proto");
    if migration_evidence_proto.exists() {
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .file_descriptor_set_path(out_dir.join("migration_evidence_descriptor.bin"))
            .compile(
                &[migration_evidence_proto.to_str().unwrap()],
                &[shared_proto_dir.to_str().unwrap()],
            )?;

        println!("cargo:rerun-if-changed={}", migration_evidence_proto.display());
    }

    let prediction_proto = shared_proto_dir.join("prediction_service.proto");
    if prediction_proto.exists() {
        tonic_build::configure()
            .build_server(false) // Core only needs client for external models
            .build_client(true)
            .file_descriptor_set_path(out_dir.join("prediction_descriptor.bin"))
            .compile(
                &[prediction_proto.to_str().unwrap()],
                &[shared_proto_dir.to_str().unwrap()],
            )?;

        println!("cargo:rerun-if-changed={}", prediction_proto.display());
    }

    Ok(())
}
