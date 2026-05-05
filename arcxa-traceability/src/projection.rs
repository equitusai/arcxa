use graphica_core::distributed::proto::shard_service::{shard_service_client::ShardServiceClient, InsertBatchRequest, Triple};
use graphica_core::migration_evidence::{ApprovalEvent, ControlResult, EvidencePacket, ExecutionEvent, ExceptionRecord, MigrationObject, MigrationProgram, TransformationRule, ValueExplanation};

pub fn program_ref(program_id: &str) -> String {
    format!("urn:arcxa:migration-evidence:program:{}", program_id)
}

pub fn object_ref(object_id: &str) -> String {
    format!("urn:arcxa:migration-evidence:object:{}", object_id)
}

pub fn value_ref(object_id: &str, value_key: &str) -> String {
    format!("urn:arcxa:migration-evidence:value:{}:{}", object_id, sanitize(value_key))
}

pub fn packet_ref(packet_id: &str) -> String {
    format!("urn:arcxa:migration-evidence:packet:{}", packet_id)
}

pub fn rule_ref(rule_id: &str) -> String {
    format!("urn:arcxa:migration-evidence:rule:{}", rule_id)
}

fn sanitize(value: &str) -> String {
    value
        .replace(':', "_")
        .replace('/', "_")
        .replace('$', "root")
        .replace('.', "_")
}

pub fn program_triples(program: &MigrationProgram) -> Vec<Triple> {
    let subject = program_ref(&program.program_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:Program"),
        triple_literal(&subject, "urn:arcxa:migration-evidence:name", &program.name),
    ]
}

pub fn object_triples(object: &MigrationObject) -> Vec<Triple> {
    let subject = object_ref(&object.object_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:Object"),
        triple(&subject, "urn:arcxa:migration-evidence:belongsToProgram", &program_ref(&object.program_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:name", &object.name),
        triple_literal(&subject, "urn:arcxa:migration-evidence:objectType", &format!("{:?}", object.object_type).to_lowercase()),
    ]
}

pub fn rule_triples(object_id: &str, value_key: Option<&str>, rule: &TransformationRule) -> Vec<Triple> {
    let subject = rule_ref(&rule.rule_id);
    let mut triples = vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:TransformationRule"),
        triple(&subject, "urn:arcxa:migration-evidence:appliesToObject", &object_ref(object_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:ruleType", &format!("{:?}", rule.rule_type).to_lowercase()),
        triple_literal(&subject, "urn:arcxa:migration-evidence:name", &rule.name),
    ];
    if let Some(key) = value_key {
        triples.push(triple(&subject, "urn:arcxa:migration-evidence:explainsValue", &value_ref(object_id, key)));
    }
    triples
}

pub fn execution_triples(event: &ExecutionEvent) -> Vec<Triple> {
    let subject = format!("urn:arcxa:migration-evidence:execution:{}", event.execution_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:ExecutionEvent"),
        triple(&subject, "urn:arcxa:migration-evidence:forObject", &object_ref(&event.object_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:toolName", &event.tool_name),
        triple_literal(&subject, "urn:arcxa:migration-evidence:stage", &event.stage),
    ]
}

pub fn control_triples(control: &ControlResult) -> Vec<Triple> {
    let subject = format!("urn:arcxa:migration-evidence:control:{}", control.control_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:ControlResult"),
        triple(&subject, "urn:arcxa:migration-evidence:forObject", &object_ref(&control.object_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:controlName", &control.control_name),
        triple_literal(&subject, "urn:arcxa:migration-evidence:status", &format!("{:?}", control.status).to_lowercase()),
    ]
}

pub fn exception_triples(exception: &ExceptionRecord) -> Vec<Triple> {
    let subject = format!("urn:arcxa:migration-evidence:exception:{}", exception.exception_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:Exception"),
        triple(&subject, "urn:arcxa:migration-evidence:forObject", &object_ref(&exception.object_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:category", &exception.category),
        triple_literal(&subject, "urn:arcxa:migration-evidence:status", &format!("{:?}", exception.status).to_lowercase()),
    ]
}

pub fn approval_triples(approval: &ApprovalEvent) -> Vec<Triple> {
    let subject = format!("urn:arcxa:migration-evidence:approval:{}", approval.approval_id);
    vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:ApprovalEvent"),
        triple(&subject, "urn:arcxa:migration-evidence:forObject", &object_ref(&approval.object_id)),
        triple_literal(&subject, "urn:arcxa:migration-evidence:approverRole", &approval.approver_role),
        triple_literal(&subject, "urn:arcxa:migration-evidence:status", &format!("{:?}", approval.status).to_lowercase()),
    ]
}

pub fn packet_triples(packet: &EvidencePacket) -> Vec<Triple> {
    let subject = packet_ref(&packet.packet_id);
    let mut triples = vec![
        triple(&subject, "rdf:type", "urn:arcxa:migration-evidence:EvidencePacket"),
        triple(&subject, "urn:arcxa:migration-evidence:forObject", &object_ref(&packet.object_id)),
        triple(&subject, "urn:arcxa:migration-evidence:explainsValue", &value_ref(&packet.object_id, &packet.value_key)),
    ];
    if let Some(rule) = &packet.transformation_rule {
        triples.push(triple(&subject, "urn:arcxa:migration-evidence:usesRule", &rule_ref(&rule.rule_id)));
    }
    triples
}

pub fn explanation_graph_refs(explanation: &ValueExplanation, packet: Option<&EvidencePacket>) -> Vec<String> {
    let mut refs = vec![
        object_ref(&explanation.locator.object_id),
        value_ref(&explanation.locator.object_id, &value_key(&explanation.locator)),
    ];
    if let Some(packet) = packet {
        refs.push(packet_ref(&packet.packet_id));
    }
    refs.sort();
    refs.dedup();
    refs
}

pub fn value_key(locator: &graphica_core::migration_evidence::ValueLocator) -> String {
    if let Some(record_id) = &locator.target_record_id {
        format!("{}::{}", record_id, locator.target_field_path)
    } else {
        locator.target_field_path.clone()
    }
}

fn triple(subject: &str, predicate: &str, object: &str) -> Triple {
    Triple {
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: object.to_string(),
        object_datatype: String::new(),
        object_language: String::new(),
        graph: "urn:arcxa:migration-evidence:graph".to_string(),
    }
}

fn triple_literal(subject: &str, predicate: &str, literal: &str) -> Triple {
    Triple {
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: literal.to_string(),
        object_datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
        object_language: String::new(),
        graph: "urn:arcxa:migration-evidence:graph".to_string(),
    }
}

pub async fn push_triples_to_shard(endpoint: &str, triples: Vec<Triple>) -> anyhow::Result<()> {
    if triples.is_empty() {
        return Ok(());
    }

    let mut client = ShardServiceClient::connect(endpoint.to_string()).await?;
    client
        .insert_batch(InsertBatchRequest {
            triples,
            transactional: false,
            default_graph: "urn:arcxa:migration-evidence:graph".to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
        })
        .await?;
    Ok(())
}
