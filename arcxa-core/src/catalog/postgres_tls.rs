use anyhow::{anyhow, Context, Result};
use rustls::{ClientConfig, RootCertStore};
use tokio_postgres::{Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

pub fn ssl_mode_uses_tls(ssl_mode: Option<&str>) -> bool {
    match ssl_mode
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(|mode| mode.to_ascii_lowercase())
    {
        None => true,
        Some(mode) => mode != "disable",
    }
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

pub async fn connect_postgres_client(
    connection_string: &str,
    ssl_mode: Option<&str>,
) -> Result<Client> {
    if ssl_mode_uses_tls(ssl_mode) {
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
    } else {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
            .await
            .context("Failed to connect to PostgreSQL without TLS")?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::warn!("PostgreSQL connection error: {}", e);
            }
        });

        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_connection_string_ssl_mode, ssl_mode_uses_tls};

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
}
