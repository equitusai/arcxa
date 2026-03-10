// graphica-core/src/inference/mapping/vocabulary.rs
//! Domain vocabulary for recognizing common field name aliases

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Domain-specific vocabulary for common field name aliases
///
/// This provides a lightweight semantic layer that recognizes common abbreviations
/// and synonyms without requiring NLP models or embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainVocabulary {
    /// Map of canonical terms to their known aliases
    /// Example: "customer" -> ["cust", "client", "buyer", "user"]
    aliases: HashMap<String, Vec<String>>,
}

impl DomainVocabulary {
    /// Create a new empty domain vocabulary
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    /// Create domain vocabulary with default business terminology
    pub fn with_defaults() -> Self {
        let mut vocab = Self::new();

        // Customer aliases
        vocab.add_alias_group(
            "customer",
            vec!["cust", "client", "buyer", "user", "acct", "account"],
        );

        // Identifier aliases
        vocab.add_alias_group(
            "identifier",
            vec!["id", "key", "ref", "code", "num", "number"],
        );

        // Product aliases
        vocab.add_alias_group(
            "product",
            vec!["item", "sku", "article", "good", "merchandise"],
        );

        // Quantity aliases
        vocab.add_alias_group("quantity", vec!["qty", "amount", "count", "vol", "volume"]);

        // Date aliases
        vocab.add_alias_group("date", vec!["dt", "timestamp", "time", "when", "day"]);

        // Order aliases
        vocab.add_alias_group(
            "order",
            vec!["ord", "purchase", "transaction", "trans", "txn"],
        );

        // Address aliases
        vocab.add_alias_group("address", vec!["addr", "location", "loc", "place"]);

        // Email aliases
        vocab.add_alias_group("email", vec!["mail", "e-mail", "contact"]);

        // Phone aliases
        vocab.add_alias_group(
            "phone",
            vec!["tel", "telephone", "mobile", "cell", "contact"],
        );

        // Name aliases
        vocab.add_alias_group("name", vec!["nm", "title", "label", "description", "desc"]);

        // Price aliases
        vocab.add_alias_group(
            "price",
            vec!["cost", "amount", "amt", "rate", "fee", "charge"],
        );

        // Status aliases
        vocab.add_alias_group("status", vec!["state", "condition", "flag"]);

        // Type/Category aliases
        vocab.add_alias_group("type", vec!["category", "cat", "class", "kind", "group"]);

        // Total aliases
        vocab.add_alias_group("total", vec!["sum", "aggregate", "agg", "subtotal"]);

        // Invoice aliases
        vocab.add_alias_group("invoice", vec!["inv", "bill", "receipt"]);

        vocab
    }

    /// Add a group of related aliases
    ///
    /// # Example
    /// ```
    /// # use graphica_core::inference::mapping::vocabulary::DomainVocabulary;
    /// let mut vocab = DomainVocabulary::new();
    /// vocab.add_alias_group("customer", vec!["cust", "client", "buyer"]);
    /// ```
    pub fn add_alias_group(&mut self, canonical: &str, aliases: Vec<&str>) {
        let canonical_lower = canonical.to_lowercase();
        let aliases_owned: Vec<String> = aliases.iter().map(|s| s.to_lowercase()).collect();
        self.aliases.insert(canonical_lower, aliases_owned);
    }

    /// Check if two terms are aliases of each other
    /// Returns Some(score) if they match, None otherwise
    ///
    /// Score is 1.0 for exact canonical match, 0.95 for known alias match
    pub fn alias_match(&self, source: &str, target: &str) -> Option<f64> {
        let source_lower = source.to_lowercase();
        let target_lower = target.to_lowercase();

        // Check if they're exactly the same
        if source_lower == target_lower {
            return Some(1.0);
        }

        // Check each alias group
        for (canonical, aliases) in &self.aliases {
            // Check if source matches canonical or any alias
            let source_match = source_lower.contains(canonical)
                || aliases.iter().any(|a| source_lower.contains(a));

            // Check if target matches canonical or any alias
            let target_match = target_lower.contains(canonical)
                || aliases.iter().any(|a| target_lower.contains(a));

            if source_match && target_match {
                // Both contain the same canonical term or alias
                return Some(0.95);
            }
        }

        None
    }

    /// Get all known aliases for a term
    /// Returns aliases for the longest matching canonical term or alias
    pub fn get_aliases(&self, term: &str) -> Vec<String> {
        let term_lower = term.to_lowercase();

        // Split term into tokens by common delimiters
        let tokens: Vec<&str> = term_lower
            .split(|c: char| c == '_' || c == '-' || c == ' ' || c == '.')
            .filter(|t| !t.is_empty())
            .collect();

        let mut best_match: Option<(usize, bool, &String, &Vec<String>)> = None; // (length, is_exact_token_match, canonical, aliases)

        for (canonical, aliases) in &self.aliases {
            // First, check for exact token matches (highest priority)
            for token in &tokens {
                if *token == canonical.as_str() {
                    let match_len = canonical.len();
                    if best_match.is_none()
                        || (true && !best_match.unwrap().1)
                        || (true && match_len > best_match.unwrap().0)
                    {
                        best_match = Some((match_len, true, canonical, aliases));
                    }
                }

                // Check if token matches any alias exactly
                for alias in aliases {
                    if *token == alias.as_str() {
                        let match_len = alias.len();
                        if best_match.is_none()
                            || (true && !best_match.unwrap().1)
                            || (true && match_len > best_match.unwrap().0)
                        {
                            best_match = Some((match_len, true, canonical, aliases));
                        }
                    }
                }
            }

            // Fall back to substring matching if no exact token matches found
            if best_match.is_none() || !best_match.unwrap().1 {
                if term_lower.contains(canonical) {
                    let match_len = canonical.len();
                    if best_match.is_none()
                        || (!best_match.unwrap().1 && match_len > best_match.unwrap().0)
                    {
                        best_match = Some((match_len, false, canonical, aliases));
                    }
                }

                for alias in aliases {
                    if term_lower.contains(alias) {
                        let match_len = alias.len();
                        if best_match.is_none()
                            || (!best_match.unwrap().1 && match_len > best_match.unwrap().0)
                        {
                            best_match = Some((match_len, false, canonical, aliases));
                        }
                    }
                }
            }
        }

        if let Some((_, _, canonical, aliases)) = best_match {
            let mut result = vec![canonical.clone()];
            result.extend(aliases.clone());
            result
        } else {
            vec![]
        }
    }

    /// Get the canonical form of a term if it's a known alias
    /// Returns the canonical term for the longest match
    pub fn canonicalize(&self, term: &str) -> Option<String> {
        let term_lower = term.to_lowercase();

        // Split term into tokens by common delimiters
        let tokens: Vec<&str> = term_lower
            .split(|c: char| c == '_' || c == '-' || c == ' ' || c == '.')
            .filter(|t| !t.is_empty())
            .collect();

        let mut best_match: Option<(usize, bool, &String)> = None; // (length, is_exact_token_match, canonical)

        for (canonical, aliases) in &self.aliases {
            // First, check for exact token matches (highest priority)
            for token in &tokens {
                if *token == canonical.as_str() {
                    let match_len = canonical.len();
                    // Exact token match: prioritize over substring matches
                    if best_match.is_none() ||
                       (true && !best_match.unwrap().1) || // Exact beats substring
                       (true && match_len > best_match.unwrap().0)
                    {
                        // Longer exact beats shorter exact
                        best_match = Some((match_len, true, canonical));
                    }
                }

                // Check if token matches any alias exactly
                for alias in aliases {
                    if *token == alias.as_str() {
                        let match_len = alias.len();
                        if best_match.is_none()
                            || (true && !best_match.unwrap().1)
                            || (true && match_len > best_match.unwrap().0)
                        {
                            best_match = Some((match_len, true, canonical));
                        }
                    }
                }
            }

            // Fall back to substring matching if no exact token matches found
            if best_match.is_none() || !best_match.unwrap().1 {
                if term_lower.contains(canonical) {
                    let match_len = canonical.len();
                    if best_match.is_none()
                        || (!best_match.unwrap().1 && match_len > best_match.unwrap().0)
                    {
                        best_match = Some((match_len, false, canonical));
                    }
                }

                for alias in aliases {
                    if term_lower.contains(alias) {
                        let match_len = alias.len();
                        if best_match.is_none()
                            || (!best_match.unwrap().1 && match_len > best_match.unwrap().0)
                        {
                            best_match = Some((match_len, false, canonical));
                        }
                    }
                }
            }
        }

        best_match.map(|(_, _, canonical)| canonical.clone())
    }

    /// Get the number of alias groups
    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    /// Check if vocabulary is empty
    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

impl Default for DomainVocabulary {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let vocab = DomainVocabulary::with_defaults();
        let score = vocab.alias_match("customer_id", "customer_id");
        assert_eq!(score, Some(1.0), "Exact match should return 1.0");
    }

    #[test]
    fn test_customer_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        // customer vs cust
        assert_eq!(vocab.alias_match("customer_id", "cust_id"), Some(0.95));

        // customer vs client
        assert_eq!(
            vocab.alias_match("customer_name", "client_name"),
            Some(0.95)
        );

        // customer vs buyer
        assert_eq!(
            vocab.alias_match("customer_email", "buyer_email"),
            Some(0.95)
        );
    }

    #[test]
    fn test_identifier_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        // id vs identifier
        assert_eq!(
            vocab.alias_match("customer_id", "customer_identifier"),
            Some(0.95)
        );

        // id vs key
        assert_eq!(vocab.alias_match("product_id", "product_key"), Some(0.95));

        // id vs ref
        assert_eq!(vocab.alias_match("order_id", "order_ref"), Some(0.95));
    }

    #[test]
    fn test_product_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        // product vs item
        assert_eq!(vocab.alias_match("product_code", "item_code"), Some(0.95));

        // product vs sku
        assert_eq!(vocab.alias_match("product_id", "sku_id"), Some(0.95));
    }

    #[test]
    fn test_quantity_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        // quantity vs qty
        assert_eq!(
            vocab.alias_match("quantity_ordered", "qty_ordered"),
            Some(0.95)
        );

        // quantity vs amount
        assert_eq!(
            vocab.alias_match("quantity_shipped", "amount_shipped"),
            Some(0.95)
        );
    }

    #[test]
    fn test_date_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        // date vs dt
        assert_eq!(vocab.alias_match("order_date", "order_dt"), Some(0.95));

        // date vs timestamp
        assert_eq!(
            vocab.alias_match("created_date", "created_timestamp"),
            Some(0.95)
        );
    }

    #[test]
    fn test_no_match() {
        let vocab = DomainVocabulary::with_defaults();

        // Completely different terms
        assert_eq!(vocab.alias_match("customer_id", "product_name"), None);
        assert_eq!(vocab.alias_match("order_date", "email_address"), None);
    }

    #[test]
    fn test_case_insensitive() {
        let vocab = DomainVocabulary::with_defaults();

        assert_eq!(vocab.alias_match("CUSTOMER_ID", "cust_id"), Some(0.95));
        assert_eq!(
            vocab.alias_match("Customer_Name", "CLIENT_NAME"),
            Some(0.95)
        );
    }

    #[test]
    fn test_get_aliases() {
        let vocab = DomainVocabulary::with_defaults();

        let aliases = vocab.get_aliases("customer_id");
        assert!(aliases.contains(&"customer".to_string()));
        assert!(aliases.contains(&"cust".to_string()));
        assert!(aliases.contains(&"client".to_string()));
    }

    #[test]
    fn test_canonicalize() {
        let vocab = DomainVocabulary::with_defaults();

        assert_eq!(vocab.canonicalize("cust_id"), Some("customer".to_string()));
        assert_eq!(
            vocab.canonicalize("client_name"),
            Some("customer".to_string())
        );
        assert_eq!(
            vocab.canonicalize("qty_ordered"),
            Some("quantity".to_string())
        );
    }

    #[test]
    fn test_custom_vocabulary() {
        let mut vocab = DomainVocabulary::new();
        vocab.add_alias_group("organization", vec!["org", "company", "corp", "firm"]);

        assert_eq!(vocab.alias_match("organization_id", "org_id"), Some(0.95));
        assert_eq!(vocab.alias_match("company_name", "firm_name"), Some(0.95));
    }

    #[test]
    fn test_vocabulary_size() {
        let vocab = DomainVocabulary::with_defaults();
        assert!(vocab.len() >= 15, "Should have at least 15 alias groups");
        assert!(!vocab.is_empty());

        let empty_vocab = DomainVocabulary::new();
        assert_eq!(empty_vocab.len(), 0);
        assert!(empty_vocab.is_empty());
    }

    #[test]
    fn test_compound_field_names() {
        let vocab = DomainVocabulary::with_defaults();

        // customer_order_id should match cust_ord_id
        assert_eq!(
            vocab.alias_match("customer_order_id", "cust_order_id"),
            Some(0.95)
        );
        assert_eq!(
            vocab.alias_match("product_quantity", "item_qty"),
            Some(0.95)
        );
    }
}
