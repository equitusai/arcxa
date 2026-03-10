//! # Storage Migration Tool
//!
//! Migrates RocksDB lineage storage from old index format to inverted indexes.
//!
//! Usage:
//!   cargo run --bin migrate -- /path/to/rocksdb
//!   cargo run --bin migrate -- --check /path/to/rocksdb

use anyhow::Result;
use clap::{Parser, Subcommand};
use graphica_migrations::{get_migration_status, migrate_database, MigrationStatus};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(name = "migrate")]
#[command(about = "Migrate Graphica lineage storage to inverted index format", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check migration status without migrating
    Check {
        /// Path to RocksDB database
        db_path: String,
    },
    /// Run migration
    Migrate {
        /// Path to RocksDB database
        db_path: String,

        /// Dry run - show what would be migrated without actually migrating
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    // Initialize logging
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check { db_path } => {
            println!("Checking migration status: {}", db_path);

            // Open database to check status
            let mut opts = rocksdb::Options::default();
            opts.create_if_missing(false);

            let cfs = vec![
                "primary",
                "record_idx",
                "model_idx",
                "run_idx",
                "tenant_idx",
                "time_idx",
                "time_travel_idx",
            ];

            match rocksdb::DB::open_cf_for_read_only(&opts, &db_path, cfs, false) {
                Ok(db) => {
                    let status = get_migration_status(&db)?;

                    match status {
                        MigrationStatus::OldFormat => {
                            println!("❌ Database uses OLD index format");
                            println!("   Run: cargo run --bin migrate -- migrate {}", db_path);
                        }
                        MigrationStatus::Migrating => {
                            println!("⚠️  Database migration is IN PROGRESS");
                            println!(
                                "   Run: cargo run --bin migrate -- migrate {} to complete",
                                db_path
                            );
                        }
                        MigrationStatus::Complete => {
                            println!("✅ Database migration is COMPLETE");
                            println!("   Using inverted index format (write amplification: 4×)");
                        }
                    }

                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Failed to open database: {}", e);
                    eprintln!("   Make sure the path is correct and database is not in use");
                    Err(e.into())
                }
            }
        }

        Commands::Migrate { db_path, dry_run } => {
            if dry_run {
                println!("🔍 DRY RUN - No changes will be made");
            }

            println!("Migrating database: {}", db_path);
            println!("");
            println!("This will:");
            println!("  1. Convert indexes from: key → Vec<event_id>");
            println!("  2. To inverted format: (key, event_id) → empty");
            println!("  3. Reduce write amplification from 7× to 4×");
            println!("");

            if dry_run {
                println!("DRY RUN: Exiting without making changes");
                return Ok(());
            }

            // Run migration
            migrate_database(&db_path)?;

            println!("");
            println!("✅ Migration completed successfully!");
            println!("");
            println!("Next steps:");
            println!("  1. Restart Graphica application");
            println!("  2. Monitor metrics: graphica_rocksdb_write_amplification_ratio");
            println!("  3. Expected: 3-4× throughput improvement");

            Ok(())
        }
    }
}
