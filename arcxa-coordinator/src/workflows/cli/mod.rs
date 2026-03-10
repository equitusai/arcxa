//! CLI for workflow management
//!
//! Provides GitOps-style workflow management commands:
//! - init: Create workflow templates
//! - validate: Validate workflow files
//! - test: Run workflow tests
//! - deploy: Deploy workflows
//! - list: List deployed workflows
//! - status: Check workflow status
//! - rollback: Rollback deployments

pub mod commands;
pub mod templates;

pub use commands::*;
pub use templates::*;

use clap::{Parser, Subcommand};

/// ARCXA workflow management CLI
#[derive(Parser, Debug)]
#[command(name = "arcxa-coordinator")]
#[command(about = "ARCXA workflow management", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available workflow commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Workflow management commands
    #[command(subcommand)]
    Workflow(WorkflowCommands),
}

/// Workflow subcommands
#[derive(Subcommand, Debug)]
pub enum WorkflowCommands {
    /// Create a new workflow from template
    Init {
        /// Workflow name
        name: String,

        /// Output file path
        #[arg(short, long, default_value = "workflow.yaml")]
        output: String,

        /// Template type
        #[arg(short, long, default_value = "basic")]
        template: String,

        /// Force overwrite if file exists
        #[arg(short, long)]
        force: bool,
    },

    /// Validate workflow file(s)
    Validate {
        /// Path to workflow file or directory
        path: String,

        /// Recursive directory scanning
        #[arg(short, long)]
        recursive: bool,

        /// Fail on warnings
        #[arg(short = 'W', long)]
        fail_on_warnings: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Test workflow with sample data
    Test {
        /// Path to workflow file
        workflow: String,

        /// Path to test file
        #[arg(short, long)]
        test_file: Option<String>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Deploy workflow to cluster
    Deploy {
        /// Path to workflow file
        workflow: String,

        /// Deployment strategy (direct, blue-green, canary)
        #[arg(short, long, default_value = "direct")]
        strategy: String,

        /// Dry run (validate only)
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Wait for deployment to complete
        #[arg(short, long)]
        wait: bool,
    },

    /// List deployed workflows
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,

        /// Output format (table, json, yaml)
        #[arg(short, long, default_value = "table")]
        format: String,
    },

    /// Show workflow status
    Status {
        /// Workflow ID or name
        workflow: String,

        /// Show execution history
        #[arg(short, long)]
        history: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Rollback workflow deployment
    Rollback {
        /// Workflow ID or name
        workflow: String,

        /// Target version (defaults to previous)
        #[arg(short, long)]
        version: Option<String>,

        /// Force rollback without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Export workflow to YAML/JSON
    Export {
        /// Workflow ID or name
        workflow: String,

        /// Output file path
        #[arg(short, long)]
        output: String,

        /// Output format (yaml, json)
        #[arg(short, long, default_value = "yaml")]
        format: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // Test init command
        let cli = Cli::parse_from(["arcxa-coordinator", "workflow", "init", "test-workflow"]);
        assert!(matches!(cli.command, Commands::Workflow(_)));
    }

    #[test]
    fn test_validate_command() {
        let cli = Cli::parse_from([
            "arcxa-coordinator",
            "workflow",
            "validate",
            "workflow.yaml",
            "-r",
        ]);
        if let Commands::Workflow(WorkflowCommands::Validate {
            path, recursive, ..
        }) = cli.command
        {
            assert_eq!(path, "workflow.yaml");
            assert!(recursive);
        } else {
            panic!("Expected Validate command");
        }
    }

    #[test]
    fn test_deploy_command() {
        let cli = Cli::parse_from([
            "graphica",
            "workflow",
            "deploy",
            "workflow.yaml",
            "-s",
            "blue-green",
        ]);
        if let Commands::Workflow(WorkflowCommands::Deploy { strategy, .. }) = cli.command {
            assert_eq!(strategy, "blue-green");
        } else {
            panic!("Expected Deploy command");
        }
    }
}
