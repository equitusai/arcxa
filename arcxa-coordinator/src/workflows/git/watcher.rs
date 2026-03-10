//! File watcher for workflow changes

use crate::workflows::declarative::DeclarativeParser;
use anyhow::{Context, Result};
use graphica_core::workflows::*;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Workflow file watcher
pub struct WorkflowWatcher {
    /// Watch paths
    paths: Vec<PathBuf>,

    /// File patterns to watch
    patterns: Vec<String>,

    /// Debounce duration (milliseconds)
    debounce_ms: u64,
}

impl WorkflowWatcher {
    /// Create a new workflow watcher
    pub fn new() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            patterns: vec!["*.yaml".to_string(), "*.yml".to_string()],
            debounce_ms: 500,
        }
    }

    /// Set watch paths
    pub fn with_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.paths = paths;
        self
    }

    /// Set file patterns
    pub fn with_patterns(mut self, patterns: Vec<String>) -> Self {
        self.patterns = patterns;
        self
    }

    /// Set debounce duration
    pub fn with_debounce(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Start watching for changes
    pub fn watch<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(WatchEvent) -> Result<()>,
    {
        info!("👁  Watching workflow files for changes...");
        info!("   Paths: {:?}", self.paths);
        info!("   Patterns: {:?}", self.patterns);
        info!("   Debounce: {}ms", self.debounce_ms);

        // Create channel for watch events
        let (tx, rx) = channel();

        // Create debounced file watcher
        let mut debouncer = new_debouncer(
            Duration::from_millis(self.debounce_ms),
            None,
            Self::create_event_handler(tx, self.patterns.clone()),
        )
        .context("Failed to create file watcher")?;

        // Add watch paths
        for path in &self.paths {
            if !path.exists() {
                warn!("Watch path does not exist: {:?}", path);
                continue;
            }

            debouncer
                .watcher()
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("Failed to watch path: {:?}", path))?;

            info!("Watching: {:?}", path);
        }

        // Process events in the main loop
        // The debouncer must be kept alive for the duration of the watch
        self.process_events(rx, callback, debouncer)?;

        Ok(())
    }

    /// Create the event handler for the debouncer
    fn create_event_handler(
        tx: Sender<WatchEvent>,
        patterns: Vec<String>,
    ) -> impl Fn(DebounceEventResult) {
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                for event in events {
                    // Filter by file patterns
                    for path in &event.paths {
                        if Self::matches_pattern_static(path, &patterns) {
                            // Convert notify event to our WatchEvent
                            let watch_event = match event.kind {
                                notify::EventKind::Create(_) => WatchEvent::Created(path.clone()),
                                notify::EventKind::Modify(_) => WatchEvent::Modified(path.clone()),
                                notify::EventKind::Remove(_) => WatchEvent::Deleted(path.clone()),
                                _ => continue, // Ignore other event types
                            };

                            debug!("File system event: {:?}", watch_event);

                            if let Err(e) = tx.send(watch_event) {
                                error!("Failed to send watch event: {}", e);
                            }
                        }
                    }
                }
            }
            Err(errors) => {
                for error in errors {
                    error!("Watch error: {:?}", error);
                }
            }
        }
    }

    /// Static version of matches_pattern for use in closures
    fn matches_pattern_static(path: &Path, patterns: &[String]) -> bool {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            patterns.iter().any(|pattern| {
                pattern
                    .trim_start_matches("*.")
                    .eq_ignore_ascii_case(&ext_str)
            })
        } else {
            false
        }
    }

    /// Process watch events
    fn process_events<F>(
        &self,
        rx: Receiver<WatchEvent>,
        mut callback: F,
        _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
    ) -> Result<()>
    where
        F: FnMut(WatchEvent) -> Result<()>,
    {
        // Keep the debouncer alive by holding it as a parameter
        // The underscore prefix indicates it's intentionally unused but necessary

        loop {
            match rx.recv() {
                Ok(event) => {
                    if let Err(e) = callback(event) {
                        error!("Error processing watch event: {}", e);
                        // Continue watching even if callback fails
                    }
                }
                Err(e) => {
                    // Channel closed - watcher is gone
                    error!("Watch channel closed: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Validate a workflow file
    pub fn validate_file(&self, path: &Path) -> Result<ValidationResult> {
        let schema = DeclarativeParser::parse_file(path.to_str().unwrap())?;

        let validators: Vec<Box<dyn Validator>> = vec![
            Box::new(SchemaValidator),
            Box::new(SemanticValidator),
            Box::new(DependencyValidator),
            Box::new(ResourceValidator),
        ];

        let composite = CompositeValidator::with_validators(validators);
        Ok(composite.validate(&schema))
    }

    /// Check if path matches patterns
    pub fn matches_pattern(&self, path: &Path) -> bool {
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
}

impl Default for WorkflowWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Watch event
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// File was created
    Created(PathBuf),

    /// File was modified
    Modified(PathBuf),

    /// File was deleted
    Deleted(PathBuf),
}

impl WatchEvent {
    /// Get the path from the event
    pub fn path(&self) -> &Path {
        match self {
            WatchEvent::Created(p) => p,
            WatchEvent::Modified(p) => p,
            WatchEvent::Deleted(p) => p,
        }
    }

    /// Get the event type as string
    pub fn event_type(&self) -> &str {
        match self {
            WatchEvent::Created(_) => "created",
            WatchEvent::Modified(_) => "modified",
            WatchEvent::Deleted(_) => "deleted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_creation() {
        let watcher = WorkflowWatcher::new();
        assert_eq!(watcher.debounce_ms, 500);
        assert_eq!(watcher.patterns.len(), 2);
    }

    #[test]
    fn test_watcher_configuration() {
        let watcher = WorkflowWatcher::new()
            .with_paths(vec![PathBuf::from("/workflows")])
            .with_patterns(vec!["*.yaml".to_string()])
            .with_debounce(1000);

        assert_eq!(watcher.paths.len(), 1);
        assert_eq!(watcher.patterns.len(), 1);
        assert_eq!(watcher.debounce_ms, 1000);
    }

    #[test]
    fn test_matches_pattern() {
        let watcher = WorkflowWatcher::new();

        assert!(watcher.matches_pattern(Path::new("workflow.yaml")));
        assert!(watcher.matches_pattern(Path::new("workflow.yml")));
        assert!(!watcher.matches_pattern(Path::new("README.md")));
    }

    #[test]
    fn test_watch_event() {
        let event = WatchEvent::Created(PathBuf::from("workflow.yaml"));
        assert_eq!(event.event_type(), "created");
        assert_eq!(event.path(), Path::new("workflow.yaml"));

        let event = WatchEvent::Modified(PathBuf::from("workflow.yaml"));
        assert_eq!(event.event_type(), "modified");

        let event = WatchEvent::Deleted(PathBuf::from("workflow.yaml"));
        assert_eq!(event.event_type(), "deleted");
    }
}
