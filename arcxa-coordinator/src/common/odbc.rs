use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "odbc")]
use odbc_api::parameter::InputParameter;

pub fn rewrite_named_parameters(query: &str) -> (String, Vec<String>) {
    let mut rewritten = String::with_capacity(query.len());
    let mut ordered = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < chars.len() {
        let current = chars[i];

        if in_single_quote {
            rewritten.push(current);
            if current == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    rewritten.push('\'');
                    i += 2;
                    continue;
                }
                in_single_quote = false;
            }
            i += 1;
            continue;
        }

        if in_double_quote {
            rewritten.push(current);
            if current == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    rewritten.push('"');
                    i += 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += 1;
            continue;
        }

        match current {
            '\'' => {
                in_single_quote = true;
                rewritten.push(current);
                i += 1;
            }
            '"' => {
                in_double_quote = true;
                rewritten.push(current);
                i += 1;
            }
            ':' => {
                let Some(next) = chars.get(i + 1).copied() else {
                    rewritten.push(current);
                    i += 1;
                    continue;
                };

                if !matches!(next, 'A'..='Z' | 'a'..='z' | '_') {
                    rewritten.push(current);
                    i += 1;
                    continue;
                }

                let mut j = i + 1;
                while let Some(ch) = chars.get(j).copied() {
                    if matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_') {
                        j += 1;
                    } else {
                        break;
                    }
                }

                ordered.push(chars[i + 1..j].iter().collect());
                rewritten.push('?');
                i = j;
            }
            _ => {
                rewritten.push(current);
                i += 1;
            }
        }
    }

    (rewritten, ordered)
}

#[cfg(feature = "odbc")]
pub fn build_named_parameters(
    parameters: &HashMap<String, Value>,
    ordered_names: &[String],
) -> Result<Vec<Box<dyn InputParameter>>> {
    let mut bound = Vec::with_capacity(ordered_names.len());
    for name in ordered_names {
        let value = parameters
            .get(name)
            .ok_or_else(|| anyhow!("Missing value for named parameter :{}", name))?;
        bound.push(json_value_to_parameter(Some(value)));
    }
    Ok(bound)
}

#[cfg(feature = "odbc")]
pub fn json_value_to_parameter(value: Option<&Value>) -> Box<dyn InputParameter> {
    use odbc_api::IntoParameter;

    match value {
        None | Some(Value::Null) => Box::new(Option::<String>::None.into_parameter()),
        Some(Value::Bool(flag)) => Box::new(if *flag { 1_i16 } else { 0_i16 }),
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_i64() {
                Box::new(value)
            } else if let Some(value) = number.as_u64() {
                if let Ok(signed) = i64::try_from(value) {
                    Box::new(signed)
                } else {
                    Box::new(value.to_string().into_parameter())
                }
            } else if let Some(value) = number.as_f64() {
                Box::new(value)
            } else {
                Box::new(number.to_string().into_parameter())
            }
        }
        Some(Value::String(text)) => Box::new(text.clone().into_parameter()),
        Some(other) => Box::new(other.to_string().into_parameter()),
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_named_parameters;

    #[test]
    fn rewrites_named_parameters_outside_quotes() {
        let (query, ordered) = rewrite_named_parameters(
            "SELECT * FROM customers WHERE updated_at > :last_value AND note = ':literal' AND owner = :owner",
        );

        assert_eq!(
            query,
            "SELECT * FROM customers WHERE updated_at > ? AND note = ':literal' AND owner = ?"
        );
        assert_eq!(ordered, vec!["last_value".to_string(), "owner".to_string()]);
    }

    #[test]
    fn leaves_double_quoted_identifiers_untouched() {
        let (query, ordered) =
            rewrite_named_parameters("SELECT \"weird:column\" FROM dual WHERE id = :id");

        assert_eq!(query, "SELECT \"weird:column\" FROM dual WHERE id = ?");
        assert_eq!(ordered, vec!["id".to_string()]);
    }
}
