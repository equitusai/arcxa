// graphica-core/src/inference/mapping/lexical.rs
//! Lexical similarity algorithms for field name matching

use std::cmp::{max, min};
use std::collections::HashSet;

/// Lexical similarity calculator using multiple string distance metrics
#[derive(Debug, Clone)]
pub struct LexicalSimilarity;

impl LexicalSimilarity {
    pub fn new() -> Self {
        Self
    }

    /// Calculate lexical similarity using multiple string metrics
    /// Returns a score from 0.0 (no match) to 1.0 (perfect match)
    pub fn compare(&self, source: &str, target: &str) -> f64 {
        // Normalize names for comparison
        let source_norm = self.normalize(source);
        let target_norm = self.normalize(target);

        // 1. Exact match after normalization
        if source_norm == target_norm {
            return 1.0;
        }

        // 2. Edit distance (Levenshtein)
        let edit_dist = self.levenshtein_distance(&source_norm, &target_norm);
        let max_len = source_norm.len().max(target_norm.len()) as f64;
        let edit_score = if max_len > 0.0 {
            1.0 - (edit_dist as f64 / max_len)
        } else {
            0.0
        };

        // 3. Jaro-Winkler distance
        let jaro_score = self.jaro_winkler(&source_norm, &target_norm);

        // 4. Token overlap (for compound names like "customer_id" vs "cust_identifier")
        let token_score = self.token_overlap(source, target);

        // 5. Longest common substring ratio
        let lcs_score = self.longest_common_substring_ratio(&source_norm, &target_norm);

        // Weighted average of all metrics
        edit_score * 0.25 + jaro_score * 0.35 + token_score * 0.25 + lcs_score * 0.15
    }

    /// Normalize column name (lowercase, remove separators)
    fn normalize(&self, name: &str) -> String {
        name.to_lowercase()
            .replace('_', "")
            .replace('-', "")
            .replace('.', "")
            .replace(' ', "")
    }

    /// Calculate Levenshtein (edit) distance between two strings
    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();

        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        // Initialize first row and column
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        // Fill matrix
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                    0
                } else {
                    1
                };

                matrix[i][j] = min(
                    min(
                        matrix[i - 1][j] + 1, // deletion
                        matrix[i][j - 1] + 1, // insertion
                    ),
                    matrix[i - 1][j - 1] + cost, // substitution
                );
            }
        }

        matrix[len1][len2]
    }

    /// Calculate Jaro-Winkler similarity
    fn jaro_winkler(&self, s1: &str, s2: &str) -> f64 {
        let jaro = self.jaro_similarity(s1, s2);

        // Winkler modification: bonus for common prefix
        let prefix_len = s1
            .chars()
            .zip(s2.chars())
            .take(4)
            .take_while(|(a, b)| a == b)
            .count() as f64;

        let p = 0.1; // Scaling factor (standard is 0.1)
        jaro + (prefix_len * p * (1.0 - jaro))
    }

    /// Calculate Jaro similarity
    fn jaro_similarity(&self, s1: &str, s2: &str) -> f64 {
        if s1.is_empty() && s2.is_empty() {
            return 1.0;
        }
        if s1.is_empty() || s2.is_empty() {
            return 0.0;
        }

        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();
        let len1 = s1_chars.len();
        let len2 = s2_chars.len();

        // Match window
        let match_distance = (max(len1, len2) / 2).saturating_sub(1);

        let mut s1_matches = vec![false; len1];
        let mut s2_matches = vec![false; len2];

        let mut matches = 0.0;
        let mut transpositions = 0.0;

        // Find matches
        for i in 0..len1 {
            let start = if i >= match_distance {
                i - match_distance
            } else {
                0
            };
            let end = min(i + match_distance + 1, len2);

            for j in start..end {
                if s2_matches[j] || s1_chars[i] != s2_chars[j] {
                    continue;
                }
                s1_matches[i] = true;
                s2_matches[j] = true;
                matches += 1.0;
                break;
            }
        }

        if matches == 0.0 {
            return 0.0;
        }

        // Count transpositions
        let mut k = 0;
        for i in 0..len1 {
            if !s1_matches[i] {
                continue;
            }
            while !s2_matches[k] {
                k += 1;
            }
            if s1_chars[i] != s2_chars[k] {
                transpositions += 1.0;
            }
            k += 1;
        }

        (matches / len1 as f64 + matches / len2 as f64 + (matches - transpositions / 2.0) / matches)
            / 3.0
    }

    /// Calculate token overlap (Jaccard similarity for compound names)
    fn token_overlap(&self, source: &str, target: &str) -> f64 {
        let source_tokens: HashSet<String> = self.tokenize(source);
        let target_tokens: HashSet<String> = self.tokenize(target);

        if source_tokens.is_empty() && target_tokens.is_empty() {
            return 1.0;
        }

        let intersection = source_tokens.intersection(&target_tokens).count();
        let union = source_tokens.union(&target_tokens).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Tokenize field name by common separators
    fn tokenize(&self, name: &str) -> HashSet<String> {
        name.split(|c| c == '_' || c == '-' || c == '.' || c == ' ')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Calculate longest common substring ratio
    fn longest_common_substring_ratio(&self, s1: &str, s2: &str) -> f64 {
        if s1.is_empty() && s2.is_empty() {
            return 1.0;
        }
        if s1.is_empty() || s2.is_empty() {
            return 0.0;
        }

        let lcs_len = self.longest_common_substring_length(s1, s2);
        let max_len = s1.len().max(s2.len()) as f64;

        lcs_len as f64 / max_len
    }

    /// Find length of longest common substring
    fn longest_common_substring_length(&self, s1: &str, s2: &str) -> usize {
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();
        let len1 = s1_chars.len();
        let len2 = s2_chars.len();

        if len1 == 0 || len2 == 0 {
            return 0;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
        let mut max_length = 0;

        for i in 1..=len1 {
            for j in 1..=len2 {
                if s1_chars[i - 1] == s2_chars[j - 1] {
                    matrix[i][j] = matrix[i - 1][j - 1] + 1;
                    max_length = max_length.max(matrix[i][j]);
                }
            }
        }

        max_length
    }
}

impl Default for LexicalSimilarity {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let lexical = LexicalSimilarity::new();
        let score = lexical.compare("customer_id", "customer_id");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalized_match() {
        let lexical = LexicalSimilarity::new();
        // Different separators but same content
        let score = lexical.compare("customer_id", "customer-id");
        assert!(score > 0.95);
    }

    #[test]
    fn test_abbreviation_match() {
        let lexical = LexicalSimilarity::new();
        // "cust_id" vs "customer_id" should have decent score
        let score = lexical.compare("cust_id", "customer_id");
        assert!(score > 0.6, "Score was {}", score);
    }

    #[test]
    fn test_token_overlap() {
        let lexical = LexicalSimilarity::new();
        // Both have "id" token in common
        let score = lexical.compare("customer_id", "cust_id");
        assert!(score > 0.5, "Score was {}", score);
    }

    #[test]
    fn test_completely_different() {
        let lexical = LexicalSimilarity::new();
        let score = lexical.compare("customer_id", "product_name");
        assert!(score < 0.5, "Score was {}", score);
    }

    #[test]
    fn test_levenshtein_distance() {
        let lexical = LexicalSimilarity::new();
        assert_eq!(lexical.levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(lexical.levenshtein_distance("saturday", "sunday"), 3);
        assert_eq!(lexical.levenshtein_distance("", "abc"), 3);
        assert_eq!(lexical.levenshtein_distance("abc", ""), 3);
    }

    #[test]
    fn test_jaro_winkler() {
        let lexical = LexicalSimilarity::new();
        // Common prefix should boost score
        let score = lexical.jaro_winkler("customer", "cust");
        assert!(score > 0.8, "Score was {}", score);
    }

    #[test]
    fn test_longest_common_substring() {
        let lexical = LexicalSimilarity::new();
        let len = lexical.longest_common_substring_length("customer", "cust");
        assert_eq!(len, 4); // "cust" is common
    }

    #[test]
    fn test_case_insensitive() {
        let lexical = LexicalSimilarity::new();
        let score = lexical.compare("Customer_ID", "customer_id");
        assert!((score - 1.0).abs() < 0.001);
    }
}
