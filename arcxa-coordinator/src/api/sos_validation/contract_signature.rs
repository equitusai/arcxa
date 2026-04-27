use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::api::sos_validation::contract_governance::{
    contract_revision_ref, effective_contract_approval_status, effective_contract_lifecycle_state,
};
use crate::api::sos_validation::storage::{Contract, ContractSignatureRecord};

pub(crate) const CONTRACT_SIGNATURE_ALGORITHM: &str = "ed25519";
pub(crate) const CONTRACT_SIGNATURE_HASH_ALGORITHM: &str = "sha256";
pub(crate) const CONTRACT_SIGNATURE_PAYLOAD_VERSION: u32 = 1;

pub(crate) fn contract_signing_key_public_material(signing_key: &SigningKey) -> (String, String) {
    let public_key = signing_key.verifying_key().to_bytes();
    (
        BASE64_STANDARD.encode(public_key),
        sha256_prefixed(public_key),
    )
}

pub(crate) fn build_contract_signature_record(
    contract: &Contract,
    signing_key: &SigningKey,
    signing_key_ref: Option<&str>,
    signing_key_version: Option<&str>,
    signing_key_source: &str,
    approval_request_id: Option<String>,
    mut evidence_ids: Vec<String>,
    mut policy_refs: Vec<String>,
) -> Result<ContractSignatureRecord> {
    if !contract.signed {
        return Err(anyhow!(
            "Contract '{}' revision {} must be marked signed before attestation material is built",
            contract.contract_id,
            contract.revision
        ));
    }

    let signed_by = contract
        .signed_by
        .clone()
        .ok_or_else(|| anyhow!("Signed contract is missing signed_by"))?;
    let signed_at = contract
        .signed_at
        .ok_or_else(|| anyhow!("Signed contract is missing signed_at"))?;

    evidence_ids.sort();
    evidence_ids.dedup();
    policy_refs.sort();
    policy_refs.dedup();

    let payload = build_contract_signature_payload(contract)?;
    let payload_hash = sha256_prefixed(&payload);
    let signature = signing_key.sign(&payload);
    let (public_key, key_fingerprint) = contract_signing_key_public_material(signing_key);

    Ok(ContractSignatureRecord {
        signature_id: uuid::Uuid::new_v4().to_string(),
        contract_id: contract.contract_id.clone(),
        contract_revision: contract.revision,
        contract_revision_ref: contract_revision_ref(contract),
        payload_hash,
        payload_hash_algorithm: CONTRACT_SIGNATURE_HASH_ALGORITHM.to_string(),
        signature_algorithm: CONTRACT_SIGNATURE_ALGORITHM.to_string(),
        signature: BASE64_STANDARD.encode(signature.to_bytes()),
        public_key,
        key_fingerprint,
        signing_key_ref: signing_key_ref.map(ToString::to_string),
        signing_key_version: signing_key_version.map(ToString::to_string),
        signing_key_source: signing_key_source.to_string(),
        signed_by,
        signed_at,
        approval_request_id,
        evidence_ids,
        policy_refs,
        metadata: std::collections::HashMap::from([(
            "payload_version".to_string(),
            Value::from(CONTRACT_SIGNATURE_PAYLOAD_VERSION),
        )]),
    })
}

pub(crate) fn verify_contract_signature(
    contract: &Contract,
    signature: &ContractSignatureRecord,
) -> bool {
    if signature.contract_id != contract.contract_id
        || signature.contract_revision != contract.revision
    {
        return false;
    }

    if signature.signature_algorithm != CONTRACT_SIGNATURE_ALGORITHM
        || signature.payload_hash_algorithm != CONTRACT_SIGNATURE_HASH_ALGORITHM
    {
        return false;
    }

    let payload = match build_contract_signature_payload(contract) {
        Ok(payload) => payload,
        Err(_) => return false,
    };
    if signature.payload_hash != sha256_prefixed(&payload) {
        return false;
    }

    let public_key = match BASE64_STANDARD.decode(&signature.public_key) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key_bytes: [u8; 32] = match public_key.as_slice().try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    if signature.key_fingerprint != sha256_prefixed(&public_key_bytes) {
        return false;
    }

    let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let signature_bytes = match BASE64_STANDARD.decode(&signature.signature) {
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

fn build_contract_signature_payload(contract: &Contract) -> Result<Vec<u8>> {
    let value = Value::Object(Map::from_iter([
        (
            "payload_version".to_string(),
            Value::from(CONTRACT_SIGNATURE_PAYLOAD_VERSION),
        ),
        (
            "contract_id".to_string(),
            Value::String(contract.contract_id.clone()),
        ),
        (
            "contract_revision".to_string(),
            Value::from(contract.revision),
        ),
        (
            "contract_revision_ref".to_string(),
            Value::String(contract_revision_ref(contract)),
        ),
        (
            "contract_name".to_string(),
            Value::String(contract.contract_name.clone()),
        ),
        (
            "provider_interface_id".to_string(),
            Value::String(contract.provider_interface_id.clone()),
        ),
        (
            "consumer_interface_id".to_string(),
            Value::String(contract.consumer_interface_id.clone()),
        ),
        (
            "sla_metrics".to_string(),
            serde_json::to_value(&contract.sla_metrics)
                .context("Failed to serialize contract SLA metrics into signature payload")?,
        ),
        (
            "transformation_rules".to_string(),
            canonicalize_json(
                &serde_json::to_value(&contract.transformation_rules)
                    .context("Failed to serialize transformation rules into signature payload")?,
            ),
        ),
        (
            "description".to_string(),
            serde_json::to_value(&contract.description)
                .context("Failed to serialize contract description into signature payload")?,
        ),
        (
            "tags".to_string(),
            serde_json::to_value(&contract.tags)
                .context("Failed to serialize contract tags into signature payload")?,
        ),
        (
            "lifecycle_state".to_string(),
            Value::String(effective_contract_lifecycle_state(contract).to_string()),
        ),
        (
            "approval_status".to_string(),
            Value::String(effective_contract_approval_status(contract).to_string()),
        ),
        (
            "approval_requested_by".to_string(),
            serde_json::to_value(&contract.approval_requested_by)
                .context("Failed to serialize contract approval_requested_by")?,
        ),
        (
            "approval_requested_at".to_string(),
            serde_json::to_value(
                contract
                    .approval_requested_at
                    .as_ref()
                    .map(|value| value.to_rfc3339()),
            )
            .context("Failed to serialize contract approval_requested_at")?,
        ),
        (
            "approved_by".to_string(),
            serde_json::to_value(&contract.approved_by)
                .context("Failed to serialize contract approved_by")?,
        ),
        (
            "approved_at".to_string(),
            serde_json::to_value(
                contract
                    .approved_at
                    .as_ref()
                    .map(|value| value.to_rfc3339()),
            )
            .context("Failed to serialize contract approved_at")?,
        ),
        (
            "signed_by".to_string(),
            serde_json::to_value(&contract.signed_by)
                .context("Failed to serialize contract signed_by")?,
        ),
        (
            "signed_at".to_string(),
            serde_json::to_value(contract.signed_at.as_ref().map(|value| value.to_rfc3339()))
                .context("Failed to serialize contract signed_at")?,
        ),
        (
            "created_by".to_string(),
            Value::String(contract.created_by.clone()),
        ),
        (
            "updated_by".to_string(),
            Value::String(contract.updated_by.clone()),
        ),
        (
            "created_at".to_string(),
            Value::String(contract.created_at.to_rfc3339()),
        ),
        (
            "updated_at".to_string(),
            Value::String(contract.updated_at.to_rfc3339()),
        ),
    ]));

    serde_json::to_vec(&canonicalize_json(&value))
        .context("Failed to serialize canonical contract signature payload")
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
    use crate::api::sos_validation::storage::SlaMetric;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};
    use std::collections::HashMap;

    fn signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn sample_contract() -> Contract {
        let now = Utc::now();
        Contract {
            contract_id: "contract-1".to_string(),
            revision: 2,
            contract_name: "Telemetry Contract".to_string(),
            provider_interface_id: "provider-if".to_string(),
            consumer_interface_id: "consumer-if".to_string(),
            sla_metrics: vec![SlaMetric {
                name: "latency_ms".to_string(),
                value: 100.0,
                operator: "<=".to_string(),
                unit: Some("ms".to_string()),
            }],
            transformation_rules: HashMap::from([(
                "normalize".to_string(),
                serde_json::json!({"b": 2, "a": 1}),
            )]),
            description: Some("Signed contract".to_string()),
            tags: vec!["signed".to_string()],
            approved: true,
            signed: true,
            lifecycle_state: Some("signed".to_string()),
            approval_status: Some("approved".to_string()),
            approval_requested_by: Some("requester".to_string()),
            approval_requested_at: Some(now),
            approved_by: Some("approver".to_string()),
            approved_at: Some(now),
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            signed_by: Some("signer".to_string()),
            signed_at: Some(now),
            created_by: "creator".to_string(),
            updated_by: "signer".to_string(),
            superseded_by_revision: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn contract_signature_round_trip_verifies() {
        let contract = sample_contract();
        let record = build_contract_signature_record(
            &contract,
            &signing_key(),
            Some("sos/contracts/signing-key"),
            Some("version-1"),
            "secret_store",
            Some("request-1".to_string()),
            vec!["evidence-2".to_string(), "evidence-1".to_string()],
            vec!["policy:b".to_string(), "policy:a".to_string()],
        )
        .expect("signature record should be built");

        assert_eq!(record.evidence_ids, vec!["evidence-1", "evidence-2"]);
        assert_eq!(record.policy_refs, vec!["policy:a", "policy:b"]);
        assert_eq!(
            record.signing_key_ref.as_deref(),
            Some("sos/contracts/signing-key")
        );
        assert_eq!(record.signing_key_version.as_deref(), Some("version-1"));
        assert_eq!(record.signing_key_source, "secret_store");
        assert!(verify_contract_signature(&contract, &record));
    }

    #[test]
    fn contract_signature_detects_tampering() {
        let contract = sample_contract();
        let key = signing_key();
        let record = build_contract_signature_record(
            &contract,
            &key,
            None,
            None,
            "embedded_public_key",
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("signature record should be built");
        let mut tampered = contract.clone();
        tampered.contract_name = "Different".to_string();
        assert!(!verify_contract_signature(&tampered, &record));
    }
}
