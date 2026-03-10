//! Secret store provider implementations

pub mod aws;
pub mod env;
pub mod file;
pub mod inline;
pub mod registry;
pub mod vault;

pub use aws::AwsSecretsManagerStore;
pub use env::EnvSecretStore;
pub use file::FileSecretStore;
pub use inline::InlineSecretStore;
pub use registry::SecretStoreRegistry;
pub use vault::VaultSecretStore;
