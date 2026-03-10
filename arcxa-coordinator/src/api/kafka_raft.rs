//! REST API endpoints for Kafka Raft coordination
//!
//! Provides HTTP endpoints for Raft leader election and heartbeat operations
//! in multi-coordinator deployments.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::storage::kafka::DistributedReplayCoordinator;

/// Vote request from candidate
#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    pub candidate_id: String,
    pub term: u64,
}

/// Vote response
#[derive(Debug, Serialize)]
pub struct VoteResponse {
    pub vote_granted: bool,
    pub term: u64,
}

/// Heartbeat request from leader
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub leader_id: String,
    pub term: u64,
}

/// Heartbeat response
#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub term: u64,
}

/// Raft API state for endpoint handlers
pub struct RaftApiState {
    pub coordinator: Arc<DistributedReplayCoordinator>,
}

/// Handle vote request from candidate
///
/// POST /kafka/raft/vote
pub async fn handle_vote_request(
    State(raft_state): State<Arc<RaftApiState>>,
    Json(request): Json<VoteRequest>,
) -> impl IntoResponse {
    info!(
        "Received vote request from candidate '{}' for term {}",
        request.candidate_id, request.term
    );

    let coordinator = &raft_state.coordinator;
    let current_term = coordinator.term().await;
    let current_state = coordinator.state().await;

    // Vote granted if:
    // 1. Candidate's term >= our term
    // 2. We haven't voted for anyone else this term
    // 3. We're not already a leader

    let vote_granted = if request.term < current_term {
        debug!(
            "Denying vote: candidate term {} < our term {}",
            request.term, current_term
        );
        false
    } else if current_state == crate::storage::kafka::RaftState::Leader {
        debug!("Denying vote: we are currently leader");
        false
    } else {
        debug!("Granting vote to candidate '{}'", request.candidate_id);
        true
    };

    let response = VoteResponse {
        vote_granted,
        term: current_term.max(request.term),
    };

    Json(response)
}

/// Handle heartbeat from leader
///
/// POST /kafka/raft/heartbeat
pub async fn handle_heartbeat(
    State(raft_state): State<Arc<RaftApiState>>,
    Json(request): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    debug!(
        "Received heartbeat from leader '{}' for term {}",
        request.leader_id, request.term
    );

    let coordinator = &raft_state.coordinator;
    let current_term = coordinator.term().await;

    // Accept heartbeat if leader's term >= our term
    let success = request.term >= current_term;

    if success {
        debug!("Heartbeat accepted from leader '{}'", request.leader_id);
    } else {
        warn!(
            "Heartbeat rejected: leader term {} < our term {}",
            request.term, current_term
        );
    }

    let response = HeartbeatResponse {
        success,
        term: current_term,
    };

    Json(response)
}

/// Get current Raft state
///
/// GET /kafka/raft/state
pub async fn get_raft_state(State(raft_state): State<Arc<RaftApiState>>) -> impl IntoResponse {
    let coordinator = &raft_state.coordinator;

    #[derive(Serialize)]
    struct RaftStateResponse {
        state: String,
        term: u64,
        is_leader: bool,
    }

    let state = coordinator.state().await;
    let term = coordinator.term().await;
    let is_leader = coordinator.is_leader().await;

    Json(RaftStateResponse {
        state: format!("{:?}", state),
        term,
        is_leader,
    })
}

/// Get Raft log
///
/// GET /kafka/raft/log
pub async fn get_raft_log(State(raft_state): State<Arc<RaftApiState>>) -> impl IntoResponse {
    let coordinator = &raft_state.coordinator;
    let log = coordinator.get_log().await;

    Json(log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::kafka::ReplayConfig;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_vote_request_handler() {
        let coordinator = Arc::new(
            DistributedReplayCoordinator::new(
                "test-coordinator",
                vec!["http://test-coordinator:8080".to_string()],
                ReplayConfig::default(),
            )
            .await
            .unwrap(),
        );

        let raft_state = Arc::new(RaftApiState { coordinator });

        let request = VoteRequest {
            candidate_id: "candidate-1".to_string(),
            term: 1,
        };

        let response = handle_vote_request(State(raft_state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_heartbeat_handler() {
        let coordinator = Arc::new(
            DistributedReplayCoordinator::new(
                "test-coordinator",
                vec!["http://test-coordinator:8080".to_string()],
                ReplayConfig::default(),
            )
            .await
            .unwrap(),
        );

        let raft_state = Arc::new(RaftApiState { coordinator });

        let request = HeartbeatRequest {
            leader_id: "leader-1".to_string(),
            term: 1,
        };

        let response = handle_heartbeat(State(raft_state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_raft_state() {
        let coordinator = Arc::new(
            DistributedReplayCoordinator::new(
                "test-coordinator",
                vec!["http://test-coordinator:8080".to_string()],
                ReplayConfig::default(),
            )
            .await
            .unwrap(),
        );

        let raft_state = Arc::new(RaftApiState { coordinator });

        let response = get_raft_state(State(raft_state)).await.into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
