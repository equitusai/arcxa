fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false) // Model service is server only
        .compile(&["../proto/model_service.proto"], &["../proto"])?;
    Ok(())
}
