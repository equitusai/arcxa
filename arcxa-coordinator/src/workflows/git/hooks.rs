//! Git hooks for workflow validation

use crate::workflows::declarative::DeclarativeParser;
use anyhow::{Context, Result};
use graphica_core::workflows::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Pre-commit hook for workflow validation
pub struct PreCommitHook {
    /// Workflow file patterns to validate
    patterns: Vec<String>,

    /// Fail on warnings
    fail_on_warnings: bool,

    /// Skip validation for specific files
    skip_patterns: Vec<String>,
}

impl PreCommitHook {
    /// Create a new pre-commit hook
    pub fn new() -> Self {
        Self {
            patterns: vec!["*.yaml".to_string(), "*.yml".to_string()],
            fail_on_warnings: false,
            skip_patterns: vec![],
        }
    }

    /// Set workflow file patterns
    pub fn with_patterns(mut self, patterns: Vec<String>) -> Self {
        self.patterns = patterns;
        self
    }

    /// Set fail on warnings
    pub fn fail_on_warnings(mut self, fail: bool) -> Self {
        self.fail_on_warnings = fail;
        self
    }

    /// Set skip patterns
    pub fn skip_patterns(mut self, patterns: Vec<String>) -> Self {
        self.skip_patterns = patterns;
        self
    }

    /// Run the pre-commit hook
    pub fn run(&self) -> Result<HookResult> {
        let mut result = HookResult {
            passed: true,
            validated_files: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Get staged files
        let staged_files = self.get_staged_files()?;

        // Filter workflow files
        let workflow_files: Vec<PathBuf> = staged_files
            .into_iter()
            .filter(|path| self.is_workflow_file(path))
            .filter(|path| !self.should_skip(path))
            .collect();

        if workflow_files.is_empty() {
            return Ok(result);
        }

        println!("🔍 Validating {} workflow file(s)...", workflow_files.len());

        // Validate each file
        for file_path in workflow_files {
            match self.validate_workflow_file(&file_path) {
                Ok(file_result) => {
                    result.validated_files.push(file_path.display().to_string());

                    if !file_result.errors.is_empty() {
                        result.passed = false;
                        for error in &file_result.errors {
                            result
                                .errors
                                .push(format!("{}: {}", file_path.display(), error));
                        }
                    }

                    if !file_result.warnings.is_empty() {
                        for warning in &file_result.warnings {
                            result
                                .warnings
                                .push(format!("{}: {}", file_path.display(), warning));
                        }

                        if self.fail_on_warnings {
                            result.passed = false;
                        }
                    }

                    if file_result.errors.is_empty() && file_result.warnings.is_empty() {
                        println!("  ✓ {}", file_path.display());
                    }
                }
                Err(e) => {
                    result.passed = false;
                    result
                        .errors
                        .push(format!("{}: {}", file_path.display(), e));
                    eprintln!("  ✗ {}: {}", file_path.display(), e);
                }
            }
        }

        Ok(result)
    }

    /// Get staged files from git
    fn get_staged_files(&self) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .args(["diff", "--cached", "--name-only", "--diff-filter=ACM"])
            .stdout(Stdio::piped())
            .output()
            .context("Failed to execute git command")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let files = String::from_utf8(output.stdout)?
            .lines()
            .map(PathBuf::from)
            .collect();

        Ok(files)
    }

    /// Check if file is a workflow file
    fn is_workflow_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            self.patterns.iter().any(|pattern| {
                pattern
                    .trim_start_matches("*.")
                    .eq_ignore_ascii_case(&ext_str)
            })
        } else {
            false
        }
    }

    /// Check if file should be skipped
    fn should_skip(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.skip_patterns
            .iter()
            .any(|pattern| path_str.contains(pattern))
    }

    /// Validate a single workflow file
    fn validate_workflow_file(&self, path: &Path) -> Result<FileValidationResult> {
        let mut file_result = FileValidationResult {
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Parse workflow
        let schema = match DeclarativeParser::parse_file(path.to_str().unwrap()) {
            Ok(s) => s,
            Err(e) => {
                file_result.errors.push(format!("Parse error: {}", e));
                return Ok(file_result);
            }
        };

        // Validate with composite validator
        let validators: Vec<Box<dyn Validator>> = vec![
            Box::new(SchemaValidator),
            Box::new(SemanticValidator),
            Box::new(DependencyValidator),
            Box::new(ResourceValidator),
        ];

        let composite = CompositeValidator::with_validators(validators);
        let validation = composite.validate(&schema);

        for error in validation.errors {
            file_result.errors.push(error.to_string());
        }

        for warning in validation.warnings {
            file_result.warnings.push(warning.to_string());
        }

        Ok(file_result)
    }

    /// Install the pre-commit hook
    pub fn install(git_dir: &Path) -> Result<()> {
        let hooks_dir = git_dir.join(".git").join("hooks");
        fs::create_dir_all(&hooks_dir)?;

        let hook_path = hooks_dir.join("pre-commit");
        let hook_content = Self::generate_hook_script();

        fs::write(&hook_path, hook_content)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }

        println!("✓ Pre-commit hook installed at {}", hook_path.display());
        Ok(())
    }

    /// Generate pre-commit hook script
    fn generate_hook_script() -> String {
        r#"#!/bin/sh
# ARCXA workflow validation pre-commit hook

echo "🔍 Running ARCXA workflow validation..."

# Run ARCXA coordinator CLI validation
if command -v arcxa-coordinator >/dev/null 2>&1; then
    arcxa-coordinator workflow validate . -r
else
    # Fallback to cargo if graphica CLI not installed
    cargo run --bin arcxa-coordinator -- workflow validate . -r
fi

if [ $? -ne 0 ]; then
    echo "❌ Workflow validation failed. Commit aborted."
    echo "   Fix validation errors or use --no-verify to skip."
    exit 1
fi

echo "✓ Workflow validation passed"
exit 0
"#
        .to_string()
    }
}

impl Default for PreCommitHook {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook execution result
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether hook passed
    pub passed: bool,

    /// Files that were validated
    pub validated_files: Vec<String>,

    /// Errors encountered
    pub errors: Vec<String>,

    /// Warnings encountered
    pub warnings: Vec<String>,
}

/// File validation result
#[derive(Debug, Clone)]
struct FileValidationResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_creation() {
        let hook = PreCommitHook::new();
        assert!(!hook.fail_on_warnings);
        assert_eq!(hook.patterns.len(), 2);
    }

    #[test]
    fn test_hook_with_patterns() {
        let hook = PreCommitHook::new()
            .with_patterns(vec!["*.yaml".to_string()])
            .fail_on_warnings(true);

        assert!(hook.fail_on_warnings);
        assert_eq!(hook.patterns.len(), 1);
    }

    #[test]
    fn test_is_workflow_file() {
        let hook = PreCommitHook::new();

        assert!(hook.is_workflow_file(Path::new("workflow.yaml")));
        assert!(hook.is_workflow_file(Path::new("workflow.yml")));
        assert!(!hook.is_workflow_file(Path::new("README.md")));
        assert!(!hook.is_workflow_file(Path::new("file.txt")));
    }

    #[test]
    fn test_should_skip() {
        let hook =
            PreCommitHook::new().skip_patterns(vec!["test/".to_string(), ".backup".to_string()]);

        assert!(hook.should_skip(Path::new("test/workflow.yaml")));
        assert!(hook.should_skip(Path::new("workflow.yaml.backup")));
        assert!(!hook.should_skip(Path::new("workflows/prod.yaml")));
    }

    #[test]
    fn test_hook_script_generation() {
        let script = PreCommitHook::generate_hook_script();
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("arcxa-coordinator workflow validate"));
        assert!(script.contains("exit 1"));
    }

    #[test]
    fn test_hook_result() {
        let result = HookResult {
            passed: true,
            validated_files: vec!["workflow.yaml".to_string()],
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        assert!(result.passed);
        assert_eq!(result.validated_files.len(), 1);
    }
}
