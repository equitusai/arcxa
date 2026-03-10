//! Ontology Alignment with Model Service
//!
//! Uses the ML model service to align detected fields and types with custom ontologies.
//! Leverages embeddings for semantic similarity matching between field metadata and ontology concepts.

use super::embeddings::{cosine_similarity as emb_cosine_similarity, PretrainedEmbeddings};
use super::field::{SemanticType, UnifiedField};
use super::similarity_index::{CachedSimilarityIndex, IndexConfig};
use super::UnifiedSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for model service integration
#[derive(Debug, Clone)]
pub struct ModelServiceConfig {
    /// Model service endpoint URL
    pub endpoint: String,

    /// Model to use for embeddings (e.g., "minilm", "bert")
    pub model_name: String,

    /// Minimum similarity threshold for ontology matching (0.0 - 1.0)
    pub min_similarity: f64,

    /// Maximum number of ontology candidates to consider
    pub max_candidates: usize,

    /// Enable caching of embeddings
    pub enable_embedding_cache: bool,
}

impl Default for ModelServiceConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8001".to_string(),
            model_name: "minilm".to_string(),
            min_similarity: 0.7,
            max_candidates: 5,
            enable_embedding_cache: true,
        }
    }
}

/// Custom ontology concept
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyConcept {
    /// Unique identifier (URI)
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Description
    pub description: Option<String>,

    /// Concept type (Class, Property, etc.)
    pub concept_type: ConceptType,

    /// Parent concepts
    pub parents: Vec<String>,

    /// Synonyms and alternative labels
    pub synonyms: Vec<String>,

    /// Domain-specific metadata
    pub metadata: HashMap<String, String>,
}

/// Type of ontology concept
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConceptType {
    /// OWL/RDFS Class
    Class,
    /// OWL/RDF Property
    Property,
    /// Data type property
    DataProperty,
    /// Object property (relationships)
    ObjectProperty,
    /// Instance/Individual
    Instance,
}

/// Result of ontology alignment for a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentResult {
    /// Matched ontology concept
    pub concept: OntologyConcept,

    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,

    /// Alignment method used
    pub method: AlignmentMethod,

    /// Confidence in the alignment
    pub confidence: f64,
}

/// Method used for alignment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlignmentMethod {
    /// Embedding-based semantic similarity
    EmbeddingSimilarity,
    /// Exact label match
    ExactMatch,
    /// Synonym match
    SynonymMatch,
    /// Pattern-based match
    PatternMatch,
    /// Combined methods
    Hybrid,
}

/// Ontology alignment engine
pub struct OntologyAligner {
    /// Configuration
    config: ModelServiceConfig,

    /// Loaded ontology concepts
    concepts: Vec<OntologyConcept>,

    /// Embedding generator (pretrained)
    embedder: Arc<PretrainedEmbeddings>,

    /// Similarity index for fast nearest neighbor search
    similarity_index: Option<CachedSimilarityIndex>,

    /// Concept embeddings (in order of concepts vector)
    concept_embeddings: Vec<Vec<f32>>,
}

impl OntologyAligner {
    /// Create a new ontology aligner
    pub fn new(config: ModelServiceConfig) -> Self {
        Self {
            config,
            concepts: Vec::new(),
            embedder: Arc::new(PretrainedEmbeddings::new()),
            similarity_index: None,
            concept_embeddings: Vec::new(),
        }
    }

    /// Load ontology concepts from a custom ontology
    pub fn load_ontology(&mut self, concepts: Vec<OntologyConcept>) {
        self.concepts = concepts;

        // Generate embeddings for all concepts
        self.concept_embeddings = self
            .concepts
            .iter()
            .map(|concept| {
                let context = Self::build_concept_context(concept);
                self.embedder.embed(&context)
            })
            .collect();

        // Build similarity index
        let metadata: Vec<Option<String>> =
            self.concepts.iter().map(|c| Some(c.uri.clone())).collect();

        let index_config = IndexConfig {
            k: self.config.max_candidates,
            min_similarity: self.config.min_similarity as f32,
            use_hnsw: false, // Brute force is fast enough for small ontologies
            ..Default::default()
        };

        self.similarity_index = Some(CachedSimilarityIndex::new(
            self.concept_embeddings.clone(),
            metadata,
            index_config,
        ));
    }

    /// Build context string for concept embedding
    fn build_concept_context(concept: &OntologyConcept) -> String {
        let mut context = concept.label.clone();

        if let Some(ref desc) = concept.description {
            context.push_str(" - ");
            context.push_str(desc);
        }

        // Add synonyms
        if !concept.synonyms.is_empty() {
            context.push_str(" (");
            context.push_str(&concept.synonyms.join(", "));
            context.push(')');
        }

        context
    }

    /// Align a field to ontology concepts using the model service
    pub async fn align_field(&mut self, field: &UnifiedField) -> Vec<AlignmentResult> {
        let mut results = Vec::new();

        // 1. Try exact label matching first (fast path)
        if let Some(exact_match) = self.find_exact_match(&field.name) {
            results.push(AlignmentResult {
                concept: exact_match,
                similarity: 1.0,
                method: AlignmentMethod::ExactMatch,
                confidence: 1.0,
            });
            return results;
        }

        // 2. Try synonym matching
        if let Some(synonym_match) = self.find_synonym_match(&field.name) {
            results.push(AlignmentResult {
                concept: synonym_match,
                similarity: 0.95,
                method: AlignmentMethod::SynonymMatch,
                confidence: 0.95,
            });
        }

        // 3. Use model service for embedding-based similarity
        if let Ok(embedding_results) = self.find_similar_concepts_by_embedding(field).await {
            results.extend(embedding_results);
        }

        // Sort by similarity (highest first)
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        // Filter by minimum similarity and take top candidates
        results
            .into_iter()
            .filter(|r| r.similarity >= self.config.min_similarity)
            .take(self.config.max_candidates)
            .collect()
    }

    /// Align an entire schema to the ontology
    pub async fn align_schema(
        &mut self,
        schema: &UnifiedSchema,
    ) -> HashMap<String, Vec<AlignmentResult>> {
        let mut alignments = HashMap::new();

        for field in &schema.fields {
            let field_alignments = self.align_field(field).await;
            if !field_alignments.is_empty() {
                alignments.insert(field.name.clone(), field_alignments);
            }
        }

        alignments
    }

    /// Find exact label match
    pub fn find_exact_match(&self, field_name: &str) -> Option<OntologyConcept> {
        let normalized = field_name.to_lowercase().replace('_', " ");

        self.concepts
            .iter()
            .find(|c| c.label.to_lowercase() == normalized)
            .cloned()
    }

    /// Find synonym match
    pub fn find_synonym_match(&self, field_name: &str) -> Option<OntologyConcept> {
        let normalized = field_name.to_lowercase().replace('_', " ");

        self.concepts
            .iter()
            .find(|c| {
                c.synonyms
                    .iter()
                    .any(|syn| syn.to_lowercase().replace('_', " ") == normalized)
            })
            .cloned()
    }

    /// Find similar concepts using embeddings from model service
    async fn find_similar_concepts_by_embedding(
        &mut self,
        field: &UnifiedField,
    ) -> Result<Vec<AlignmentResult>, String> {
        // Build field context for embedding
        let field_context = self.build_field_context(field);

        // Get embedding for field using pretrained embedder
        let field_embedding = self.embedder.embed(&field_context);

        // Use similarity index for fast search
        if let Some(ref index) = self.similarity_index {
            let matches = index.search(&field_embedding);

            let results = matches
                .into_iter()
                .map(|m| AlignmentResult {
                    concept: self.concepts[m.index].clone(),
                    similarity: m.similarity as f64,
                    method: AlignmentMethod::EmbeddingSimilarity,
                    confidence: (m.similarity as f64) * 0.9, // Slightly lower confidence for ML-based matching
                })
                .collect();

            Ok(results)
        } else {
            // Fallback: Brute force search if index not built
            let mut results = Vec::new();

            for (idx, concept_emb) in self.concept_embeddings.iter().enumerate() {
                let similarity = emb_cosine_similarity(&field_embedding, concept_emb);

                if similarity >= self.config.min_similarity as f32 {
                    results.push(AlignmentResult {
                        concept: self.concepts[idx].clone(),
                        similarity: similarity as f64,
                        method: AlignmentMethod::EmbeddingSimilarity,
                        confidence: (similarity as f64) * 0.9,
                    });
                }
            }

            // Sort by similarity (descending)
            results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
            results.truncate(self.config.max_candidates);

            Ok(results)
        }
    }

    /// Build context string for field embedding
    fn build_field_context(&self, field: &UnifiedField) -> String {
        let mut context = field.name.clone();

        // Add semantic type if available
        if let Some(ref semantic) = field.semantic.semantic_type {
            context.push_str(&format!(" {}", Self::semantic_type_to_string(semantic)));
        }

        // Add data type
        context.push_str(&format!(" {}", field.data_type));

        // Add sample values if available
        if let Some(ref profile) = field.profile {
            if !profile.samples.is_empty() {
                let samples: Vec<&str> =
                    profile.samples.iter().map(|s| s.as_str()).take(3).collect();
                context.push_str(&format!(" examples: {}", samples.join(", ")));
            }
        }

        context
    }

    /// Convert SemanticType to string
    fn semantic_type_to_string(semantic_type: &SemanticType) -> String {
        match semantic_type {
            SemanticType::Email => "email address".to_string(),
            SemanticType::PhoneNumber => "phone number".to_string(),
            SemanticType::FirstName => "first name".to_string(),
            SemanticType::LastName => "last name".to_string(),
            SemanticType::FullName => "full name".to_string(),
            SemanticType::CustomerId => "customer identifier".to_string(),
            SemanticType::OrderNumber => "order number".to_string(),
            SemanticType::Custom(s) => s.clone(),
            _ => format!("{:?}", semantic_type).to_lowercase(),
        }
    }

    /// Get all loaded concepts
    pub fn concepts(&self) -> &[OntologyConcept] {
        &self.concepts
    }

    /// Find concepts by type
    pub fn find_concepts_by_type(&self, concept_type: ConceptType) -> Vec<&OntologyConcept> {
        self.concepts
            .iter()
            .filter(|c| c.concept_type == concept_type)
            .collect()
    }

    /// Map alignment result to semantic type
    pub fn alignment_to_semantic_type(alignment: &AlignmentResult) -> Option<SemanticType> {
        // Map ontology concept to SemanticType
        // This is domain-specific and should be customizable

        let label = alignment.concept.label.to_lowercase();

        if label.contains("email") {
            Some(SemanticType::Email)
        } else if label.contains("phone") || label.contains("telephone") {
            Some(SemanticType::PhoneNumber)
        } else if label.contains("first") && label.contains("name") {
            Some(SemanticType::FirstName)
        } else if label.contains("last") && label.contains("name") {
            Some(SemanticType::LastName)
        } else if label.contains("customer")
            && (label.contains("id") || label.contains("identifier"))
        {
            Some(SemanticType::CustomerId)
        } else if label.contains("order") && (label.contains("number") || label.contains("id")) {
            Some(SemanticType::OrderNumber)
        } else {
            // Create custom semantic type from ontology concept
            Some(SemanticType::Custom(alignment.concept.label.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_concept(label: &str, uri: &str) -> OntologyConcept {
        OntologyConcept {
            uri: uri.to_string(),
            label: label.to_string(),
            description: Some(format!("Description of {}", label)),
            concept_type: ConceptType::Property,
            parents: vec![],
            synonyms: vec![],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_ontology_aligner_creation() {
        let config = ModelServiceConfig::default();
        let aligner = OntologyAligner::new(config);
        assert_eq!(aligner.concepts().len(), 0);
    }

    #[test]
    fn test_load_ontology() {
        let config = ModelServiceConfig::default();
        let mut aligner = OntologyAligner::new(config);

        let concepts = vec![
            create_test_concept("Customer Email", "http://example.org/email"),
            create_test_concept("Customer Phone", "http://example.org/phone"),
        ];

        aligner.load_ontology(concepts);
        assert_eq!(aligner.concepts().len(), 2);
    }

    #[test]
    fn test_exact_label_match() {
        let config = ModelServiceConfig::default();
        let mut aligner = OntologyAligner::new(config);

        let concepts = vec![create_test_concept(
            "customer email",
            "http://example.org/email",
        )];

        aligner.load_ontology(concepts);

        let result = aligner.find_exact_match("customer_email");
        assert!(result.is_some());
        assert_eq!(result.unwrap().uri, "http://example.org/email");
    }

    #[test]
    fn test_synonym_match() {
        let config = ModelServiceConfig::default();
        let mut aligner = OntologyAligner::new(config);

        let mut concept = create_test_concept("Customer Email", "http://example.org/email");
        concept.synonyms = vec!["email address".to_string(), "e-mail".to_string()];

        aligner.load_ontology(vec![concept]);

        let result = aligner.find_synonym_match("email_address");
        assert!(result.is_some());
    }

    #[test]
    fn test_find_concepts_by_type() {
        let config = ModelServiceConfig::default();
        let mut aligner = OntologyAligner::new(config);

        let mut concepts = vec![
            create_test_concept("Customer", "http://example.org/Customer"),
            create_test_concept("Order", "http://example.org/Order"),
            create_test_concept("email", "http://example.org/email"),
        ];

        concepts[0].concept_type = ConceptType::Class;
        concepts[1].concept_type = ConceptType::Class;
        concepts[2].concept_type = ConceptType::Property;

        aligner.load_ontology(concepts);

        let classes = aligner.find_concepts_by_type(ConceptType::Class);
        assert_eq!(classes.len(), 2);

        let properties = aligner.find_concepts_by_type(ConceptType::Property);
        assert_eq!(properties.len(), 1);
    }

    #[test]
    fn test_cosine_similarity() {
        use super::emb_cosine_similarity;

        let vec1 = vec![1.0, 0.0, 0.0];
        let vec2 = vec![1.0, 0.0, 0.0];
        assert!((emb_cosine_similarity(&vec1, &vec2) - 1.0).abs() < 0.001);

        let vec3 = vec![1.0, 0.0, 0.0];
        let vec4 = vec![0.0, 1.0, 0.0];
        assert!((emb_cosine_similarity(&vec3, &vec4) - 0.0).abs() < 0.001);

        let vec5 = vec![1.0, 1.0, 0.0];
        let vec6 = vec![1.0, 0.0, 0.0];
        let sim = emb_cosine_similarity(&vec5, &vec6);
        assert!(sim > 0.7 && sim < 0.8);
    }

    #[test]
    fn test_alignment_to_semantic_type() {
        let concept = create_test_concept("Customer Email", "http://example.org/email");
        let alignment = AlignmentResult {
            concept,
            similarity: 0.95,
            method: AlignmentMethod::ExactMatch,
            confidence: 0.95,
        };

        let semantic_type = OntologyAligner::alignment_to_semantic_type(&alignment);
        assert_eq!(semantic_type, Some(SemanticType::Email));
    }

    #[test]
    fn test_semantic_type_to_string() {
        assert_eq!(
            OntologyAligner::semantic_type_to_string(&SemanticType::Email),
            "email address"
        );
        assert_eq!(
            OntologyAligner::semantic_type_to_string(&SemanticType::PhoneNumber),
            "phone number"
        );
        assert_eq!(
            OntologyAligner::semantic_type_to_string(&SemanticType::Custom("MyType".to_string())),
            "MyType"
        );
    }
}
