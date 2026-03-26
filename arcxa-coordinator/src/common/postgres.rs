use anyhow::{anyhow, Result};

fn unquote_segment(segment: &str) -> &str {
    segment
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .unwrap_or(segment)
}

pub fn normalize_postgres_identifier_segment(segment: &str) -> Result<String> {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("PostgreSQL identifier segment cannot be empty"));
    }

    let normalized = unquote_segment(trimmed).replace("\"\"", "\"");
    if normalized.is_empty() {
        return Err(anyhow!("PostgreSQL identifier segment cannot be empty"));
    }

    if normalized
        .chars()
        .any(|ch| ch == '\0' || ch.is_ascii_control())
    {
        return Err(anyhow!(
            "PostgreSQL identifier segment contains unsupported control characters"
        ));
    }

    Ok(normalized)
}

pub fn quote_postgres_identifier_segment(segment: &str) -> Result<String> {
    let normalized = normalize_postgres_identifier_segment(segment)?;
    Ok(format!("\"{}\"", normalized.replace('"', "\"\"")))
}

pub fn quote_postgres_qualified_identifier(identifier: &str) -> Result<String> {
    let parts = identifier
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(quote_postgres_identifier_segment)
        .collect::<Result<Vec<_>>>()?;

    if parts.is_empty() {
        return Err(anyhow!("PostgreSQL identifier cannot be empty"));
    }

    Ok(parts.join("."))
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_postgres_identifier_segment, quote_postgres_identifier_segment,
        quote_postgres_qualified_identifier,
    };

    #[test]
    fn quotes_reserved_and_mixed_case_identifiers() {
        assert_eq!(
            quote_postgres_identifier_segment("User").unwrap(),
            "\"User\""
        );
        assert_eq!(
            quote_postgres_identifier_segment("order").unwrap(),
            "\"order\""
        );
    }

    #[test]
    fn normalizes_prequoted_identifiers() {
        assert_eq!(
            normalize_postgres_identifier_segment("\"CaseSensitive\"").unwrap(),
            "CaseSensitive"
        );
        assert_eq!(
            quote_postgres_qualified_identifier("public.\"User\"").unwrap(),
            "\"public\".\"User\""
        );
    }
}
