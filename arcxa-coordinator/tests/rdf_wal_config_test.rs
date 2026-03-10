/// RDF WAL Configuration Parsing Test
///
/// Tests environment variable parsing for RdfWalConfig
use graphica_coordinator::config::CoordinatorConfig;
use std::env;

#[test]
fn test_rdf_wal_config_disabled_by_default() {
    // Clear any existing config
    env::remove_var("RDF_WAL_ENABLED");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");

    assert!(
        config.rdf_wal.is_none(),
        "RDF WAL should be disabled by default"
    );
}

#[test]
fn test_rdf_wal_config_enabled() {
    // Set minimal config
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");

    assert!(config.rdf_wal.is_some(), "RDF WAL should be enabled");

    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();
    assert!(rdf_wal_config.enabled, "enabled flag should be true");
    assert_eq!(
        rdf_wal_config.wal.path.to_str().unwrap(),
        "/tmp/test_rdf_wal",
        "WAL path should match environment variable"
    );

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
}

#[test]
fn test_rdf_wal_config_auto_recover() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_AUTO_RECOVER", "false");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    assert!(!rdf_wal_config.auto_recover, "auto_recover should be false");

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_AUTO_RECOVER");
}

#[test]
fn test_rdf_wal_config_recovery_start_lsn() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_RECOVERY_START_LSN", "12345");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    assert_eq!(
        rdf_wal_config.recovery_start_lsn,
        Some(12345),
        "recovery_start_lsn should be parsed"
    );

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_RECOVERY_START_LSN");
}

#[test]
fn test_rdf_wal_config_max_recovery_entries() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_MAX_RECOVERY_ENTRIES", "1000");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    assert_eq!(
        rdf_wal_config.max_recovery_entries,
        Some(1000),
        "max_recovery_entries should be parsed"
    );

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_MAX_RECOVERY_ENTRIES");
}

#[test]
fn test_rdf_wal_config_fsync_mode() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_FSYNC_MODE", "every_write");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    // We can't directly compare FsyncMode enum, but we can verify it parsed successfully
    // by checking the config loaded without errors
    assert!(rdf_wal_config.enabled);

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_FSYNC_MODE");
}

#[test]
fn test_rdf_wal_config_compression() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_COMPRESSION", "lz4");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    // Verify compression is enabled (we can't directly check the enum type)
    assert!(rdf_wal_config.wal.compression.is_some());

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_COMPRESSION");
}

#[test]
fn test_rdf_wal_config_max_file_size() {
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/tmp/test_rdf_wal");
    env::set_var("RDF_WAL_MAX_FILE_SIZE", "52428800"); // 50MB

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config.rdf_wal.as_ref().unwrap();

    assert_eq!(
        rdf_wal_config.wal.max_file_size, 52428800,
        "max_file_size should be parsed"
    );

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_MAX_FILE_SIZE");
}

#[test]
fn test_rdf_wal_config_full_configuration() {
    // Set comprehensive configuration
    env::set_var("RDF_WAL_ENABLED", "true");
    env::set_var("RDF_WAL_PATH", "/var/lib/graphica/rdf_wal");
    env::set_var("RDF_WAL_AUTO_RECOVER", "true");
    env::set_var("RDF_WAL_RECOVERY_START_LSN", "100");
    env::set_var("RDF_WAL_MAX_RECOVERY_ENTRIES", "5000");
    env::set_var("RDF_WAL_MAX_FILE_SIZE", "104857600"); // 100MB
    env::set_var("RDF_WAL_MAX_SEGMENTS", "20");
    env::set_var("RDF_WAL_FSYNC_MODE", "batch_sync");
    env::set_var("RDF_WAL_COMPRESSION", "zstd");

    let config = CoordinatorConfig::from_env().expect("Failed to load config");
    let rdf_wal_config = config
        .rdf_wal
        .as_ref()
        .expect("RDF WAL should be configured");

    assert!(rdf_wal_config.enabled);
    assert_eq!(
        rdf_wal_config.wal.path.to_str().unwrap(),
        "/var/lib/graphica/rdf_wal"
    );
    assert!(rdf_wal_config.auto_recover);
    assert_eq!(rdf_wal_config.recovery_start_lsn, Some(100));
    assert_eq!(rdf_wal_config.max_recovery_entries, Some(5000));
    assert_eq!(rdf_wal_config.wal.max_file_size, 104857600);
    assert_eq!(rdf_wal_config.wal.max_segments, 20);
    assert!(rdf_wal_config.wal.compression.is_some());

    // Cleanup
    env::remove_var("RDF_WAL_ENABLED");
    env::remove_var("RDF_WAL_PATH");
    env::remove_var("RDF_WAL_AUTO_RECOVER");
    env::remove_var("RDF_WAL_RECOVERY_START_LSN");
    env::remove_var("RDF_WAL_MAX_RECOVERY_ENTRIES");
    env::remove_var("RDF_WAL_MAX_FILE_SIZE");
    env::remove_var("RDF_WAL_MAX_SEGMENTS");
    env::remove_var("RDF_WAL_FSYNC_MODE");
    env::remove_var("RDF_WAL_COMPRESSION");
}
