//! File-based secret store

use crate::secrets::{Secret, SecretError, SecretMetadata, SecretResult, SecretStore, SecretValue};
use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use std::fs;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
enum FileFormat {
    Json,
    Yaml,
}

impl FileFormat {
    fn from_str(input: &str) -> SecretResult<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            other => Err(SecretError::ConfigurationError(format!(
                "Unsupported file secret format '{}'",
                other
            ))),
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }

    fn serialize(&self, secret: &Secret) -> SecretResult<Vec<u8>> {
        match self {
            Self::Json => Ok(serde_json::to_vec_pretty(secret)?),
            Self::Yaml => {
                let text = serde_yaml::to_string(secret)
                    .map_err(|e| SecretError::SerializationError(e.to_string()))?;
                Ok(text.into_bytes())
            }
        }
    }

    fn deserialize(&self, bytes: &[u8]) -> SecretResult<Secret> {
        match self {
            Self::Json => Ok(serde_json::from_slice(bytes)?),
            Self::Yaml => serde_yaml::from_slice(bytes)
                .map_err(|e| SecretError::SerializationError(e.to_string())),
        }
    }
}

/// File-based secret store
///
/// Persists secrets to disk and supports versioning. Intended for
/// single-node or PVC-backed deployments (non-HSM).
pub struct FileSecretStore {
    base_dir: PathBuf,
    format: FileFormat,
    write_lock: Mutex<()>,
}

impl FileSecretStore {
    pub fn new() -> Self {
        Self::with_directory(PathBuf::from("./data/secrets"))
    }

    pub fn with_directory(directory: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: directory.into(),
            format: FileFormat::Json,
            write_lock: Mutex::new(()),
        }
    }

    pub fn with_directory_and_format(
        directory: impl Into<PathBuf>,
        format: &str,
    ) -> SecretResult<Self> {
        Ok(Self {
            base_dir: directory.into(),
            format: FileFormat::from_str(format)?,
            write_lock: Mutex::new(()),
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn ensure_base_dir(&self) -> SecretResult<()> {
        fs::create_dir_all(&self.base_dir)?;
        Ok(())
    }

    fn sanitize_path(&self, path: &str) -> SecretResult<PathBuf> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(SecretError::InvalidFormat(
                "Secret path cannot be empty".to_string(),
            ));
        }

        let mut relative = PathBuf::new();
        for component in Path::new(trimmed).components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir
                | Component::ParentDir
                | Component::RootDir
                | Component::Prefix(_) => {
                    return Err(SecretError::InvalidFormat(format!(
                        "Invalid secret path '{}'",
                        path
                    )));
                }
            }
        }

        if relative.as_os_str().is_empty() {
            return Err(SecretError::InvalidFormat(format!(
                "Invalid secret path '{}'",
                path
            )));
        }

        Ok(relative)
    }

    fn secret_dir(&self, path: &str) -> SecretResult<PathBuf> {
        let relative = self.sanitize_path(path)?;
        Ok(self.base_dir.join(relative))
    }

    fn versions_dir(&self, path: &str) -> SecretResult<PathBuf> {
        Ok(self.secret_dir(path)?.join("versions"))
    }

    fn latest_file(&self, path: &str) -> SecretResult<PathBuf> {
        Ok(self.secret_dir(path)?.join("latest"))
    }

    fn version_file(&self, path: &str, version: &str) -> SecretResult<PathBuf> {
        let filename = format!("{}.{}", version, self.format.extension());
        Ok(self.versions_dir(path)?.join(filename))
    }

    fn write_atomic(path: &Path, contents: &[u8]) -> SecretResult<()> {
        let parent = path.parent().ok_or_else(|| {
            SecretError::Internal("Secret file path missing parent directory".to_string())
        })?;
        fs::create_dir_all(parent)?;

        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SecretError::InvalidFormat("Invalid secret filename".to_string()))?;
        let tmp_name = format!(".{}.tmp-{}", filename, Uuid::new_v4());
        let tmp_path = parent.join(tmp_name);

        fs::write(&tmp_path, contents)?;
        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    fn read_latest_version(&self, path: &str) -> SecretResult<String> {
        let latest_file = self.latest_file(path)?;
        let version = fs::read_to_string(&latest_file)
            .map_err(|_| SecretError::NotFound(format!("Secret not found: {}", path)))?;
        let trimmed = version.trim();
        if trimmed.is_empty() {
            return Err(SecretError::NotFound(format!("Secret not found: {}", path)));
        }
        Ok(trimmed.to_string())
    }

    fn load_secret(&self, path: &str, version: &str) -> SecretResult<Secret> {
        let version_file = self.version_file(path, version)?;
        let bytes = fs::read(&version_file).map_err(|_| {
            SecretError::NotFound(format!(
                "Version {} not found for secret: {}",
                version, path
            ))
        })?;
        self.format.deserialize(&bytes)
    }

    fn list_versions(&self, path: &str) -> SecretResult<Vec<Secret>> {
        let versions_dir = self.versions_dir(path)?;
        let mut versions = Vec::new();

        if !versions_dir.exists() {
            return Ok(versions);
        }

        for entry in fs::read_dir(versions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext != self.format.extension())
                .unwrap_or(true)
            {
                continue;
            }
            let bytes = fs::read(&path)?;
            let secret = self.format.deserialize(&bytes)?;
            versions.push(secret);
        }

        Ok(versions)
    }

    fn to_secret_path(&self, path: &Path) -> SecretResult<String> {
        let relative = path
            .strip_prefix(&self.base_dir)
            .map_err(|_| SecretError::Internal("Failed to compute secret path".to_string()))?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
                _ => {
                    return Err(SecretError::InvalidFormat(
                        "Invalid secret path on disk".to_string(),
                    ))
                }
            }
        }
        Ok(parts.join("/"))
    }

    fn collect_secret_paths(&self, dir: &Path, results: &mut Vec<String>) -> SecretResult<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map(|n| n == "versions").unwrap_or(false) {
                    continue;
                }
                if path.join("latest").is_file() {
                    results.push(self.to_secret_path(&path)?);
                }
                self.collect_secret_paths(&path, results)?;
            }
        }
        Ok(())
    }
}

impl Default for FileSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for FileSecretStore {
    fn name(&self) -> &'static str {
        "file"
    }

    async fn put_secret(
        &self,
        path: &str,
        secret: SecretValue,
        metadata: Option<SecretMetadata>,
    ) -> SecretResult<String> {
        self.ensure_base_dir()?;
        let _guard = self.write_lock.lock();

        let version = Uuid::new_v4().to_string();
        let now = Utc::now();

        let secret_obj = Secret {
            path: path.to_string(),
            value: secret,
            metadata: metadata.unwrap_or_default(),
            version: version.clone(),
            created_at: now,
            updated_at: now,
        };

        let version_file = self.version_file(path, &version)?;
        let bytes = self.format.serialize(&secret_obj)?;
        Self::write_atomic(&version_file, &bytes)?;

        let latest_file = self.latest_file(path)?;
        Self::write_atomic(&latest_file, version.as_bytes())?;

        Ok(version)
    }

    async fn get_secret(&self, path: &str, version: Option<&str>) -> SecretResult<Secret> {
        self.ensure_base_dir()?;
        let version = match version {
            Some(ver) => ver.to_string(),
            None => self.read_latest_version(path)?,
        };
        self.load_secret(path, &version)
    }

    async fn delete_secret(&self, path: &str, version: Option<&str>) -> SecretResult<()> {
        self.ensure_base_dir()?;
        let _guard = self.write_lock.lock();

        if let Some(ver) = version {
            let version_file = self.version_file(path, ver)?;
            if version_file.exists() {
                fs::remove_file(&version_file)?;
            }

            let latest_file = self.latest_file(path)?;
            if latest_file.exists() {
                if let Ok(current) = fs::read_to_string(&latest_file) {
                    if current.trim() == ver {
                        let mut versions = self.list_versions(path)?;
                        versions.sort_by_key(|secret| secret.created_at);
                        if let Some(latest) = versions.last() {
                            Self::write_atomic(&latest_file, latest.version.as_bytes())?;
                        } else {
                            let _ = fs::remove_file(&latest_file);
                            let _ = fs::remove_dir_all(self.secret_dir(path)?);
                        }
                    }
                }
            }
            return Ok(());
        }

        let secret_dir = self.secret_dir(path)?;
        if secret_dir.exists() {
            fs::remove_dir_all(secret_dir)?;
        }
        Ok(())
    }

    async fn list_secrets(&self, prefix: Option<&str>) -> SecretResult<Vec<String>> {
        self.ensure_base_dir()?;
        let mut results = Vec::new();
        if self.base_dir.exists() {
            self.collect_secret_paths(&self.base_dir, &mut results)?;
        }
        if let Some(prefix) = prefix {
            results.retain(|path| path.starts_with(prefix));
        }
        results.sort();
        results.dedup();
        Ok(results)
    }

    async fn exists(&self, path: &str) -> SecretResult<bool> {
        self.ensure_base_dir()?;
        Ok(self.latest_file(path)?.is_file())
    }

    async fn rotate_secret(&self, path: &str, new_secret: SecretValue) -> SecretResult<String> {
        let metadata = self.get_metadata(path).await.ok();
        self.put_secret(path, new_secret, metadata).await
    }

    async fn get_metadata(&self, path: &str) -> SecretResult<SecretMetadata> {
        let secret = self.get_secret(path, None).await?;
        Ok(secret.metadata)
    }

    async fn health_check(&self) -> SecretResult<bool> {
        self.ensure_base_dir()?;
        let probe_path = self
            .base_dir
            .join(format!(".healthcheck-{}", Uuid::new_v4()));
        fs::write(&probe_path, b"ok")?;
        fs::remove_file(&probe_path)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_put_get_and_list() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileSecretStore::with_directory(temp_dir.path());

        let path = "prod/db/postgres";
        store
            .put_secret(path, SecretValue::from_credentials("user", "pass"), None)
            .await
            .unwrap();

        let secret = store.get_secret(path, None).await.unwrap();
        assert_eq!(secret.value.username(), Some("user"));
        assert_eq!(secret.value.password(), Some("pass"));

        let all = store.list_secrets(None).await.unwrap();
        assert_eq!(all, vec![path.to_string()]);
    }

    #[tokio::test]
    async fn test_file_versioning() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileSecretStore::with_directory(temp_dir.path());

        let path = "test/secret";
        let v1 = store
            .put_secret(path, SecretValue::from_string("v1"), None)
            .await
            .unwrap();
        let _v2 = store
            .put_secret(path, SecretValue::from_string("v2"), None)
            .await
            .unwrap();

        let latest = store.get_secret(path, None).await.unwrap();
        assert_eq!(latest.value.as_string(), Some("v2"));

        let first = store.get_secret(path, Some(&v1)).await.unwrap();
        assert_eq!(first.value.as_string(), Some("v1"));
    }
}
