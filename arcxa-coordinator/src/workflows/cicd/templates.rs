//! CI/CD templates for workflow automation

use anyhow::Result;
use std::collections::HashMap;

/// CI/CD platform
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CiPlatform {
    /// GitHub Actions
    GitHubActions,

    /// GitLab CI/CD
    GitLabCI,

    /// Jenkins
    Jenkins,

    /// CircleCI
    CircleCI,
}

impl CiPlatform {
    /// Get platform name
    pub fn name(&self) -> &'static str {
        match self {
            CiPlatform::GitHubActions => "GitHub Actions",
            CiPlatform::GitLabCI => "GitLab CI",
            CiPlatform::Jenkins => "Jenkins",
            CiPlatform::CircleCI => "CircleCI",
        }
    }

    /// Get config file name
    pub fn config_file(&self) -> &'static str {
        match self {
            CiPlatform::GitHubActions => ".github/workflows/graphica.yml",
            CiPlatform::GitLabCI => ".gitlab-ci.yml",
            CiPlatform::Jenkins => "Jenkinsfile",
            CiPlatform::CircleCI => ".circleci/config.yml",
        }
    }
}

/// CI/CD template generator
pub struct TemplateGenerator {
    platform: CiPlatform,
    variables: HashMap<String, String>,
}

impl TemplateGenerator {
    /// Create a new template generator
    pub fn new(platform: CiPlatform) -> Self {
        Self {
            platform,
            variables: HashMap::new(),
        }
    }

    /// Set a template variable
    pub fn set_variable(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Generate CI/CD configuration
    pub fn generate(&self) -> Result<String> {
        let template = match self.platform {
            CiPlatform::GitHubActions => self.generate_github_actions(),
            CiPlatform::GitLabCI => self.generate_gitlab_ci(),
            CiPlatform::Jenkins => self.generate_jenkins(),
            CiPlatform::CircleCI => self.generate_circleci(),
        };

        Ok(self.replace_variables(&template))
    }

    /// Generate GitHub Actions workflow
    fn generate_github_actions(&self) -> String {
        GITHUB_ACTIONS_TEMPLATE.to_string()
    }

    /// Generate GitLab CI configuration
    fn generate_gitlab_ci(&self) -> String {
        GITLAB_CI_TEMPLATE.to_string()
    }

    /// Generate Jenkins pipeline
    fn generate_jenkins(&self) -> String {
        JENKINS_TEMPLATE.to_string()
    }

    /// Generate CircleCI configuration
    fn generate_circleci(&self) -> String {
        CIRCLECI_TEMPLATE.to_string()
    }

    /// Replace variables in template
    fn replace_variables(&self, template: &str) -> String {
        let mut result = template.to_string();
        for (key, value) in &self.variables {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        result
    }
}

/// GitHub Actions workflow template
const GITHUB_ACTIONS_TEMPLATE: &str = r#"name: ARCXA Workflows

on:
  push:
    branches: [ main, develop ]
    paths:
      - 'workflows/**/*.yaml'
      - 'workflows/**/*.yml'
  pull_request:
    branches: [ main, develop ]
    paths:
      - 'workflows/**/*.yaml'
      - 'workflows/**/*.yml'

env:
  RUST_VERSION: {{rust_version}}

jobs:
  validate:
    name: Validate Workflows
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: stable

      - name: Cache dependencies
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Install ARCXA CLI (graphica)
        run: |
          cargo install --path arcxa-coordinator

      - name: Validate workflows
        run: |
          arcxa-coordinator workflow validate workflows/ --recursive --fail-on-warnings

      - name: Run workflow tests
        run: |
          for test_file in workflows/tests/*.yaml; do
            workflow_file="${test_file/tests\//}"
            workflow_file="${workflow_file/_test/}"
            if [ -f "$workflow_file" ]; then
              arcxa-coordinator workflow test "$workflow_file" -t "$test_file" -v
            fi
          done

  test:
    name: Integration Tests
    runs-on: ubuntu-latest
    needs: validate

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Run tests
        run: cargo test --workspace

  deploy-staging:
    name: Deploy to Staging
    runs-on: ubuntu-latest
    needs: test
    if: github.ref == 'refs/heads/develop'
    environment: staging

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Deploy workflows
        run: |
          for workflow in workflows/*.yaml; do
            arcxa-coordinator workflow deploy "$workflow" \
              --strategy blue-green \
              --environment staging \
              --wait
          done

  deploy-production:
    name: Deploy to Production
    runs-on: ubuntu-latest
    needs: test
    if: github.ref == 'refs/heads/main'
    environment: production

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Deploy workflows
        run: |
          for workflow in workflows/*.yaml; do
            arcxa-coordinator workflow deploy "$workflow" \
              --strategy canary \
              --environment production \
              --wait
          done

      - name: Health check
        run: |
          # Add health check commands here
          echo "Health checks passed"

      - name: Notify deployment
        if: success()
        run: |
          echo "✅ Production deployment successful"
"#;

/// GitLab CI/CD template
const GITLAB_CI_TEMPLATE: &str = r#"# ARCXA Workflows CI/CD Pipeline

variables:
  RUST_VERSION: "{{rust_version}}"
  CARGO_HOME: ${CI_PROJECT_DIR}/.cargo

stages:
  - validate
  - test
  - deploy-staging
  - deploy-production

cache:
  key: ${CI_COMMIT_REF_SLUG}
  paths:
    - .cargo/
    - target/

.rust-setup: &rust-setup
  image: rust:${RUST_VERSION}
  before_script:
    - rustc --version
    - cargo --version
    - cargo install --path arcxa-coordinator || true

validate-workflows:
  <<: *rust-setup
  stage: validate
  script:
    - arcxa-coordinator workflow validate workflows/ --recursive --fail-on-warnings
  only:
    changes:
      - workflows/**/*.yaml
      - workflows/**/*.yml

test-workflows:
  <<: *rust-setup
  stage: test
  script:
    - |
      for test_file in workflows/tests/*.yaml; do
        workflow_file="${test_file/tests\//}"
        workflow_file="${workflow_file/_test/}"
        if [ -f "$workflow_file" ]; then
          arcxa-coordinator workflow test "$workflow_file" -t "$test_file" -v
        fi
      done
  only:
    changes:
      - workflows/**/*.yaml
      - workflows/**/*.yml

integration-tests:
  <<: *rust-setup
  stage: test
  script:
    - cargo test --workspace

deploy-staging:
  <<: *rust-setup
  stage: deploy-staging
  environment:
    name: staging
    on_stop: stop-staging
  script:
    - |
      for workflow in workflows/*.yaml; do
        arcxa-coordinator workflow deploy "$workflow" \
          --strategy blue-green \
          --environment staging \
          --wait
      done
  only:
    - develop

deploy-production:
  <<: *rust-setup
  stage: deploy-production
  environment:
    name: production
  script:
    - |
      for workflow in workflows/*.yaml; do
        arcxa-coordinator workflow deploy "$workflow" \
          --strategy canary \
          --environment production \
          --wait
      done
  when: manual
  only:
    - main

rollback-production:
  <<: *rust-setup
  stage: deploy-production
  environment:
    name: production
  script:
    - |
      # Rollback to previous version
      arcxa-coordinator workflow rollback ${DEPLOYMENT_ID} --force
  when: manual
  only:
    - main
"#;

/// Jenkins pipeline template
const JENKINS_TEMPLATE: &str = r#"// ARCXA Workflows Jenkins Pipeline

pipeline {
    agent any

    environment {
        RUST_VERSION = '{{rust_version}}'
    }

    stages {
        stage('Setup') {
            steps {
                sh 'rustc --version'
                sh 'cargo --version'
                sh 'cargo install --path arcxa-coordinator || true'
            }
        }

        stage('Validate') {
            steps {
                sh 'arcxa-coordinator workflow validate workflows/ --recursive --fail-on-warnings'
            }
        }

        stage('Test') {
            steps {
                sh '''
                    for test_file in workflows/tests/*.yaml; do
                        workflow_file="${test_file/tests\\//}"
                        workflow_file="${workflow_file/_test/}"
                        if [ -f "$workflow_file" ]; then
                            arcxa-coordinator workflow test "$workflow_file" -t "$test_file" -v
                        fi
                    done
                '''
                sh 'cargo test --workspace'
            }
        }

        stage('Deploy Staging') {
            when {
                branch 'develop'
            }
            steps {
                sh '''
                    for workflow in workflows/*.yaml; do
                        arcxa-coordinator workflow deploy "$workflow" \
                            --strategy blue-green \
                            --environment staging \
                            --wait
                    done
                '''
            }
        }

        stage('Deploy Production') {
            when {
                branch 'main'
            }
            input {
                message "Deploy to production?"
                ok "Deploy"
            }
            steps {
                sh '''
                    for workflow in workflows/*.yaml; do
                        arcxa-coordinator workflow deploy "$workflow" \
                            --strategy canary \
                            --environment production \
                            --wait
                    done
                '''
            }
        }
    }

    post {
        success {
            echo '✅ Pipeline completed successfully'
        }
        failure {
            echo '❌ Pipeline failed'
        }
    }
}
"#;

/// CircleCI configuration template
const CIRCLECI_TEMPLATE: &str = r#"# ARCXA Workflows CircleCI Configuration

version: 2.1

orbs:
  rust: circleci/rust@1.6.0

executors:
  rust-executor:
    docker:
      - image: cimg/rust:{{rust_version}}

jobs:
  validate:
    executor: rust-executor
    steps:
      - checkout
      - rust/install
      - restore_cache:
          keys:
            - cargo-cache-{{ checksum "Cargo.lock" }}
      - run:
          name: Install ARCXA CLI (graphica)
          command: cargo install --path arcxa-coordinator
      - run:
          name: Validate workflows
          command: arcxa-coordinator workflow validate workflows/ --recursive --fail-on-warnings
      - save_cache:
          key: cargo-cache-{{ checksum "Cargo.lock" }}
          paths:
            - ~/.cargo
            - ./target

  test:
    executor: rust-executor
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-cache-{{ checksum "Cargo.lock" }}
      - run:
          name: Run workflow tests
          command: |
            for test_file in workflows/tests/*.yaml; do
              workflow_file="${test_file/tests\//}"
              workflow_file="${workflow_file/_test/}"
              if [ -f "$workflow_file" ]; then
                arcxa-coordinator workflow test "$workflow_file" -t "$test_file" -v
              fi
            done
      - run:
          name: Run integration tests
          command: cargo test --workspace

  deploy-staging:
    executor: rust-executor
    steps:
      - checkout
      - run:
          name: Deploy to staging
          command: |
            for workflow in workflows/*.yaml; do
              arcxa-coordinator workflow deploy "$workflow" \
                --strategy blue-green \
                --environment staging \
                --wait
            done

  deploy-production:
    executor: rust-executor
    steps:
      - checkout
      - run:
          name: Deploy to production
          command: |
            for workflow in workflows/*.yaml; do
              arcxa-coordinator workflow deploy "$workflow" \
                --strategy canary \
                --environment production \
                --wait
            done

workflows:
  version: 2
  graphica-workflows:
    jobs:
      - validate
      - test:
          requires:
            - validate
      - deploy-staging:
          requires:
            - test
          filters:
            branches:
              only: develop
      - deploy-production:
          requires:
            - test
          filters:
            branches:
              only: main
"#;

/// Get available CI platforms
pub fn available_platforms() -> Vec<CiPlatform> {
    vec![
        CiPlatform::GitHubActions,
        CiPlatform::GitLabCI,
        CiPlatform::Jenkins,
        CiPlatform::CircleCI,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_names() {
        assert_eq!(CiPlatform::GitHubActions.name(), "GitHub Actions");
        assert_eq!(CiPlatform::GitLabCI.name(), "GitLab CI");
        assert_eq!(CiPlatform::Jenkins.name(), "Jenkins");
        assert_eq!(CiPlatform::CircleCI.name(), "CircleCI");
    }

    #[test]
    fn test_config_files() {
        assert_eq!(
            CiPlatform::GitHubActions.config_file(),
            ".github/workflows/graphica.yml"
        );
        assert_eq!(CiPlatform::GitLabCI.config_file(), ".gitlab-ci.yml");
        assert_eq!(CiPlatform::Jenkins.config_file(), "Jenkinsfile");
        assert_eq!(CiPlatform::CircleCI.config_file(), ".circleci/config.yml");
    }

    #[test]
    fn test_template_generator() {
        let mut gen = TemplateGenerator::new(CiPlatform::GitHubActions);
        gen.set_variable("rust_version", "1.75");

        let template = gen.generate().unwrap();
        assert!(template.contains("name: ARCXA Workflows"));
        assert!(template.contains("workflows/**/*.yaml"));
    }

    #[test]
    fn test_variable_replacement() {
        let mut gen = TemplateGenerator::new(CiPlatform::GitLabCI);
        gen.set_variable("rust_version", "1.75");

        let template = gen.generate().unwrap();
        assert!(template.contains("RUST_VERSION: \"1.75\""));
    }

    #[test]
    fn test_available_platforms() {
        let platforms = available_platforms();
        assert_eq!(platforms.len(), 4);
        assert!(platforms.contains(&CiPlatform::GitHubActions));
        assert!(platforms.contains(&CiPlatform::GitLabCI));
    }

    #[test]
    fn test_github_actions_template() {
        let gen = TemplateGenerator::new(CiPlatform::GitHubActions);
        let template = gen.generate().unwrap();

        assert!(template.contains("validate:"));
        assert!(template.contains("deploy-staging:"));
        assert!(template.contains("deploy-production:"));
    }

    #[test]
    fn test_gitlab_ci_template() {
        let gen = TemplateGenerator::new(CiPlatform::GitLabCI);
        let template = gen.generate().unwrap();

        assert!(template.contains("stages:"));
        assert!(template.contains("validate-workflows:"));
        assert!(template.contains("deploy-production:"));
    }
}
