use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use graphica_core::migration_evidence::{EvidencePacket, EvidencePacketSignature};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const SIGNATURE_ALGORITHM: &str = "ed25519";
const HASH_ALGORITHM: &str = "sha256";

pub fn build_evidence_packet_signature(
    packet: &EvidencePacket,
    signing_key: &SigningKey,
) -> Result<EvidencePacketSignature> {
    let payload = canonical_packet_payload(packet)?;
    let signature = signing_key.sign(&payload);
    let public_key = signing_key.verifying_key().to_bytes();

    Ok(EvidencePacketSignature {
        algorithm: SIGNATURE_ALGORITHM.to_string(),
        payload_hash_algorithm: HASH_ALGORITHM.to_string(),
        payload_hash: sha256_prefixed(&payload),
        public_key: BASE64_STANDARD.encode(public_key),
        key_fingerprint: sha256_prefixed(&public_key),
        signature: BASE64_STANDARD.encode(signature.to_bytes()),
        signed_at: Utc::now(),
    })
}

pub fn verify_evidence_packet_signature(
    packet: &EvidencePacket,
    signature: &EvidencePacketSignature,
) -> bool {
    if signature.algorithm != SIGNATURE_ALGORITHM
        || signature.payload_hash_algorithm != HASH_ALGORITHM
    {
        return false;
    }

    let payload = match canonical_packet_payload(packet) {
        Ok(payload) => payload,
        Err(_) => return false,
    };

    if signature.payload_hash != sha256_prefixed(&payload) {
        return false;
    }

    let public_key_bytes = match BASE64_STANDARD.decode(&signature.public_key) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key_bytes: [u8; 32] = match public_key_bytes.as_slice().try_into() {
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

fn canonical_packet_payload(packet: &EvidencePacket) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(packet).context("failed to serialize evidence packet")?;
    if let Value::Object(ref mut object) = value {
        object.remove("signature");
    }
    let canonical = canonicalize_value(&value);
    serde_json::to_vec(&canonical).context("failed to serialize canonical evidence packet payload")
}

fn canonicalize_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_value).collect()),
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = object.keys().cloned().collect();
            keys.sort();
            for key in keys {
                if let Some(inner) = object.get(&key) {
                    sorted.insert(key, canonicalize_value(inner));
                }
            }
            Value::Object(sorted)
        }
        other => other.clone(),
    }
}

fn sha256_prefixed<T: AsRef<[u8]>>(value: T) -> String {
    let digest = Sha256::digest(value.as_ref());
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{ApprovalEvent, ApprovalStatus, ControlResult, ControlStatus, EvidencePacket, ExecutionEvent, ExecutionStatus, ExceptionRecord, ExceptionSeverity, ExceptionStatus, SourceFieldRef, TargetFieldRef};
    use std::collections::HashMap;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn packet() -> EvidencePacket {
        EvidencePacket {
            packet_id: "packet-1".to_string(),
            program_id: "program-1".to_string(),
            object_id: "object-1".to_string(),
            value_key: "record-1::$.amount".to_string(),
            generated_at: Utc::now(),
            source_field: SourceFieldRef {
                system: "ECC".to_string(),
                object_name: "VBAK".to_string(),
                field_name: "NETWR".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: None,
                record_id: Some("record-1".to_string()),
            },
            target_field: TargetFieldRef {
                system: "S4".to_string(),
                object_name: "A_SalesOrder".to_string(),
                field_name: "NetAmount".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: None,
                record_id: Some("record-1".to_string()),
            },
            transformation_rule: None,
            execution_event: Some(ExecutionEvent {
                execution_id: "exec-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                connector_run_id: "run-1".to_string(),
                tool_name: "ibm_rapid_move".to_string(),
                tool_run_id: "tool-run-1".to_string(),
                stage: "load".to_string(),
                status: ExecutionStatus::Succeeded,
                happened_at: Utc::now(),
                source_snapshot_ref: None,
                target_snapshot_ref: None,
                records_examined: Some(1),
                records_affected: Some(1),
                metadata: HashMap::new(),
            }),
            exceptions: vec![ExceptionRecord {
                exception_id: "exc-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                severity: ExceptionSeverity::Warning,
                status: ExceptionStatus::Accepted,
                category: "delta".to_string(),
                message: "accepted delta".to_string(),
                source_value: Some(Value::from(10)),
                target_value: Some(Value::from(11)),
                remediation: None,
                detected_at: Utc::now(),
                resolved_at: None,
                metadata: HashMap::new(),
            }],
            controls: vec![ControlResult {
                control_id: "control-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                control_name: "amount-match".to_string(),
                control_type: "reconciliation".to_string(),
                status: ControlStatus::Passed,
                summary: "matched".to_string(),
                expected_value: Some(Value::from(10)),
                actual_value: Some(Value::from(10)),
                tolerance: Some(0.0),
                executed_at: Utc::now(),
                evidence_refs: vec![],
                metadata: HashMap::new(),
            }],
            approvals: vec![ApprovalEvent {
                approval_id: "approval-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                approver_role: "data_owner".to_string(),
                approver_id: "user-1".to_string(),
                status: ApprovalStatus::Approved,
                comment: None,
                approved_at: Utc::now(),
                evidence_refs: vec![],
                attestation_ref: None,
                metadata: HashMap::new(),
            }],
            graph_refs: vec!["urn:arcxa:migration-evidence:object:object-1".to_string()],
            narrative: Some("value is justified".to_string()),
            signature: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn evidence_packet_signature_round_trip_verifies() {
        let mut packet = packet();
        let signature = build_evidence_packet_signature(&packet, &signing_key()).unwrap();
        packet.signature = Some(signature.clone());
        assert!(verify_evidence_packet_signature(&packet, &signature));
    }
}
