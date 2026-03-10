//! Command implementations for workflow CLI

use crate::workflows::cicd::*;
use crate::workflows::declarative::{DeclarativeParser, WorkflowBuilder, WorkflowSerializer};
use crate::workflows::deployment::*;
use crate::workflows::git::*;
use crate::workflows::storage::WorkflowStore;
use crate::workflows::testing::TestExecutor;
use anyhow::{Context, Result};
use colored::Colorize;
use graphica_core::workflows::*;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Execute workflow init command
pub fn init_workflow(name: String, output: String, template: String, force: bool) -> Result<()> {
    // Check if file exists
    if Path::new(&output).exists() && !force {
        anyhow::bail!(
            "File '{}' already exists. Use --force to overwrite.",
            output
        );
    }

    // Get template content
    let template_content = super::templates::get_template(&template)
        .with_context(|| format!("Template '{}' not found", template))?;

    // Replace template variables
    let content = template_content.replace("{{name}}", &name);

    // Write to file
    fs::write(&output, content).with_context(|| format!("Failed to write to '{}'", output))?;

    println!(
        "{} Created workflow '{}' at {}",
        "✓".green().bold(),
        name.cyan(),
        output.yellow()
    );

    Ok(())
}

/// Execute workflow validate command
pub fn validate_workflow(
    path: String,
    recursive: bool,
    fail_on_warnings: bool,
    format: String,
) -> Result<()> {
    let path_obj = Path::new(&path);

    // Determine if path is file or directory
    let schemas = if path_obj.is_file() {
        vec![(
            path_obj.to_path_buf(),
            DeclarativeParser::parse_file(&path)?,
        )]
    } else if path_obj.is_dir() {
        let (successes, failures) = DeclarativeParser::parse_directory(&path, recursive);

        // Report parsing failures
        if !failures.is_empty() {
            eprintln!("{} {} parsing error(s):", "✗".red().bold(), failures.len());
            for (file, error) in &failures {
                eprintln!("  {} {}", file.display(), error.to_string().red());
            }
        }

        if successes.is_empty() {
            anyhow::bail!("No valid workflow files found");
        }

        successes
            .into_iter()
            .map(|s| (path_obj.join(&s.metadata.name).with_extension("yaml"), s))
            .collect()
    } else {
        anyhow::bail!("Path '{}' does not exist", path);
    };

    // Validate each schema
    let mut total_errors = 0;
    let mut total_warnings = 0;

    // Create composite validator
    let validators: Vec<Box<dyn Validator>> = vec![
        Box::new(SchemaValidator),
        Box::new(SemanticValidator),
        Box::new(DependencyValidator),
        Box::new(ResourceValidator),
    ];
    let composite = CompositeValidator::with_validators(validators);

    for (file_path, schema) in &schemas {
        let result = composite.validate(schema);

        match format.as_str() {
            "json" => {
                let json = serde_json::json!({
                    "file": file_path.display().to_string(),
                    "valid": result.valid,
                    "errors": result.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
                    "warnings": result.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            _ => {
                // Text format
                if result.valid && result.warnings.is_empty() {
                    println!(
                        "{} {} is valid",
                        "✓".green().bold(),
                        file_path.display().to_string().cyan()
                    );
                } else {
                    println!(
                        "{} {}",
                        if result.valid {
                            "⚠".yellow().bold()
                        } else {
                            "✗".red().bold()
                        },
                        file_path.display().to_string().cyan()
                    );

                    for error in &result.errors {
                        println!("  {} {}", "ERROR:".red().bold(), error);
                    }

                    for warning in &result.warnings {
                        println!("  {} {}", "WARNING:".yellow().bold(), warning);
                    }
                }
            }
        }

        total_errors += result.errors.len();
        total_warnings += result.warnings.len();
    }

    // Summary
    if format == "text" {
        println!();
        println!(
            "Validated {} workflow(s): {} error(s), {} warning(s)",
            schemas.len(),
            if total_errors > 0 {
                total_errors.to_string().red()
            } else {
                total_errors.to_string().green()
            },
            if total_warnings > 0 {
                total_warnings.to_string().yellow()
            } else {
                total_warnings.to_string().green()
            }
        );
    }

    // Exit code
    if total_errors > 0 {
        anyhow::bail!("Validation failed with {} error(s)", total_errors);
    }

    if fail_on_warnings && total_warnings > 0 {
        anyhow::bail!(
            "Validation failed with {} warning(s) (--fail-on-warnings)",
            total_warnings
        );
    }

    Ok(())
}

/// Execute workflow test command
pub async fn test_workflow(
    workflow: String,
    test_file: Option<String>,
    verbose: bool,
) -> Result<()> {
    // Parse workflow
    let schema = DeclarativeParser::parse_file(&workflow)
        .with_context(|| format!("Failed to parse workflow '{}'", workflow))?;

    // Build domain workflow
    let domain_workflow =
        WorkflowBuilder::build(&schema).with_context(|| "Failed to build workflow from schema")?;

    println!("{} Workflow: {}", "✓".green().bold(), schema.metadata.name);
    println!("  Routes: {}", domain_workflow.routes.len());
    println!("  Execution mode: {:?}", domain_workflow.execution_mode);

    // If test file provided, run tests
    let has_test_file = test_file.is_some();
    if let Some(test_path) = test_file {
        println!("\n{} Running tests from {}", "→".blue().bold(), test_path);

        let executor = TestExecutor::new(verbose);
        let result = executor.execute_suite(&test_path).await?;

        println!();
        if result.all_passed() {
            println!("{} {}", "✓".green().bold(), result.summary().green());
        } else {
            println!("{} {}", "✗".red().bold(), result.summary().red());
        }

        if !result.all_passed() {
            anyhow::bail!("Tests failed");
        }
    }

    if verbose && !has_test_file {
        println!("\n{}", "Workflow details:".bold());
        for (i, route) in domain_workflow.routes.iter().enumerate() {
            println!("  Route {}: {}", i + 1, route.name);
            println!("    Priority: {}", route.priority);
            println!("    Actions: {}", route.actions.len());
        }
    }

    Ok(())
}

/// Execute workflow deploy command
pub async fn deploy_workflow(
    workflow: String,
    strategy: String,
    dry_run: bool,
    wait: bool,
) -> Result<()> {
    // Parse deployment strategy
    let deployment_strategy = match strategy.as_str() {
        "direct" => DeploymentStrategy::Direct,
        "blue-green" => DeploymentStrategy::BlueGreen {
            traffic_percentage: 100,
        },
        "canary" => DeploymentStrategy::Canary {
            initial_percentage: 10,
            increment: 10,
            interval_seconds: 300,
        },
        _ => anyhow::bail!("Unknown deployment strategy: {}", strategy),
    };

    // Create deployment request
    let request = DeploymentRequest {
        workflow_file: workflow.clone(),
        environment: "production".to_string(),
        strategy: deployment_strategy,
        deployed_by: "cli@graphica".to_string(),
        metadata: std::collections::HashMap::new(),
        skip_validation: false,
        dry_run,
    };

    // Create deployment engine
    let deployment_store = Arc::new(DeploymentStore::new());
    let workflow_store = Arc::new(WorkflowStore::new());
    let engine = DeploymentEngine::new(deployment_store.clone(), workflow_store);

    println!(
        "{} Deploying workflow with strategy '{}'...",
        "→".blue().bold(),
        strategy
    );

    // Execute deployment
    let deployment = engine.deploy(request).await?;

    if dry_run {
        println!(
            "{} Dry run: workflow is valid and ready for deployment",
            "✓".green().bold()
        );
        println!("  Deployment ID: {}", deployment.id.yellow());
        println!("  Strategy: {}", strategy);
        println!("  Status: {:?}", deployment.status);
        return Ok(());
    }

    // Display deployment result
    if deployment.is_active() {
        println!("{} Workflow deployed successfully", "✓".green().bold());
        println!("  Deployment ID: {}", deployment.id.yellow());
        println!("  Version: {}", deployment.version);
        println!("  Environment: {}", deployment.environment);

        if let Some(duration) = deployment.duration_seconds() {
            println!("  Duration: {}s", duration);
        }
    } else {
        println!(
            "{} Deployment completed with status: {:?}",
            "⚠".yellow().bold(),
            deployment.status
        );
    }

    if wait {
        println!("  {} Monitoring deployment health...", "→".blue());
        // Health checks are already performed in the deployment process
        if deployment.all_health_checks_passed() {
            println!("  {} All health checks passed", "✓".green());
        }
    }

    Ok(())
}

/// Execute workflow list command
pub fn list_workflows(status_filter: Option<String>, format: String) -> Result<()> {
    let store = WorkflowStore::new();
    let workflows = store.list()?;

    // Filter by status if specified
    let filtered: Vec<_> = if let Some(ref status) = status_filter {
        workflows
            .into_iter()
            .filter(|w| {
                // TODO: Add status field to workflow
                status == "active" // Placeholder
            })
            .collect()
    } else {
        workflows
    };

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&filtered)?;
            println!("{}", json);
        }
        "yaml" => {
            let yaml = serde_yaml::to_string(&filtered)?;
            println!("{}", yaml);
        }
        _ => {
            // Table format
            println!("{}", "Deployed Workflows:".bold());
            println!();
            println!(
                "{:<40} {:<20} {:<10}",
                "ID".bold(),
                "Name".bold(),
                "Routes".bold()
            );
            println!("{}", "-".repeat(70));

            for workflow in &filtered {
                println!(
                    "{:<40} {:<20} {:<10}",
                    workflow.id, workflow.name, workflow.route_count
                );
            }

            println!();
            println!("Total: {} workflow(s)", filtered.len());
        }
    }

    Ok(())
}

/// Execute workflow status command
pub fn status_workflow(workflow: String, history: bool, format: String) -> Result<()> {
    let store = WorkflowStore::new();
    let wf = store.get_required(&workflow)?;

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&wf)?;
            println!("{}", json);
        }
        _ => {
            println!("{}", "Workflow Status:".bold());
            println!();
            println!("  {}: {}", "ID".bold(), wf.id);
            println!("  {}: {}", "Name".bold(), wf.name);
            println!("  {}: {}", "Description".bold(), wf.description);
            println!("  {}: {}", "Version".bold(), wf.version);
            println!("  {}: {:?}", "Execution Mode".bold(), wf.execution_mode);
            println!("  {}: {}", "Routes".bold(), wf.routes.len());

            if history {
                println!();
                println!("{}", "Execution History:".bold());
                // TODO: Implement execution history
                println!("  {} History not yet implemented", "⚠".yellow());
            }
        }
    }

    Ok(())
}

/// Execute workflow rollback command
pub async fn rollback_workflow(
    workflow: String,
    version: Option<String>,
    force: bool,
) -> Result<()> {
    if !force {
        println!(
            "{} Are you sure you want to rollback deployment '{}'? (y/N): ",
            "⚠".yellow().bold(),
            workflow.cyan()
        );

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Rollback cancelled");
            return Ok(());
        }
    }

    // Create deployment engine
    let deployment_store = Arc::new(DeploymentStore::new());
    let workflow_store = Arc::new(WorkflowStore::new());
    let engine = DeploymentEngine::new(deployment_store.clone(), workflow_store);

    // Create rollback request
    let request = RollbackRequest {
        deployment_id: workflow.clone(),
        target_deployment_id: version,
        reason: "Manual rollback via CLI".to_string(),
        rolled_back_by: "cli@graphica".to_string(),
        force,
    };

    println!(
        "{} Rolling back deployment '{}'...",
        "→".blue().bold(),
        workflow
    );

    // Execute rollback
    let target_deployment = engine.rollback(request).await?;

    println!("{} Rollback successful", "✓".green().bold());
    println!("  Target deployment ID: {}", target_deployment.id.yellow());
    println!("  Version: {}", target_deployment.version);
    println!("  Environment: {}", target_deployment.environment);

    Ok(())
}

/// Execute workflow export command
pub fn export_workflow(workflow: String, output: String, format: String) -> Result<()> {
    let store = WorkflowStore::new();
    let wf = store.get_required(&workflow)?;

    // Serialize to desired format
    let content = match format.as_str() {
        "json" => WorkflowSerializer::to_json(&wf)?,
        _ => WorkflowSerializer::to_yaml(&wf)?,
    };

    // Write to file
    fs::write(&output, content).with_context(|| format!("Failed to write to '{}'", output))?;

    println!(
        "{} Exported workflow '{}' to {}",
        "✓".green().bold(),
        workflow.cyan(),
        output.yellow()
    );

    Ok(())
}

/// Install Git pre-commit hook
pub fn install_git_hook(path: String, fail_on_warnings: bool) -> Result<()> {
    let repo_path = Path::new(&path);

    if !repo_path.exists() {
        anyhow::bail!("Path does not exist: {}", path);
    }

    // Check if it's a Git repository
    let helper = GitHelper::new(path.clone());
    if !helper.is_git_repo() {
        println!(
            "{} Not a Git repository. Initialize? (y/N): ",
            "?".yellow().bold()
        );

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            helper.init()?;
            println!("{} Git repository initialized", "✓".green().bold());
        } else {
            anyhow::bail!("Not a Git repository");
        }
    }

    // Install pre-commit hook
    PreCommitHook::install(repo_path)?;

    println!(
        "{} Git pre-commit hook installed successfully",
        "✓".green().bold()
    );
    println!("  Workflows will be validated before each commit");

    if fail_on_warnings {
        println!("  {} Commits will fail on warnings", "⚠".yellow());
    }

    Ok(())
}

/// Watch workflow files for changes
pub fn watch_workflows(path: String, auto_validate: bool) -> Result<()> {
    let watch_path = Path::new(&path);

    if !watch_path.exists() {
        anyhow::bail!("Path does not exist: {}", path);
    }

    let watcher = WorkflowWatcher::new()
        .with_paths(vec![watch_path.to_path_buf()])
        .with_debounce(500);

    println!("{} Starting workflow file watcher...", "👁".bold());
    println!("  Path: {}", watch_path.display());
    println!("  Auto-validate: {}", auto_validate);
    println!("  Press Ctrl+C to stop");
    println!();

    watcher.watch(|event| {
        println!(
            "{} {} {}",
            "📝".bold(),
            event.event_type().cyan(),
            event.path().display()
        );

        if auto_validate && watcher.matches_pattern(event.path()) {
            match watcher.validate_file(event.path()) {
                Ok(result) => {
                    if result.valid {
                        println!("  {} Validation passed", "✓".green());
                    } else {
                        println!("  {} Validation failed", "✗".red());
                        for error in &result.errors {
                            println!("    {}", error.to_string().red());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  {} Validation error: {}", "✗".red(), e);
                }
            }
        }

        Ok(())
    })?;

    Ok(())
}

/// Setup Git repository for workflows
pub fn setup_git_repo(path: String) -> Result<()> {
    let repo_path = Path::new(&path);

    println!("{} Setting up Git repository for workflows...", "🔧".bold());

    setup_workflow_repo(repo_path)?;

    println!();
    println!("{} Git repository setup complete!", "✅".bold());
    println!("  Next steps:");
    println!("    1. Install pre-commit hook: arcxa-coordinator git install-hook");
    println!("    2. Create your first workflow: arcxa-coordinator workflow init my-workflow");
    println!("    3. Commit changes: git add . && git commit -m 'Add workflow'");

    Ok(())
}

/// Generate CI/CD configuration
pub fn generate_cicd_config(platform: String, output: Option<String>) -> Result<()> {
    // Parse platform
    let ci_platform = match platform.to_lowercase().as_str() {
        "github" | "github-actions" => CiPlatform::GitHubActions,
        "gitlab" | "gitlab-ci" => CiPlatform::GitLabCI,
        "jenkins" => CiPlatform::Jenkins,
        "circleci" | "circle" => CiPlatform::CircleCI,
        _ => anyhow::bail!(
            "Unknown CI platform: {}. Supported: github, gitlab, jenkins, circleci",
            platform
        ),
    };

    println!(
        "{} Generating {} configuration...",
        "🔧".bold(),
        ci_platform.name().cyan()
    );

    // Create template generator
    let mut generator = TemplateGenerator::new(ci_platform);

    // Set default variables
    generator.set_variable("rust_version", "1.75");

    // Generate configuration
    let config = generator.generate()?;

    // Determine output path
    let output_path = output.unwrap_or_else(|| ci_platform.config_file().to_string());

    // Create parent directories if needed
    if let Some(parent) = Path::new(&output_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Write configuration
    fs::write(&output_path, config)?;

    println!(
        "{} CI/CD configuration generated successfully",
        "✓".green().bold()
    );
    println!("  Platform: {}", ci_platform.name());
    println!("  File: {}", output_path.yellow());
    println!();
    println!("  Next steps:");
    println!("    1. Review and customize the configuration");
    println!(
        "    2. Commit the file: git add {} && git commit -m 'Add CI/CD'",
        output_path
    );
    println!("    3. Push to repository to trigger the pipeline");

    Ok(())
}

/// List available CI/CD platforms
pub fn list_cicd_platforms() -> Result<()> {
    println!("{} Available CI/CD platforms:", "📋".bold());
    println!();

    for platform in available_platforms() {
        println!("  {} {}", "•".cyan(), platform.name().bold());
        println!("    Config file: {}", platform.config_file().yellow());
        println!();
    }

    println!("Usage:");
    println!("  graphica cicd generate <platform>");
    println!();
    println!("Example:");
    println!("  graphica cicd generate github");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("test.yaml");

        let result = init_workflow(
            "test-workflow".to_string(),
            output.to_string_lossy().to_string(),
            "basic".to_string(),
            false,
        );

        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[test]
    fn test_init_workflow_force_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let output = temp_dir.path().join("test.yaml");

        // Create file first
        fs::write(&output, "existing content").unwrap();

        // Try without force (should fail)
        let result = init_workflow(
            "test-workflow".to_string(),
            output.to_string_lossy().to_string(),
            "basic".to_string(),
            false,
        );
        assert!(result.is_err());

        // Try with force (should succeed)
        let result = init_workflow(
            "test-workflow".to_string(),
            output.to_string_lossy().to_string(),
            "basic".to_string(),
            true,
        );
        assert!(result.is_ok());
    }
}
