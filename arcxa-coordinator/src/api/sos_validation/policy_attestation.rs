use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::api::sos_validation::storage::{PolicyAttestationRecord, SosPolicy};

pub(crate) const POLICY_ATTESTATION_ALGORITHM: &str = "ed25519";
pub(crate) const POLICY_ATTESTATION_HASH_ALGORITHM: &str = "sha256";
pub(crate) const POLICY_ATTESTATION_PAYLOAD_VERSION: u32 = 1;
pub(crate) const POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY: &str = "trust_mode";
pub(crate) const POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY: &str = "trust_provider";
pub(crate) const POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY: &str = "external_key_ref";
pub(crate) const POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY: &str =
    "trust_attestation_ref";
pub(crate) const POLICY_ATTESTATION_DEFAULT_TRUST_MODE: &str = "software";

pub struct PolicyAttestationSigningMaterial {
    pub signing_key: SigningKey,
    pub signing_key_ref: Option<String>,
    pub signing_key_version: Option<String>,
    pub signing_key_source: String,
    pub metadata: HashMap<String, Value>,
}

pub(crate) fn policy_signing_key_public_material(signing_key: &SigningKey) -> (String, String) {
    let public_key = signing_key.verifying_key().to_bytes();
    (
        BASE64_STANDARD.encode(public_key),
        sha256_prefixed(public_key),
    )
}

pub(crate) fn build_policy_attestation_record(
    policy: &SosPolicy,
    signing_material: &PolicyAttestationSigningMaterial,
    approval_request_id: Option<String>,
    mut evidence_ids: Vec<String>,
    mut policy_refs: Vec<String>,
) -> Result<PolicyAttestationRecord> {
    let attested_by = policy
        .approved_by
        .clone()
        .ok_or_else(|| anyhow!("Approved policy is missing approved_by"))?;
    let attested_at = policy
        .approved_at
        .ok_or_else(|| anyhow!("Approved policy is missing approved_at"))?;

    evidence_ids.sort();
    evidence_ids.dedup();
    policy_refs.sort();
    policy_refs.dedup();

    let payload = build_policy_attestation_payload(policy)?;
    let payload_hash = sha256_prefixed(&payload);
    let signature = signing_material.signing_key.sign(&payload);
    let (public_key, key_fingerprint) =
        policy_signing_key_public_material(&signing_material.signing_key);

    let mut metadata = signing_material.metadata.clone();
    metadata.insert(
        "payload_version".to_string(),
        Value::from(POLICY_ATTESTATION_PAYLOAD_VERSION),
    );

    Ok(PolicyAttestationRecord {
        attestation_id: uuid::Uuid::new_v4().to_string(),
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.revision,
        policy_revision_ref: policy_revision_ref(policy),
        payload_hash,
        payload_hash_algorithm: POLICY_ATTESTATION_HASH_ALGORITHM.to_string(),
        signature_algorithm: POLICY_ATTESTATION_ALGORITHM.to_string(),
        signature: BASE64_STANDARD.encode(signature.to_bytes()),
        public_key,
        key_fingerprint,
        signing_key_ref: signing_material.signing_key_ref.clone(),
        signing_key_version: signing_material.signing_key_version.clone(),
        signing_key_source: signing_material.signing_key_source.clone(),
        attested_by,
        attested_at,
        approval_request_id,
        evidence_ids,
        policy_refs,
        metadata,
    })
}

pub(crate) fn verify_policy_attestation(
    policy: &SosPolicy,
    attestation: &PolicyAttestationRecord,
) -> bool {
    if attestation.policy_id != policy.policy_id || attestation.policy_revision != policy.revision {
        return false;
    }

    if attestation.signature_algorithm != POLICY_ATTESTATION_ALGORITHM
        || attestation.payload_hash_algorithm != POLICY_ATTESTATION_HASH_ALGORITHM
    {
        return false;
    }

    let payload = match build_policy_attestation_payload(policy) {
        Ok(payload) => payload,
        Err(_) => return false,
    };
    if attestation.payload_hash != sha256_prefixed(&payload) {
        return false;
    }

    let public_key = match BASE64_STANDARD.decode(&attestation.public_key) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key_bytes: [u8; 32] = match public_key.as_slice().try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    if attestation.key_fingerprint != sha256_prefixed(&public_key_bytes) {
        return false;
    }

    let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let signature_bytes = match BASE64_STANDARD.decode(&attestation.signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature_bytes: [u8; 64] = match signature_bytes.as_slice().try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = Signature::from_bytes(&signature_bytes);

    verifying_key.verify(&payload, &signature).is_ok()
}

fn build_policy_attestation_payload(policy: &SosPolicy) -> Result<Vec<u8>> {
    let value = Value::Object(Map::from_iter([
        (
            "payload_version".to_string(),
            Value::from(POLICY_ATTESTATION_PAYLOAD_VERSION),
        ),
        (
            "policy_id".to_string(),
            Value::String(policy.policy_id.clone()),
        ),
        ("policy_revision".to_string(), Value::from(policy.revision)),
        (
            "policy_revision_ref".to_string(),
            Value::String(policy_revision_ref(policy)),
        ),
        (
            "policy_name".to_string(),
            Value::String(policy.policy_name.clone()),
        ),
        (
            "description".to_string(),
            serde_json::to_value(&policy.description)
                .context("Failed to serialize policy description into attestation payload")?,
        ),
        (
            "target_type".to_string(),
            Value::String(policy.target_type.clone()),
        ),
        (
            "target_key".to_string(),
            serde_json::to_value(&policy.target_key)
                .context("Failed to serialize policy target_key into attestation payload")?,
        ),
        (
            "stages".to_string(),
            serde_json::to_value(&policy.stages)
                .context("Failed to serialize policy stages into attestation payload")?,
        ),
        (
            "enforcement_level".to_string(),
            Value::String(policy.enforcement_level.clone()),
        ),
        (
            "severity".to_string(),
            Value::String(policy.severity.clone()),
        ),
        (
            "sparql_query".to_string(),
            Value::String(policy.sparql_query.clone()),
        ),
        (
            "context".to_string(),
            canonicalize_json(
                &serde_json::to_value(&policy.context)
                    .context("Failed to serialize policy context into attestation payload")?,
            ),
        ),
        (
            "tags".to_string(),
            serde_json::to_value(&policy.tags)
                .context("Failed to serialize policy tags into attestation payload")?,
        ),
        (
            "ontology_refs".to_string(),
            serde_json::to_value(&policy.ontology_refs)
                .context("Failed to serialize policy ontology_refs into attestation payload")?,
        ),
        (
            "shape_refs".to_string(),
            serde_json::to_value(&policy.shape_refs)
                .context("Failed to serialize policy shape_refs into attestation payload")?,
        ),
        (
            "lifecycle_state".to_string(),
            Value::String(effective_policy_lifecycle_state(policy).to_string()),
        ),
        (
            "approval_status".to_string(),
            Value::String(effective_policy_approval_status(policy).to_string()),
        ),
        (
            "active".to_string(),
            Value::Bool(policy_is_automatic(policy)),
        ),
        (
            "provider_interface_id".to_string(),
            serde_json::to_value(&policy.provider_interface_id).context(
                "Failed to serialize policy provider_interface_id into attestation payload",
            )?,
        ),
        (
            "consumer_interface_id".to_string(),
            serde_json::to_value(&policy.consumer_interface_id).context(
                "Failed to serialize policy consumer_interface_id into attestation payload",
            )?,
        ),
        (
            "contract_id".to_string(),
            serde_json::to_value(&policy.contract_id)
                .context("Failed to serialize policy contract_id into attestation payload")?,
        ),
        (
            "source_system_id".to_string(),
            serde_json::to_value(&policy.source_system_id)
                .context("Failed to serialize policy source_system_id into attestation payload")?,
        ),
        (
            "target_system_id".to_string(),
            serde_json::to_value(&policy.target_system_id)
                .context("Failed to serialize policy target_system_id into attestation payload")?,
        ),
        (
            "interface_id".to_string(),
            serde_json::to_value(&policy.interface_id)
                .context("Failed to serialize policy interface_id into attestation payload")?,
        ),
        (
            "approval_requested_by".to_string(),
            serde_json::to_value(&policy.approval_requested_by).context(
                "Failed to serialize policy approval_requested_by into attestation payload",
            )?,
        ),
        (
            "approval_requested_at".to_string(),
            serde_json::to_value(
                policy
                    .approval_requested_at
                    .as_ref()
                    .map(|value| value.to_rfc3339()),
            )
            .context("Failed to serialize policy approval_requested_at into attestation payload")?,
        ),
        (
            "approved_by".to_string(),
            serde_json::to_value(&policy.approved_by)
                .context("Failed to serialize policy approved_by into attestation payload")?,
        ),
        (
            "approved_at".to_string(),
            serde_json::to_value(policy.approved_at.as_ref().map(|value| value.to_rfc3339()))
                .context("Failed to serialize policy approved_at into attestation payload")?,
        ),
        (
            "rejected_by".to_string(),
            serde_json::to_value(&policy.rejected_by)
                .context("Failed to serialize policy rejected_by into attestation payload")?,
        ),
        (
            "rejected_at".to_string(),
            serde_json::to_value(policy.rejected_at.as_ref().map(|value| value.to_rfc3339()))
                .context("Failed to serialize policy rejected_at into attestation payload")?,
        ),
        (
            "rejection_reason".to_string(),
            serde_json::to_value(&policy.rejection_reason)
                .context("Failed to serialize policy rejection_reason into attestation payload")?,
        ),
        (
            "created_by".to_string(),
            Value::String(policy.created_by.clone()),
        ),
        (
            "updated_by".to_string(),
            Value::String(policy.updated_by.clone()),
        ),
        (
            "created_at".to_string(),
            Value::String(policy.created_at.to_rfc3339()),
        ),
        (
            "updated_at".to_string(),
            Value::String(policy.updated_at.to_rfc3339()),
        ),
    ]));

    serde_json::to_vec(&canonicalize_json(&value))
        .context("Failed to serialize canonical policy attestation payload")
}

fn effective_policy_lifecycle_state(policy: &SosPolicy) -> &str {
    match policy.lifecycle_state.as_deref() {
        Some("draft") | Some("dry_run") | Some("active") | Some("deprecated") | Some("retired") => {
            policy.lifecycle_state.as_deref().unwrap_or("draft")
        }
        Some(_) | None => {
            if policy.active {
                "active"
            } else {
                "draft"
            }
        }
    }
}

fn effective_policy_approval_status(policy: &SosPolicy) -> &str {
    match policy.approval_status.as_deref() {
        Some("pending") | Some("approved") | Some("rejected") => {
            policy.approval_status.as_deref().unwrap_or("pending")
        }
        Some(_) | None => {
            if policy_is_automatic(policy)
                || policy.approved_by.is_some()
                || policy.approved_at.is_some()
            {
                "approved"
            } else if policy.rejected_by.is_some() || policy.rejected_at.is_some() {
                "rejected"
            } else {
                "pending"
            }
        }
    }
}

fn policy_is_automatic(policy: &SosPolicy) -> bool {
    matches!(
        effective_policy_lifecycle_state(policy),
        "dry_run" | "active" | "deprecated"
    )
}

fn policy_revision_ref(policy: &SosPolicy) -> String {
    format!("policy:{}@{}", policy.policy_id, policy.revision)
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonicalize_json(&map[&key]));
            }
            Value::Object(canonical)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

fn sha256_prefixed(bytes: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes.as_ref());
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};

    fn signing_material() -> PolicyAttestationSigningMaterial {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        PolicyAttestationSigningMaterial {
            signing_key: SigningKey::from_bytes(&bytes),
            signing_key_ref: Some("sos/policies/signing-key".to_string()),
            signing_key_version: Some("1".to_string()),
            signing_key_source: "secret_store".to_string(),
            metadata: HashMap::from([
                (
                    POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY.to_string(),
                    Value::String("external_reference".to_string()),
                ),
                (
                    POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY.to_string(),
                    Value::String("aws-kms".to_string()),
                ),
            ]),
        }
    }

    fn sample_policy() -> SosPolicy {
        let now = Utc::now();
        SosPolicy {
            policy_id: "policy-1".to_string(),
            revision: 2,
            policy_name: "Safety Policy".to_string(),
            description: Some("Approved policy".to_string()),
            lifecycle_state: Some("active".to_string()),
            approval_status: Some("approved".to_string()),
            approval_requested_by: Some("requester".to_string()),
            approval_requested_at: Some(now),
            approved_by: Some("approver".to_string()),
            approved_at: Some(now),
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            target_type: "interface_pair".to_string(),
            target_key: Some("interface_pair:provider-if:consumer-if".to_string()),
            stages: vec!["pre_execution".to_string(), "contract_signing".to_string()],
            enforcement_level: "mandatory".to_string(),
            severity: "high".to_string(),
            sparql_query: "ASK { ?s ?p ?o }".to_string(),
            context: HashMap::from([("runtime".to_string(), serde_json::json!({"b": 2, "a": 1}))]),
            tags: vec!["approved".to_string()],
            ontology_refs: vec!["sos-core".to_string()],
            shape_refs: vec!["shape:test".to_string()],
            active: true,
            provider_interface_id: Some("provider-if".to_string()),
            consumer_interface_id: Some("consumer-if".to_string()),
            contract_id: None,
            source_system_id: None,
            target_system_id: None,
            interface_id: None,
            created_by: "creator".to_string(),
            updated_by: "approver".to_string(),
            superseded_by_revision: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn policy_attestation_round_trip_verifies() {
        let policy = sample_policy();
        let attestation = build_policy_attestation_record(
            &policy,
            &signing_material(),
            Some("request-1".to_string()),
            vec!["evidence-2".to_string(), "evidence-1".to_string()],
            vec!["policy:b".to_string(), "policy:a".to_string()],
        )
        .expect("policy attestation should be built");

        assert_eq!(attestation.evidence_ids, vec!["evidence-1", "evidence-2"]);
        assert_eq!(attestation.policy_refs, vec!["policy:a", "policy:b"]);
        assert_eq!(
            attestation
                .metadata
                .get(POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY),
            Some(&Value::String("external_reference".to_string()))
        );
        assert!(verify_policy_attestation(&policy, &attestation));
    }

    #[test]
    fn policy_attestation_detects_tampering() {
        let policy = sample_policy();
        let attestation = build_policy_attestation_record(
            &policy,
            &signing_material(),
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("policy attestation should be built");

        let mut tampered = policy.clone();
        tampered.severity = "critical".to_string();

        assert!(!verify_policy_attestation(&tampered, &attestation));
    }
}
