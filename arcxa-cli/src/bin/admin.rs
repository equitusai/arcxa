use anyhow::Result;
use clap::{ArgGroup, Args, Parser, Subcommand, ValueHint};
use graphica_cli::migration_evidence::{
    load_required_json_value, ExplainValueRequest, MigrationEvidenceApiClient,
    MigrationEvidenceApiConfig,
};
use graphica_cli::sos::{
    load_json_value_array, load_optional_json_value_object, render_pretty_json,
    ListPoliciesRequest, ListSystemsRequest, PolicyValidationRequest,
    RotatePolicySigningKeyRequest, SosApiClient, SosApiConfig, StatusPageRequest,
    WhatIfAnalysisRequest,
};
use graphica_cli::utils::init_tracing;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "admin")]
#[command(about = "Operator-facing Graphica administrative commands")]
struct Cli {
    #[command(flatten)]
    api: ApiOptions,

    #[command(subcommand)]
    command: AdminCommand,
}

#[derive(Args, Debug, Clone)]
struct ApiOptions {
    /// Graphica API base URL.
    #[arg(
        long,
        env = "GRAPHICA_API_BASE_URL",
        default_value = "http://localhost:8080/api/v1"
    )]
    base_url: String,

    /// Bearer token for the Graphica API.
    #[arg(long, env = "GRAPHICA_API_TOKEN")]
    token: String,

    /// HTTP timeout in seconds.
    #[arg(long, env = "GRAPHICA_API_TIMEOUT_SECONDS", default_value_t = 30)]
    timeout_seconds: u64,
}

#[derive(Subcommand)]
enum AdminCommand {
    /// Systems-of-systems operational controls and governance audit views.
    Sos(SosCommand),
    /// Migration evidence connectors, explainability, and audit views.
    MigrationEvidence(MigrationEvidenceCommand),
}

#[derive(Parser)]
struct SosCommand {
    #[command(subcommand)]
    command: SosSubcommand,
}

#[derive(Parser)]
struct MigrationEvidenceCommand {
    #[command(subcommand)]
    command: MigrationEvidenceSubcommand,
}

#[derive(Subcommand)]
enum MigrationEvidenceSubcommand {
    /// Manage migration-evidence connectors and ingestion runs.
    Connectors(MigrationEvidenceConnectorCommand),
    /// Inspect or rebuild the traceability read models behind the evidence graph.
    Runtime(MigrationEvidenceRuntimeCommand),
    /// Explain one migrated value from the evidence graph.
    Explain(MigrationEvidenceExplainCommand),
    /// Build an operator-friendly audit bundle for one migrated value.
    Audit(MigrationEvidenceAuditCommand),
    /// Fetch the signed evidence packet for one object and optional value key.
    EvidencePacket {
        #[arg(long)]
        object_id: String,
        #[arg(long)]
        value_key: Option<String>,
    },
    /// Fetch persisted verification and reconciliation controls for one object.
    Controls {
        #[arg(long)]
        object_id: String,
    },
    /// Fetch persisted program-level exceptions.
    Exceptions {
        #[arg(long)]
        program_id: String,
    },
    /// Fetch persisted program-level approvals.
    Approvals {
        #[arg(long)]
        program_id: String,
    },
}

#[derive(Parser)]
struct MigrationEvidenceRuntimeCommand {
    #[command(subcommand)]
    command: MigrationEvidenceRuntimeSubcommand,
}

#[derive(Subcommand)]
enum MigrationEvidenceRuntimeSubcommand {
    /// Show backend, replay, and read-model status for the traceability service.
    Status,
    /// Rebuild the read models from the persisted event log.
    Rebuild,
}

#[derive(Parser)]
struct MigrationEvidenceConnectorCommand {
    #[command(subcommand)]
    command: MigrationEvidenceConnectorSubcommand,
}

#[derive(Subcommand)]
enum MigrationEvidenceConnectorSubcommand {
    /// Upsert one migration-evidence connector from JSON.
    Upsert(MigrationEvidenceJsonInput),
    /// Start one connector run from JSON.
    Run {
        #[arg(long)]
        connector_id: String,
        #[command(flatten)]
        input: MigrationEvidenceJsonInput,
    },
}

#[derive(Args)]
#[command(group(
    ArgGroup::new("migration_evidence_json_input")
        .required(true)
        .args(["json", "file"])
))]
struct MigrationEvidenceJsonInput {
    /// Inline JSON object payload.
    #[arg(long, conflicts_with = "file")]
    json: Option<String>,
    /// Path to a JSON file containing one object payload.
    #[arg(long, value_hint = ValueHint::FilePath, conflicts_with = "json")]
    file: Option<PathBuf>,
}

#[derive(Args)]
struct MigrationEvidenceExplainCommand {
    #[arg(long)]
    program_id: String,
    #[arg(long)]
    object_id: String,
    #[arg(long)]
    target_field_path: String,
    #[arg(long)]
    target_record_id: Option<String>,
    #[arg(long)]
    source_record_id: Option<String>,
}

#[derive(Args)]
struct MigrationEvidenceAuditCommand {
    #[arg(long)]
    program_id: String,
    #[arg(long)]
    object_id: String,
    #[arg(long)]
    target_field_path: String,
    #[arg(long)]
    target_record_id: Option<String>,
    #[arg(long)]
    source_record_id: Option<String>,
    /// Override the evidence-packet lookup key when the default field-path lookup is not enough.
    #[arg(long)]
    value_key: Option<String>,
}

#[derive(Subcommand)]
enum SosSubcommand {
    /// Explicitly rerun SoS reconcile/recovery.
    Reconcile {
        /// Skip ontology/shape asset synchronization and rebuild only the SoS RDF graphs.
        #[arg(long)]
        skip_ontology_sync: bool,
    },
    /// Read-only SoS catalog listings for systems, interfaces, contracts, and policies.
    Catalog(CatalogCommand),
    /// Run SoS validations directly against persisted coordinator state.
    Validate(ValidationCommand),
    /// Fetch persisted SoS validation reports, history, and lineage.
    Reports(ReportCommand),
    /// Read-only SoS analytics workflows.
    Analytics(AnalyticsCommand),
    /// Contract governance controls and audit views.
    Contracts(ContractCommand),
    /// Policy governance controls and audit views.
    Policies(PolicyCommand),
}

#[derive(Parser)]
struct ValidationCommand {
    #[command(subcommand)]
    command: ValidationSubcommand,
}

#[derive(Parser)]
struct CatalogCommand {
    #[command(subcommand)]
    command: CatalogSubcommand,
}

#[derive(Subcommand)]
enum CatalogSubcommand {
    /// List persisted systems with optional filters.
    Systems {
        #[arg(long)]
        system_type: Option<String>,
        #[arg(long)]
        vendor: Option<String>,
        #[arg(long)]
        classification: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        active: Option<bool>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// List persisted interfaces.
    Interfaces,
    /// List persisted contracts.
    Contracts,
    /// List persisted policies with optional filters.
    Policies {
        #[arg(long)]
        target_type: Option<String>,
        #[arg(long)]
        stage: Option<String>,
        #[arg(long)]
        active: Option<bool>,
        #[arg(long)]
        lifecycle_state: Option<String>,
        #[arg(long)]
        approval_status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum ValidationSubcommand {
    /// Validate one provider/consumer interface pair.
    InterfacePair {
        #[arg(long)]
        provider_interface_id: String,
        #[arg(long)]
        consumer_interface_id: String,
        /// Run against the dry-run endpoint without persisting a report.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Parser)]
struct ReportCommand {
    #[command(subcommand)]
    command: ReportSubcommand,
}

#[derive(Subcommand)]
enum ReportSubcommand {
    /// Fetch one persisted validation report by report ID.
    Get {
        #[arg(long)]
        report_id: String,
    },
    /// Fetch newest-first validation history for one normalized subject.
    History {
        #[arg(long)]
        subject_key: String,
        #[arg(long)]
        subject_type: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Fetch validation lineage for one normalized subject.
    Lineage {
        #[arg(long)]
        subject_key: String,
        #[arg(long)]
        subject_type: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Parser)]
struct AnalyticsCommand {
    #[command(subcommand)]
    command: AnalyticsSubcommand,
}

#[derive(Subcommand)]
enum AnalyticsSubcommand {
    /// Generate the interface compatibility matrix.
    CompatibilityMatrix {
        #[arg(long)]
        evaluation_budget: Option<usize>,
    },
    /// Generate the SoS dependency graph.
    DependencyGraph {
        #[arg(long)]
        node_budget: Option<usize>,
        #[arg(long)]
        edge_budget: Option<usize>,
    },
    /// Run a read-only what-if analysis from inline JSON or a JSON file.
    WhatIf(WhatIfCommand),
}

#[derive(Args)]
#[command(group(
    ArgGroup::new("changes_input")
        .required(true)
        .args(["changes_json", "changes_file"])
))]
struct WhatIfCommand {
    #[arg(long)]
    scenario: String,
    #[arg(long)]
    evaluation_budget: Option<usize>,
    /// JSON array of change objects.
    #[arg(long, conflicts_with = "changes_file")]
    changes_json: Option<String>,
    /// Path to a JSON file containing an array of change objects.
    #[arg(long, value_hint = ValueHint::FilePath, conflicts_with = "changes_json")]
    changes_file: Option<PathBuf>,
}

#[derive(Parser)]
struct ContractCommand {
    #[command(subcommand)]
    command: ContractSubcommand,
}

#[derive(Subcommand)]
enum ContractSubcommand {
    /// Print the current contract governance audit bundle for one contract.
    Audit {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Fetch one contract by ID.
    Get {
        #[arg(long)]
        contract_id: String,
    },
    /// Lookup the contract binding for one provider/consumer interface pair.
    Lookup {
        #[arg(long)]
        provider_interface_id: String,
        #[arg(long)]
        consumer_interface_id: String,
    },
    /// Read persisted contract approval requests.
    ApprovalRequests(ContractApprovalRequestsCommand),
    /// List persisted contract signatures.
    Signatures {
        #[arg(long)]
        contract_id: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Inspect or rotate the contract signing key.
    SigningKey(ContractSigningKeyCommand),
}

#[derive(Parser)]
struct ContractApprovalRequestsCommand {
    #[command(subcommand)]
    command: ContractApprovalRequestsSubcommand,
}

#[derive(Subcommand)]
enum ContractApprovalRequestsSubcommand {
    /// List approval requests for one contract.
    List {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Fetch one approval request for one contract.
    Get {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Parser)]
struct ContractSigningKeyCommand {
    #[command(subcommand)]
    command: ContractSigningKeySubcommand,
}

#[derive(Subcommand)]
enum ContractSigningKeySubcommand {
    /// Show the current contract signing-key status.
    Status,
    /// Rotate the managed contract signing key.
    Rotate {
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Parser)]
struct PolicyCommand {
    #[command(subcommand)]
    command: PolicySubcommand,
}

#[derive(Subcommand)]
enum PolicySubcommand {
    /// Print the current policy governance audit bundle for one policy.
    Audit {
        #[arg(long)]
        policy_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Fetch one policy by ID.
    Get {
        #[arg(long)]
        policy_id: String,
    },
    /// Read persisted policy approval requests.
    ApprovalRequests(PolicyApprovalRequestsCommand),
    /// List persisted policy approval attestations.
    Attestations {
        #[arg(long)]
        policy_id: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Evaluate one persisted policy with optional revision pinning and runtime context.
    Validate(PolicyValidateCommand),
    /// Inspect or rotate the policy signing key.
    SigningKey(PolicySigningKeyCommand),
}

#[derive(Parser)]
struct PolicyApprovalRequestsCommand {
    #[command(subcommand)]
    command: PolicyApprovalRequestsSubcommand,
}

#[derive(Subcommand)]
enum PolicyApprovalRequestsSubcommand {
    /// List approval requests for one policy.
    List {
        #[arg(long)]
        policy_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Fetch one approval request for one policy.
    Get {
        #[arg(long)]
        policy_id: String,
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Args)]
#[command(group(
    ArgGroup::new("policy_context_input")
        .required(false)
        .args(["context_json", "context_file"])
))]
struct PolicyValidateCommand {
    #[arg(long)]
    policy_id: String,
    #[arg(long)]
    stage: Option<String>,
    #[arg(long)]
    revision: Option<u32>,
    /// Run against the dry-run endpoint without persisting a report.
    #[arg(long)]
    dry_run: bool,
    /// Runtime context as an inline JSON object.
    #[arg(long, conflicts_with = "context_file")]
    context_json: Option<String>,
    /// Runtime context as a JSON file containing one object.
    #[arg(long, value_hint = ValueHint::FilePath, conflicts_with = "context_json")]
    context_file: Option<PathBuf>,
}

#[derive(Parser)]
struct PolicySigningKeyCommand {
    #[command(subcommand)]
    command: PolicySigningKeySubcommand,
}

#[derive(Subcommand)]
enum PolicySigningKeySubcommand {
    /// Show the current policy signing-key status.
    Status,
    /// Rotate the managed policy signing key and optionally record external-trust metadata.
    Rotate {
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        trust_mode: Option<String>,
        #[arg(long)]
        trust_provider: Option<String>,
        #[arg(long)]
        external_key_ref: Option<String>,
        #[arg(long)]
        trust_attestation_ref: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let cli = Cli::parse();
    let sos_client = SosApiClient::new(SosApiConfig {
        base_url: cli.api.base_url.clone(),
        token: cli.api.token.clone(),
        timeout: Duration::from_secs(cli.api.timeout_seconds),
    })?;
    let migration_evidence_client = MigrationEvidenceApiClient::new(MigrationEvidenceApiConfig {
        base_url: cli.api.base_url,
        token: cli.api.token,
        timeout: Duration::from_secs(cli.api.timeout_seconds),
    })?;

    let response = match cli.command {
        AdminCommand::Sos(sos) => run_sos_command(&sos_client, sos).await?,
        AdminCommand::MigrationEvidence(command) => {
            run_migration_evidence_command(&migration_evidence_client, command).await?
        }
    };

    println!("{}", render_pretty_json(&response)?);
    Ok(())
}

async fn run_sos_command(client: &SosApiClient, command: SosCommand) -> Result<serde_json::Value> {
    match command.command {
        SosSubcommand::Reconcile { skip_ontology_sync } => {
            client.reconcile(!skip_ontology_sync).await
        }
        SosSubcommand::Catalog(command) => run_catalog_command(client, command).await,
        SosSubcommand::Validate(command) => run_validation_command(client, command).await,
        SosSubcommand::Reports(command) => run_report_command(client, command).await,
        SosSubcommand::Analytics(command) => run_analytics_command(client, command).await,
        SosSubcommand::Contracts(command) => run_contract_command(client, command).await,
        SosSubcommand::Policies(command) => run_policy_command(client, command).await,
    }
}

async fn run_migration_evidence_command(
    client: &MigrationEvidenceApiClient,
    command: MigrationEvidenceCommand,
) -> Result<serde_json::Value> {
    match command.command {
        MigrationEvidenceSubcommand::Connectors(command) => {
            run_migration_evidence_connector_command(client, command).await
        }
        MigrationEvidenceSubcommand::Runtime(command) => match command.command {
            MigrationEvidenceRuntimeSubcommand::Status => client.runtime_status().await,
            MigrationEvidenceRuntimeSubcommand::Rebuild => client.rebuild_read_models().await,
        },
        MigrationEvidenceSubcommand::Explain(command) => {
            client
                .explain_value(ExplainValueRequest {
                    program_id: &command.program_id,
                    object_id: &command.object_id,
                    target_field_path: &command.target_field_path,
                    target_record_id: command.target_record_id.as_deref(),
                    source_record_id: command.source_record_id.as_deref(),
                })
                .await
        }
        MigrationEvidenceSubcommand::Audit(command) => {
            let explanation = client
                .explain_value(ExplainValueRequest {
                    program_id: &command.program_id,
                    object_id: &command.object_id,
                    target_field_path: &command.target_field_path,
                    target_record_id: command.target_record_id.as_deref(),
                    source_record_id: command.source_record_id.as_deref(),
                })
                .await?;
            let value_key = command
                .value_key
                .clone()
                .or_else(|| {
                    command
                        .target_record_id
                        .as_deref()
                        .map(|record_id| format!("{record_id}::{}", command.target_field_path))
                })
                .unwrap_or_else(|| command.target_field_path.clone());
            let packet = client
                .evidence_packet(&command.object_id, Some(&value_key))
                .await?;
            let controls = client.object_controls(&command.object_id).await?;
            let exceptions = client.program_exceptions(&command.program_id).await?;
            let approvals = client.program_approvals(&command.program_id).await?;
            Ok(serde_json::json!({
                "explanation": explanation,
                "evidence_packet": packet,
                "controls": controls,
                "exceptions": exceptions,
                "approvals": approvals,
            }))
        }
        MigrationEvidenceSubcommand::EvidencePacket {
            object_id,
            value_key,
        } => {
            client
                .evidence_packet(&object_id, value_key.as_deref())
                .await
        }
        MigrationEvidenceSubcommand::Controls { object_id } => {
            client.object_controls(&object_id).await
        }
        MigrationEvidenceSubcommand::Exceptions { program_id } => {
            client.program_exceptions(&program_id).await
        }
        MigrationEvidenceSubcommand::Approvals { program_id } => {
            client.program_approvals(&program_id).await
        }
    }
}

async fn run_migration_evidence_connector_command(
    client: &MigrationEvidenceApiClient,
    command: MigrationEvidenceConnectorCommand,
) -> Result<serde_json::Value> {
    match command.command {
        MigrationEvidenceConnectorSubcommand::Upsert(input) => {
            let connector = load_required_json_value(
                input.json.as_deref(),
                input.file.as_deref(),
                "connector",
            )?;
            client.upsert_connector(connector).await
        }
        MigrationEvidenceConnectorSubcommand::Run {
            connector_id,
            input,
        } => {
            let request = load_required_json_value(
                input.json.as_deref(),
                input.file.as_deref(),
                "connector run request",
            )?;
            client.run_connector(&connector_id, request).await
        }
    }
}

async fn run_catalog_command(
    client: &SosApiClient,
    command: CatalogCommand,
) -> Result<serde_json::Value> {
    match command.command {
        CatalogSubcommand::Systems {
            system_type,
            vendor,
            classification,
            tags,
            active,
            offset,
            limit,
        } => {
            client
                .list_systems(ListSystemsRequest {
                    system_type,
                    vendor,
                    classification,
                    tags,
                    active,
                    offset,
                    limit,
                })
                .await
        }
        CatalogSubcommand::Interfaces => client.list_interfaces().await,
        CatalogSubcommand::Contracts => client.list_contracts().await,
        CatalogSubcommand::Policies {
            target_type,
            stage,
            active,
            lifecycle_state,
            approval_status,
            offset,
            limit,
        } => {
            client
                .list_policies(ListPoliciesRequest {
                    target_type,
                    stage,
                    active,
                    lifecycle_state,
                    approval_status,
                    offset,
                    limit,
                })
                .await
        }
    }
}

async fn run_validation_command(
    client: &SosApiClient,
    command: ValidationCommand,
) -> Result<serde_json::Value> {
    match command.command {
        ValidationSubcommand::InterfacePair {
            provider_interface_id,
            consumer_interface_id,
            dry_run,
        } => {
            client
                .validate_interface_pair(&provider_interface_id, &consumer_interface_id, dry_run)
                .await
        }
    }
}

async fn run_report_command(
    client: &SosApiClient,
    command: ReportCommand,
) -> Result<serde_json::Value> {
    match command.command {
        ReportSubcommand::Get { report_id } => client.get_validation_report(&report_id).await,
        ReportSubcommand::History {
            subject_key,
            subject_type,
            limit,
        } => {
            client
                .get_validation_history(&subject_key, subject_type.as_deref(), limit)
                .await
        }
        ReportSubcommand::Lineage {
            subject_key,
            subject_type,
            limit,
        } => {
            client
                .get_validation_lineage(&subject_key, subject_type.as_deref(), limit)
                .await
        }
    }
}

async fn run_analytics_command(
    client: &SosApiClient,
    command: AnalyticsCommand,
) -> Result<serde_json::Value> {
    match command.command {
        AnalyticsSubcommand::CompatibilityMatrix { evaluation_budget } => {
            client.get_compatibility_matrix(evaluation_budget).await
        }
        AnalyticsSubcommand::DependencyGraph {
            node_budget,
            edge_budget,
        } => client.get_dependency_graph(node_budget, edge_budget).await,
        AnalyticsSubcommand::WhatIf(command) => {
            let changes = load_json_value_array(
                command.changes_json.as_deref(),
                command.changes_file.as_deref(),
            )?;
            client
                .run_what_if_analysis(WhatIfAnalysisRequest {
                    scenario: command.scenario,
                    changes,
                    evaluation_budget: command.evaluation_budget,
                })
                .await
        }
    }
}

async fn run_contract_command(
    client: &SosApiClient,
    command: ContractCommand,
) -> Result<serde_json::Value> {
    match command.command {
        ContractSubcommand::Audit {
            contract_id,
            status,
            offset,
            limit,
        } => {
            client
                .contract_audit(&contract_id, status.as_deref(), offset, limit)
                .await
        }
        ContractSubcommand::Get { contract_id } => client.get_contract(&contract_id).await,
        ContractSubcommand::Lookup {
            provider_interface_id,
            consumer_interface_id,
        } => {
            client
                .lookup_contract(&provider_interface_id, &consumer_interface_id)
                .await
        }
        ContractSubcommand::ApprovalRequests(command) => match command.command {
            ContractApprovalRequestsSubcommand::List {
                contract_id,
                status,
                offset,
                limit,
            } => {
                client
                    .list_contract_approval_requests(
                        &contract_id,
                        StatusPageRequest {
                            status,
                            offset,
                            limit,
                        },
                    )
                    .await
            }
            ContractApprovalRequestsSubcommand::Get {
                contract_id,
                request_id,
            } => {
                client
                    .get_contract_approval_request(&contract_id, &request_id)
                    .await
            }
        },
        ContractSubcommand::Signatures { contract_id, limit } => {
            client.list_contract_signatures(&contract_id, limit).await
        }
        ContractSubcommand::SigningKey(command) => match command.command {
            ContractSigningKeySubcommand::Status => client.contract_signing_key_status().await,
            ContractSigningKeySubcommand::Rotate { reason } => {
                client.rotate_contract_signing_key(reason.as_deref()).await
            }
        },
    }
}

async fn run_policy_command(
    client: &SosApiClient,
    command: PolicyCommand,
) -> Result<serde_json::Value> {
    match command.command {
        PolicySubcommand::Audit {
            policy_id,
            status,
            offset,
            limit,
        } => {
            client
                .policy_audit(&policy_id, status.as_deref(), offset, limit)
                .await
        }
        PolicySubcommand::Get { policy_id } => client.get_policy(&policy_id).await,
        PolicySubcommand::ApprovalRequests(command) => match command.command {
            PolicyApprovalRequestsSubcommand::List {
                policy_id,
                status,
                offset,
                limit,
            } => {
                client
                    .list_policy_approval_requests(
                        &policy_id,
                        StatusPageRequest {
                            status,
                            offset,
                            limit,
                        },
                    )
                    .await
            }
            PolicyApprovalRequestsSubcommand::Get {
                policy_id,
                request_id,
            } => {
                client
                    .get_policy_approval_request(&policy_id, &request_id)
                    .await
            }
        },
        PolicySubcommand::Attestations { policy_id, limit } => {
            client.list_policy_attestations(&policy_id, limit).await
        }
        PolicySubcommand::Validate(command) => {
            let context = load_optional_json_value_object(
                command.context_json.as_deref(),
                command.context_file.as_deref(),
            )?;
            client
                .validate_policy(
                    &command.policy_id,
                    PolicyValidationRequest {
                        stage: command.stage,
                        revision: command.revision,
                        context,
                        dry_run: command.dry_run,
                    },
                )
                .await
        }
        PolicySubcommand::SigningKey(command) => match command.command {
            PolicySigningKeySubcommand::Status => client.policy_signing_key_status().await,
            PolicySigningKeySubcommand::Rotate {
                reason,
                trust_mode,
                trust_provider,
                external_key_ref,
                trust_attestation_ref,
            } => {
                client
                    .rotate_policy_signing_key(RotatePolicySigningKeyRequest {
                        reason: reason.as_deref(),
                        trust_mode: trust_mode.as_deref(),
                        trust_provider: trust_provider.as_deref(),
                        external_key_ref: external_key_ref.as_deref(),
                        trust_attestation_ref: trust_attestation_ref.as_deref(),
                    })
                    .await
            }
        },
    }
}
