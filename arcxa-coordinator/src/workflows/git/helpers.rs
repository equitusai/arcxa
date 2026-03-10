//! Git workflow helpers

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Git repository helper
pub struct GitHelper {
    /// Repository path
    repo_path: String,
}

impl GitHelper {
    /// Create a new Git helper
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Initialize a new Git repository
    pub fn init(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to initialize git repository")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Check if directory is a Git repository
    pub fn is_git_repo(&self) -> bool {
        let output = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&self.repo_path)
            .output();

        output.map(|o| o.status.success()).unwrap_or(false)
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get current branch")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to get branch: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    /// Get list of modified workflow files
    pub fn get_modified_workflows(&self) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["diff", "--name-only", "--", "*.yaml", "*.yml"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get modified files")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let files: Vec<String> = String::from_utf8(output.stdout)?
            .lines()
            .map(String::from)
            .collect();

        Ok(files)
    }

    /// Commit workflow changes
    pub fn commit_workflows(&self, message: &str, files: &[String]) -> Result<()> {
        // Add files
        for file in files {
            let output = Command::new("git")
                .args(["add", file])
                .current_dir(&self.repo_path)
                .output()
                .context("Failed to add file to git")?;

            if !output.status.success() {
                anyhow::bail!(
                    "Git add failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // Commit
        let output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to commit")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git commit failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Create a new branch for workflow changes
    pub fn create_workflow_branch(&self, workflow_name: &str) -> Result<String> {
        let branch_name = format!(
            "workflow/{}",
            workflow_name.to_lowercase().replace(' ', "-")
        );

        let output = Command::new("git")
            .args(["checkout", "-b", &branch_name])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to create branch")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create branch: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(branch_name)
    }

    /// Get workflow file history
    pub fn get_file_history(&self, file_path: &str, limit: usize) -> Result<Vec<CommitInfo>> {
        let output = Command::new("git")
            .args([
                "log",
                &format!("-{}", limit),
                "--pretty=format:%H|%an|%ae|%ai|%s",
                "--",
                file_path,
            ])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get file history")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git log failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let commits = String::from_utf8(output.stdout)?
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() == 5 {
                    Some(CommitInfo {
                        hash: parts[0].to_string(),
                        author_name: parts[1].to_string(),
                        author_email: parts[2].to_string(),
                        date: parts[3].to_string(),
                        message: parts[4].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(commits)
    }

    /// Get diff for a workflow file
    pub fn get_file_diff(&self, file_path: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["diff", "--", file_path])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get file diff")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Tag a workflow version
    pub fn tag_version(&self, workflow_name: &str, version: &str) -> Result<()> {
        let tag = format!("{}-{}", workflow_name, version);

        let output = Command::new("git")
            .args([
                "tag",
                "-a",
                &tag,
                "-m",
                &format!("Version {} of {}", version, workflow_name),
            ])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to create tag")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git tag failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// List all workflow tags
    pub fn list_workflow_tags(&self, workflow_name: &str) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["tag", "-l", &format!("{}*", workflow_name)])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to list tags")?;

        if !output.status.success() {
            anyhow::bail!(
                "Git tag list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let tags: Vec<String> = String::from_utf8(output.stdout)?
            .lines()
            .map(String::from)
            .collect();

        Ok(tags)
    }
}

/// Commit information
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Commit hash
    pub hash: String,

    /// Author name
    pub author_name: String,

    /// Author email
    pub author_email: String,

    /// Commit date
    pub date: String,

    /// Commit message
    pub message: String,
}

/// Setup Git repository for workflows
pub fn setup_workflow_repo(path: &Path) -> Result<()> {
    let helper = GitHelper::new(path.to_string_lossy().to_string());

    if !helper.is_git_repo() {
        println!("📁 Initializing Git repository...");
        helper.init()?;
    }

    // Create .gitignore if it doesn't exist
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        let gitignore_content = r#"# Graphica workflow artifacts
*.log
*.tmp
.DS_Store
target/
.idea/
*.swp
*.swo
"#;
        std::fs::write(&gitignore_path, gitignore_content)?;
        println!("📝 Created .gitignore");
    }

    // Create workflows directory structure
    let workflows_dir = path.join("workflows");
    if !workflows_dir.exists() {
        std::fs::create_dir(&workflows_dir)?;
        println!("📂 Created workflows/ directory");
    }

    let tests_dir = workflows_dir.join("tests");
    if !tests_dir.exists() {
        std::fs::create_dir(&tests_dir)?;
        println!("📂 Created workflows/tests/ directory");
    }

    println!("✅ Git repository setup complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_helper_creation() {
        let helper = GitHelper::new(".");
        assert_eq!(helper.repo_path, ".");
    }

    #[test]
    fn test_commit_info() {
        let commit = CommitInfo {
            hash: "abc123".to_string(),
            author_name: "John Doe".to_string(),
            author_email: "john@example.com".to_string(),
            date: "2024-01-01".to_string(),
            message: "Update workflow".to_string(),
        };

        assert_eq!(commit.hash, "abc123");
        assert_eq!(commit.author_name, "John Doe");
    }

    #[test]
    fn test_branch_name_generation() {
        let helper = GitHelper::new(".");
        let branch = format!(
            "workflow/{}",
            "Test Workflow".to_lowercase().replace(' ', "-")
        );
        assert_eq!(branch, "workflow/test-workflow");
    }
}
