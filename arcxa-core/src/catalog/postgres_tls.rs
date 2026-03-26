use anyhow::{anyhow, Context, Result};
use rustls::{ClientConfig, RootCertStore};
use tokio_postgres::{Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostgresSslBehavior {
    Disable,
    Prefer,
    Require,
}

pub fn postgres_ssl_behavior(ssl_mode: Option<&str>) -> PostgresSslBehavior {
    match ssl_mode
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(|mode| mode.to_ascii_lowercase())
        .as_deref()
    {
        Some("disable") => PostgresSslBehavior::Disable,
        Some("prefer") | None => PostgresSslBehavior::Prefer,
        Some("require") | Some("verify-ca") | Some("verify-full") => PostgresSslBehavior::Require,
        Some(_) => PostgresSslBehavior::Require,
    }
}

pub fn ssl_mode_uses_tls(ssl_mode: Option<&str>) -> bool {
    !matches!(
        postgres_ssl_behavior(ssl_mode),
        PostgresSslBehavior::Disable
    )
}

pub fn parse_connection_string_ssl_mode(connection_string: &str) -> Option<String> {
    connection_string.split_whitespace().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key.eq_ignore_ascii_case("sslmode") && !value.is_empty() {
            Some(value.to_string())
        } else {
            None
        }
    })
}

pub fn make_rustls_connector() -> Result<MakeRustlsConnect> {
    let native_certs = rustls_native_certs::load_native_certs();
    let mut roots = RootCertStore::empty();

    let (added, ignored) = roots.add_parsable_certificates(native_certs.certs);

    for err in native_certs.errors {
        tracing::warn!("Ignoring native certificate load error: {:?}", err);
    }

    if added == 0 {
        return Err(anyhow!(
            "No native root certificates available for PostgreSQL TLS"
        ));
    }

    if ignored > 0 {
        tracing::warn!(
            "Ignored {} malformed native root certificates while configuring PostgreSQL TLS",
            ignored
        );
    }

    let tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok(MakeRustlsConnect::new(tls_config))
}

fn strip_ssl_mode(connection_string: &str) -> String {
    connection_string
        .split_whitespace()
        .filter(|part| {
            part.split_once('=')
                .map(|(key, _)| !key.eq_ignore_ascii_case("sslmode"))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn connect_with_tls(connection_string: &str) -> Result<Client> {
    let tls = make_rustls_connector()?;
    let (client, connection) = tokio_postgres::connect(connection_string, tls)
        .await
        .context("Failed to connect to PostgreSQL over TLS")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!("PostgreSQL TLS connection error: {}", e);
        }
    });

    Ok(client)
}

async fn connect_without_tls(connection_string: &str) -> Result<Client> {
    let no_tls_connection_string = strip_ssl_mode(connection_string);
    let (client, connection) = tokio_postgres::connect(&no_tls_connection_string, NoTls)
        .await
        .context("Failed to connect to PostgreSQL without TLS")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!("PostgreSQL connection error: {}", e);
        }
    });

    Ok(client)
}

pub async fn connect_postgres_client_with_transport(
    connection_string: &str,
    ssl_mode: Option<&str>,
) -> Result<(Client, bool)> {
    match postgres_ssl_behavior(ssl_mode) {
        PostgresSslBehavior::Disable => connect_without_tls(connection_string)
            .await
            .map(|client| (client, false)),
        PostgresSslBehavior::Prefer => match connect_with_tls(connection_string).await {
            Ok(client) => Ok((client, true)),
            Err(error) => {
                tracing::warn!(
                    "PostgreSQL TLS connection failed with sslmode=prefer, falling back to non-TLS: {}",
                    error
                );
                connect_without_tls(connection_string)
                    .await
                    .map(|client| (client, false))
            }
        },
        PostgresSslBehavior::Require => connect_with_tls(connection_string)
            .await
            .map(|client| (client, true)),
    }
}

pub async fn connect_postgres_client(
    connection_string: &str,
    ssl_mode: Option<&str>,
) -> Result<Client> {
    connect_postgres_client_with_transport(connection_string, ssl_mode)
        .await
        .map(|(client, _)| client)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_connection_string_ssl_mode, postgres_ssl_behavior, ssl_mode_uses_tls,
        PostgresSslBehavior,
    };

    #[test]
    fn parses_sslmode_from_connection_string() {
        let ssl_mode = parse_connection_string_ssl_mode(
            "host=localhost dbname=test user=postgres sslmode=require",
        );

        assert_eq!(ssl_mode.as_deref(), Some("require"));
    }

    #[test]
    fn defaults_to_tls_when_ssl_mode_missing() {
        assert!(ssl_mode_uses_tls(None));
        assert!(ssl_mode_uses_tls(Some("prefer")));
        assert!(ssl_mode_uses_tls(Some("require")));
        assert!(!ssl_mode_uses_tls(Some("disable")));
    }

    #[test]
    fn models_sslmode_prefer_separately() {
        assert_eq!(postgres_ssl_behavior(None), PostgresSslBehavior::Prefer);
        assert_eq!(
            postgres_ssl_behavior(Some("prefer")),
            PostgresSslBehavior::Prefer
        );
        assert_eq!(
            postgres_ssl_behavior(Some("require")),
            PostgresSslBehavior::Require
        );
        assert_eq!(
            postgres_ssl_behavior(Some("verify-full")),
            PostgresSslBehavior::Require
        );
        assert_eq!(
            postgres_ssl_behavior(Some("disable")),
            PostgresSslBehavior::Disable
        );
    }
}
