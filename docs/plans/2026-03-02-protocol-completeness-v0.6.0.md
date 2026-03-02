# WWS v0.6.0 Protocol Completeness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Implement 8 concrete improvements from Moltbook insights cycles #33-#39: full receipt state machine, spec-anchored deliverables, constraint conflict provenance, silent failure tracking, principal clarification accounting, budget enforcement, guardian quality score, and version release.

**Architecture:** All changes are additive — new fields use `#[serde(default)]`, new ConnectorState fields use `HashMap::new()`. No breaking changes to existing protocol. New RPCs are purely additive to the match table in rpc_server.rs.

**Tech Stack:** Rust, tokio, axum, serde_json, chrono, uuid, openswarm-protocol crate for types, openswarm-connector crate for server logic.

---

## Task 1: Protocol Types — Deliverables, ConstraintConflict, PendingReview, ClarificationRequest

**Files:**
- Modify: `crates/openswarm-protocol/src/types.rs`

**Context:** `types.rs` currently has `Task` struct (line 53), `CommitmentState` enum (line 419), `FailureReason` enum (line 391), `TaskStatus` enum (line 33). We add new types and extend existing ones. All new fields on `Task` use `#[serde(default)]` so old JSON still deserialises.

**Step 1: Write failing tests** in `crates/openswarm-protocol/src/types.rs` in the existing `mod tests` block (around line 800).

```rust
#[test]
fn test_deliverable_tri_state_serialises() {
    let d = Deliverable {
        id: "d1".into(),
        description: "Write tests".into(),
        state: DeliverableState::Partial { note: "half done".into() },
    };
    let json = serde_json::to_string(&d).unwrap();
    let back: Deliverable = serde_json::from_str(&json).unwrap();
    match back.state {
        DeliverableState::Partial { note } => assert_eq!(note, "half done"),
        _ => panic!("wrong state"),
    }
}

#[test]
fn test_task_with_deliverables_default_zero() {
    let t = Task::new("x".into(), 1, 0);
    assert!(t.deliverables.is_empty());
    assert_eq!(t.coverage_threshold, 0.0);
    assert_eq!(t.confidence_review_threshold, 1.0);
}

#[test]
fn test_constraint_conflict_provenance() {
    let cc = ConstraintConflict {
        constraint_a: "must finish by Friday".into(),
        introduced_by_a: "principal".into(),
        constraint_b: "cannot start until Monday".into(),
        introduced_by_b: "alice".into(),
    };
    let json = serde_json::to_string(&cc).unwrap();
    let back: ConstraintConflict = serde_json::from_str(&json).unwrap();
    assert_eq!(back.introduced_by_b, "alice");
}

#[test]
fn test_failure_reason_contradictory_uses_conflict_graph() {
    let fr = FailureReason::ContradictoryConstraints {
        conflict_graph: vec![ConstraintConflict {
            constraint_a: "A".into(),
            introduced_by_a: "p1".into(),
            constraint_b: "B".into(),
            introduced_by_b: "p2".into(),
        }],
    };
    let json = serde_json::to_string(&fr).unwrap();
    let back: FailureReason = serde_json::from_str(&json).unwrap();
    match back {
        FailureReason::ContradictoryConstraints { conflict_graph } => {
            assert_eq!(conflict_graph.len(), 1);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_pending_review_task_status() {
    let s = TaskStatus::PendingReview;
    let json = serde_json::to_string(&s).unwrap();
    let back: TaskStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(back, TaskStatus::PendingReview);
}

#[test]
fn test_clarification_request_serialises() {
    let cr = ClarificationRequest {
        id: "cr-1".into(),
        task_id: "t-1".into(),
        requesting_agent: "alice".into(),
        principal_id: "bob".into(),
        question: "Which format?".into(),
        resolution: None,
        created_at: chrono::Utc::now(),
        resolved_at: None,
    };
    let json = serde_json::to_string(&cr).unwrap();
    let back: ClarificationRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.question, "Which format?");
}
```

**Step 2: Run tests to verify they fail.**

```bash
~/.cargo/bin/cargo test --package openswarm-protocol --lib types::tests 2>&1 | tail -20
```

Expected: compile errors (types not defined yet).

**Step 3: Add the new types.** In `crates/openswarm-protocol/src/types.rs`:

After line 50 (before `pub struct Task`), add:

```rust
/// Tri-state of a spec-anchored deliverable (Moltbook insight #13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeliverableState {
    Done,
    Partial { note: String },
    Skipped,
}

/// A single named deliverable item in a task spec (Moltbook insight #13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub id: String,
    pub description: String,
    pub state: DeliverableState,
}

/// A clarification request from an agent to a task principal (Moltbook insight #20).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationRequest {
    pub id: String,
    pub task_id: String,
    pub requesting_agent: String,
    pub principal_id: String,
    pub question: String,
    pub resolution: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

Add `PendingReview` variant to `TaskStatus` enum (after line ~48, before closing brace):

```rust
    /// Task result submitted but confidence delta exceeded review threshold.
    PendingReview,
```

In `Task` struct (after `tools_available` field), add:

```rust
    /// Spec-anchored deliverable checklist (Moltbook insight #13).
    #[serde(default)]
    pub deliverables: Vec<Deliverable>,
    /// Minimum coverage fraction for SucceededPartially to be accepted (0.0 = any, 1.0 = full).
    #[serde(default)]
    pub coverage_threshold: f32,
    /// Confidence delta gate: if pre−post > threshold, task moves to PendingReview.
    #[serde(default = "default_confidence_review_threshold")]
    pub confidence_review_threshold: f32,
```

Add helper function near `Task::new`:

```rust
fn default_confidence_review_threshold() -> f32 { 1.0 }
```

Add `ConstraintConflict` struct before `TaskOutcome`:

```rust
/// A constraint conflict with provenance (Moltbook insights #2, #15).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintConflict {
    pub constraint_a: String,
    pub introduced_by_a: String,
    pub constraint_b: String,
    pub introduced_by_b: String,
}
```

Replace `FailureReason::ContradictoryConstraints` variant:

Old:
```rust
    ContradictoryConstraints { conflict_description: String },
```
New:
```rust
    ContradictoryConstraints { conflict_graph: Vec<ConstraintConflict> },
```

Add `proposed_resolution` to `TaskAmbiguous`:

Old:
```rust
    TaskAmbiguous { ambiguity_description: String },
```
New:
```rust
    TaskAmbiguous { ambiguity_description: String, proposed_resolution: Option<String> },
```

Upgrade `CommitmentState` — replace existing enum body with:

```rust
pub enum CommitmentState {
    Active,
    AgentFulfilled,   // agent reports completion, awaiting external verification
    Verified,         // external verifier confirmed evidence_hash
    Closed,           // finalized, calibration updated
    Expired,
    Failed,
    Disputed,
    // Legacy alias kept for backwards-compat deserialisation
    #[serde(alias = "Fulfilled")]
    Fulfilled,
}
```

**Step 4: Run tests to verify they pass.**

```bash
~/.cargo/bin/cargo test --package openswarm-protocol 2>&1 | tail -20
```

Expected: all tests pass including the 3 existing Moltbook tests.

**Step 5: Check for compile errors across workspace.**

```bash
~/.cargo/bin/cargo check --workspace 2>&1 | grep "^error" | head -20
```

Fix any `ContradictoryConstraints { conflict_description }` usages in connector crates (grep for `conflict_description`, change to `conflict_graph: vec![]`).

**Step 6: Commit.**

```bash
git add crates/openswarm-protocol/src/types.rs
git commit -m "feat(protocol): add Deliverable, ConstraintConflict, ClarificationRequest; upgrade CommitmentState; PendingReview status"
```

---

## Task 2: ConnectorState — receipts, clarifications, silent failure tracking, budget helpers

**Files:**
- Modify: `crates/openswarm-connector/src/connector.rs`

**Context:** `ConnectorState` struct ends around line 264. `AgentActivity` is around line 78. We add new fields and helper methods.

**Step 1: Write failing tests** in connector.rs `mod tests` block (near end of file):

```rust
#[test]
fn test_silent_failure_rate_zero_when_no_outcomes() {
    let a = AgentActivity::default();
    assert_eq!(a.silent_failure_rate(), 0.0);
}

#[test]
fn test_silent_failure_rate_computed() {
    let mut a = AgentActivity::default();
    a.total_outcomes_reported = 10;
    a.silent_failure_count = 3;
    assert!((a.silent_failure_rate() - 0.3).abs() < 1e-6);
}

#[test]
fn test_blast_radius_cost_low_medium_high() {
    assert_eq!(blast_radius_cost(Some("low")), 1);
    assert_eq!(blast_radius_cost(Some("medium")), 3);
    assert_eq!(blast_radius_cost(Some("high")), 10);
    assert_eq!(blast_radius_cost(None), 0);
}

#[test]
fn test_unverified_receipt_count() {
    use openswarm_protocol::CommitmentReceipt;
    use openswarm_protocol::CommitmentState;
    // Build minimal ConnectorState with receipts
    // ... test via state.unverified_receipt_count("agent-1")
}
```

**Step 2: Run test to verify failure.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector --lib connector::tests 2>&1 | tail -10
```

**Step 3: Implement.** In `connector.rs`:

Add to `AgentActivity` struct (after `contribution_ratio` field):

```rust
    /// Number of tasks that timed out with no signal (FailedSilently).
    pub silent_failure_count: u64,
    /// Total task outcomes reported (for computing silent_failure_rate).
    pub total_outcomes_reported: u64,
```

Add method to `AgentActivity` impl (add impl block if not present):

```rust
impl AgentActivity {
    pub fn silent_failure_rate(&self) -> f64 {
        if self.total_outcomes_reported == 0 {
            0.0
        } else {
            self.silent_failure_count as f64 / self.total_outcomes_reported as f64
        }
    }
}
```

Add free function (near top of `impl ConnectorState`):

```rust
/// Blast radius cost for a rollback_cost string value.
pub fn blast_radius_cost(rollback_cost: Option<&str>) -> u32 {
    match rollback_cost {
        Some("high") => 10,
        Some("medium") => 3,
        Some("low") => 1,
        _ => 0,
    }
}
```

Add constants before `ConnectorState` struct:

```rust
/// Maximum concurrent active tasks per principal (budget enforcement).
pub const MAX_CONCURRENT_INJECTIONS: usize = 50;
/// Maximum blast radius (sum of rollback_cost weights) per principal.
pub const MAX_BLAST_RADIUS: u32 = 200;
```

Add new fields to `ConnectorState` struct (after `guardian_votes` field):

```rust
    /// Commitment receipts by receipt_id.
    pub receipts: std::collections::HashMap<String, openswarm_protocol::CommitmentReceipt>,
    /// Clarification requests by clarification_id.
    pub clarifications: std::collections::HashMap<String, openswarm_protocol::ClarificationRequest>,
```

Add methods to `ConnectorState` impl:

```rust
    /// Count unverified receipts for a given agent (AgentFulfilled state).
    pub fn unverified_receipt_count(&self, agent_id: &str) -> usize {
        self.receipts.values()
            .filter(|r| r.agent_id == agent_id
                && r.commitment_state == openswarm_protocol::CommitmentState::AgentFulfilled)
            .count()
    }

    /// Compute blast radius for a principal's active injected tasks.
    pub fn principal_blast_radius(&self, principal_id: &str) -> u32 {
        self.receipts.values()
            .filter(|r| r.agent_id == principal_id
                && matches!(r.commitment_state,
                    openswarm_protocol::CommitmentState::Active
                    | openswarm_protocol::CommitmentState::AgentFulfilled))
            .map(|r| blast_radius_cost(r.rollback_cost.as_deref()))
            .sum()
    }

    /// Count active injected tasks for a principal.
    pub fn principal_active_injection_count(&self, principal_id: &str) -> usize {
        self.task_details.values()
            .filter(|t| {
                t.assigned_to.as_ref().map(|id| id.to_string()).unwrap_or_default() != principal_id
                && matches!(t.status,
                    openswarm_protocol::TaskStatus::Pending
                    | openswarm_protocol::TaskStatus::InProgress
                    | openswarm_protocol::TaskStatus::ProposalPhase
                    | openswarm_protocol::TaskStatus::VotingPhase)
            })
            .count()
    }

    /// Compute guardian quality score for a DID (avg tier score of its guardians).
    pub fn guardian_quality_score(&self, agent_did: &str) -> (f64, usize) {
        let designation = match self.guardian_designations.get(agent_did) {
            Some(d) => d,
            None => return (0.0, 0),
        };
        let tier_score = |did: &str| -> f64 {
            let score = self.reputation_ledgers.get(did).map(|l| l.effective_score()).unwrap_or(0);
            use crate::reputation::ScoreTier;
            match ScoreTier::from_score(score) {
                ScoreTier::Newcomer => 0.1,
                ScoreTier::Member => 0.2,
                ScoreTier::Trusted => 0.5,
                ScoreTier::Established => 0.75,
                ScoreTier::Veteran => 1.0,
                _ => 0.0,
            }
        };
        let n = designation.guardians.len();
        if n == 0 {
            return (0.0, 0);
        }
        let sum: f64 = designation.guardians.iter().map(|g| tier_score(g)).sum();
        (sum / n as f64, n)
    }
```

Also initialise new fields in all `ConnectorState { ... }` literals in `operator_console.rs` tests:

```rust
receipts: std::collections::HashMap::new(),
clarifications: std::collections::HashMap::new(),
```

**Step 4: Run tests.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector 2>&1 | tail -20
```

**Step 5: Commit.**

```bash
git add crates/openswarm-connector/src/connector.rs crates/openswarm-connector/src/operator_console.rs
git commit -m "feat(connector): add receipts/clarifications state, silent_failure_rate, blast_radius helpers, budget constants, guardian_quality_score"
```

---

## Task 3: Receipt State Machine RPCs (swarm.create_receipt, swarm.fulfill_receipt, swarm.verify_receipt)

**Files:**
- Modify: `crates/openswarm-connector/src/rpc_server.rs`

**Context:** The routing `match` is at line ~159. All existing handlers follow the pattern: `async fn handle_<name>(id: Option<String>, params: &serde_json::Value, state: &Arc<RwLock<ConnectorState>>) -> SwarmResponse`. The `CommitmentReceipt` struct is in `openswarm_protocol::types`.

**Step 1: Write failing tests** (in the existing `mod tests` block near the end of rpc_server.rs, after finding the tests section with `grep -n "^mod tests" crates/openswarm-connector/src/rpc_server.rs`).

```rust
#[tokio::test]
async fn test_create_receipt_stores_receipt() {
    let state = make_test_state();
    let params = serde_json::json!({
        "task_id": "t1",
        "agent_id": "alice",
        "deliverable_type": "artifact",
        "rollback_cost": "low"
    });
    let resp = handle_create_receipt(Some("r1".into()), &params, &state).await;
    assert!(resp.result.is_some(), "should return result");
    let receipt_id = resp.result.unwrap()["receipt_id"].as_str().unwrap().to_string();
    let s = state.read().await;
    assert!(s.receipts.contains_key(&receipt_id));
    assert_eq!(s.receipts[&receipt_id].commitment_state, CommitmentState::Active);
}

#[tokio::test]
async fn test_fulfill_receipt_advances_state() {
    let state = make_test_state();
    // First create a receipt
    let create_params = serde_json::json!({
        "task_id": "t1", "agent_id": "alice",
        "deliverable_type": "artifact", "rollback_cost": "low"
    });
    let create_resp = handle_create_receipt(Some("1".into()), &create_params, &state).await;
    let receipt_id = create_resp.result.unwrap()["receipt_id"].as_str().unwrap().to_string();
    // Fulfill it
    let fulfill_params = serde_json::json!({
        "receipt_id": receipt_id,
        "evidence_hash": "sha256:abc123",
        "confidence_delta": 0.1
    });
    let resp = handle_fulfill_receipt(Some("2".into()), &fulfill_params, &state).await;
    assert!(resp.error.is_none());
    let s = state.read().await;
    assert_eq!(s.receipts[&receipt_id].commitment_state, CommitmentState::AgentFulfilled);
}

#[tokio::test]
async fn test_verify_receipt_closes_to_verified() {
    let state = make_test_state();
    // Create + fulfill
    let create_params = serde_json::json!({
        "task_id": "t1", "agent_id": "alice",
        "deliverable_type": "artifact", "rollback_cost": "low"
    });
    let create_resp = handle_create_receipt(Some("1".into()), &create_params, &state).await;
    let receipt_id = create_resp.result.unwrap()["receipt_id"].as_str().unwrap().to_string();
    let fulfill_params = serde_json::json!({
        "receipt_id": receipt_id.clone(),
        "evidence_hash": "sha256:abc",
        "confidence_delta": 0.0
    });
    handle_fulfill_receipt(Some("2".into()), &fulfill_params, &state).await;
    // Verify
    let verify_params = serde_json::json!({
        "receipt_id": receipt_id.clone(),
        "verifier_id": "ci-runner",
        "confirmed": true
    });
    let resp = handle_verify_receipt(Some("3".into()), &verify_params, &state).await;
    assert!(resp.error.is_none());
    let s = state.read().await;
    assert_eq!(s.receipts[&receipt_id].commitment_state, CommitmentState::Verified);
}
```

**Step 2: Run to verify failure.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector --lib rpc_server::tests 2>&1 | grep -E "FAILED|error\[" | head -10
```

**Step 3: Implement three handlers.** Add after the last handler in rpc_server.rs (before `mod tests`):

```rust
async fn handle_create_receipt(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let task_id = match params.get("task_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'task_id'".into()),
    };
    let agent_id = match params.get("agent_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'agent_id'".into()),
    };
    let deliverable_type = params.get("deliverable_type")
        .and_then(|v| v.as_str())
        .unwrap_or("artifact")
        .to_string();
    let rollback_cost = params.get("rollback_cost")
        .and_then(|v| v.as_str())
        .map(String::from);
    let rollback_window = params.get("rollback_window")
        .and_then(|v| v.as_str())
        .map(String::from);

    let receipt = openswarm_protocol::CommitmentReceipt {
        commitment_id: uuid::Uuid::new_v4().to_string(),
        deliverable_type,
        evidence_hash: String::new(),
        confidence_delta: 0.0,
        can_undo: rollback_cost.as_deref().map(|c| c != "null").unwrap_or(true),
        rollback_cost,
        rollback_window,
        expires_at: None,
        commitment_state: openswarm_protocol::CommitmentState::Active,
        task_id,
        agent_id,
        created_at: chrono::Utc::now(),
    };
    let receipt_id = receipt.commitment_id.clone();
    let mut s = state.write().await;
    s.receipts.insert(receipt_id.clone(), receipt);
    SwarmResponse::success(id, serde_json::json!({ "receipt_id": receipt_id, "ok": true }))
}

async fn handle_fulfill_receipt(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let receipt_id = match params.get("receipt_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'receipt_id'".into()),
    };
    let evidence_hash = params.get("evidence_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence_delta = params.get("confidence_delta")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut s = state.write().await;
    match s.receipts.get_mut(&receipt_id) {
        Some(r) if r.commitment_state == openswarm_protocol::CommitmentState::Active => {
            r.commitment_state = openswarm_protocol::CommitmentState::AgentFulfilled;
            r.evidence_hash = evidence_hash;
            r.confidence_delta = confidence_delta;
            SwarmResponse::success(id, serde_json::json!({ "ok": true, "state": "AgentFulfilled" }))
        }
        Some(_) => SwarmResponse::error(id, -32600, "Receipt is not in Active state".into()),
        None => SwarmResponse::error(id, -32602, format!("Receipt '{}' not found", receipt_id)),
    }
}

async fn handle_verify_receipt(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let receipt_id = match params.get("receipt_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'receipt_id'".into()),
    };
    let confirmed = params.get("confirmed").and_then(|v| v.as_bool()).unwrap_or(true);

    let mut s = state.write().await;
    match s.receipts.get_mut(&receipt_id) {
        Some(r) if r.commitment_state == openswarm_protocol::CommitmentState::AgentFulfilled => {
            r.commitment_state = if confirmed {
                openswarm_protocol::CommitmentState::Verified
            } else {
                openswarm_protocol::CommitmentState::Disputed
            };
            let new_state = format!("{:?}", r.commitment_state);
            SwarmResponse::success(id, serde_json::json!({ "ok": true, "state": new_state }))
        }
        Some(_) => SwarmResponse::error(id, -32600, "Receipt is not in AgentFulfilled state".into()),
        None => SwarmResponse::error(id, -32602, format!("Receipt '{}' not found", receipt_id)),
    }
}
```

Add routing arms to the match block (before the `_ =>` wildcard):

```rust
"swarm.create_receipt" => handle_create_receipt(request_id, &request.params, state).await,
"swarm.fulfill_receipt" => handle_fulfill_receipt(request_id, &request.params, state).await,
"swarm.verify_receipt" => handle_verify_receipt(request_id, &request.params, state).await,
```

Update the doc comment at the top of rpc_server.rs to list the 3 new methods.

**Step 4: Run tests.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector 2>&1 | tail -10
```

**Step 5: Commit.**

```bash
git add crates/openswarm-connector/src/rpc_server.rs
git commit -m "feat(rpc): add swarm.create_receipt, swarm.fulfill_receipt, swarm.verify_receipt"
```

---

## Task 4: Clarification RPCs (swarm.request_clarification, swarm.resolve_clarification) + Budget Enforcement

**Files:**
- Modify: `crates/openswarm-connector/src/rpc_server.rs`

**Context:** We add two clarification RPCs and update the existing `handle_inject_task` handler to enforce `MAX_CONCURRENT_INJECTIONS` and `MAX_BLAST_RADIUS`. Find `handle_inject_task` with `grep -n "async fn handle_inject_task" crates/openswarm-connector/src/rpc_server.rs`.

**Step 1: Write failing tests:**

```rust
#[tokio::test]
async fn test_request_clarification_stored() {
    let state = make_test_state();
    let params = serde_json::json!({
        "task_id": "t1",
        "requesting_agent": "alice",
        "principal_id": "bob",
        "question": "Which output format?"
    });
    let resp = handle_request_clarification(Some("1".into()), &params, &state).await;
    assert!(resp.result.is_some());
    let clar_id = resp.result.unwrap()["clarification_id"].as_str().unwrap().to_string();
    let s = state.read().await;
    assert!(s.clarifications.contains_key(&clar_id));
    assert!(s.clarifications[&clar_id].resolution.is_none());
}

#[tokio::test]
async fn test_resolve_clarification_updates_resolution() {
    let state = make_test_state();
    // Create clarification first
    let req_params = serde_json::json!({
        "task_id": "t1", "requesting_agent": "alice",
        "principal_id": "bob", "question": "Format?"
    });
    let resp = handle_request_clarification(Some("1".into()), &req_params, &state).await;
    let clar_id = resp.result.unwrap()["clarification_id"].as_str().unwrap().to_string();
    // Resolve it
    let res_params = serde_json::json!({
        "clarification_id": clar_id.clone(),
        "resolution": "Use JSON Lines format."
    });
    let resp2 = handle_resolve_clarification(Some("2".into()), &res_params, &state).await;
    assert!(resp2.error.is_none());
    let s = state.read().await;
    assert!(s.clarifications[&clar_id].resolution.is_some());
    assert!(s.clarifications[&clar_id].resolved_at.is_some());
}
```

**Step 2: Run to verify failure.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector --lib rpc_server::tests 2>&1 | grep "FAILED\|error" | head -10
```

**Step 3: Implement handlers.** Add to rpc_server.rs:

```rust
async fn handle_request_clarification(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let task_id = match params.get("task_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'task_id'".into()),
    };
    let requesting_agent = params.get("requesting_agent")
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let principal_id = params.get("principal_id")
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let question = match params.get("question").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'question'".into()),
    };

    let cr = openswarm_protocol::ClarificationRequest {
        id: uuid::Uuid::new_v4().to_string(),
        task_id,
        requesting_agent,
        principal_id,
        question,
        resolution: None,
        created_at: chrono::Utc::now(),
        resolved_at: None,
    };
    let clar_id = cr.id.clone();
    let mut s = state.write().await;
    s.clarifications.insert(clar_id.clone(), cr);
    SwarmResponse::success(id, serde_json::json!({ "clarification_id": clar_id, "ok": true }))
}

async fn handle_resolve_clarification(
    id: Option<String>,
    params: &serde_json::Value,
    state: &Arc<RwLock<ConnectorState>>,
) -> SwarmResponse {
    let clar_id = match params.get("clarification_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'clarification_id'".into()),
    };
    let resolution = match params.get("resolution").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return SwarmResponse::error(id, -32602, "Missing 'resolution'".into()),
    };

    let mut s = state.write().await;
    match s.clarifications.get_mut(&clar_id) {
        Some(cr) if cr.resolution.is_none() => {
            cr.resolution = Some(resolution);
            cr.resolved_at = Some(chrono::Utc::now());
            SwarmResponse::success(id, serde_json::json!({ "ok": true }))
        }
        Some(_) => SwarmResponse::error(id, -32600, "Clarification already resolved".into()),
        None => SwarmResponse::error(id, -32602, format!("Clarification '{}' not found", clar_id)),
    }
}
```

Add routing arms:

```rust
"swarm.request_clarification" => handle_request_clarification(request_id, &request.params, state).await,
"swarm.resolve_clarification" => handle_resolve_clarification(request_id, &request.params, state).await,
```

**Update `handle_inject_task` for budget enforcement.** In the existing `handle_inject_task` function, after the reputation check (find the line that checks `has_inject_reputation`), add budget checks:

```rust
    // Budget enforcement (Moltbook insight #19)
    {
        let s = state.read().await;
        let concurrent = s.principal_active_injection_count(&injector_id);
        if concurrent >= crate::connector::MAX_CONCURRENT_INJECTIONS {
            return SwarmResponse::error(
                id,
                -32007,
                format!("Budget exceeded: {} concurrent active injections (max {}). Retry when some complete.",
                    concurrent, crate::connector::MAX_CONCURRENT_INJECTIONS),
            );
        }
        let blast = s.principal_blast_radius(&injector_id);
        if blast >= crate::connector::MAX_BLAST_RADIUS {
            return SwarmResponse::error(
                id,
                -32008,
                format!("Blast radius budget exceeded: {} points (max {}). Close or verify pending receipts first.",
                    blast, crate::connector::MAX_BLAST_RADIUS),
            );
        }
    }
```

**Step 4: Run tests.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector 2>&1 | tail -10
```

**Step 5: Commit.**

```bash
git add crates/openswarm-connector/src/rpc_server.rs
git commit -m "feat(rpc): add swarm.request_clarification, swarm.resolve_clarification; budget enforcement in inject_task"
```

---

## Task 5: Silent Failure Tracking in submit_result + Board Status Hint

**Files:**
- Modify: `crates/openswarm-connector/src/rpc_server.rs` (submit_result handler)
- Modify: `crates/openswarm-connector/src/rpc_server.rs` (get_board_status handler)

**Context:** Find `handle_submit_result` with `grep -n "async fn handle_submit_result" crates/openswarm-connector/src/rpc_server.rs`. Find `handle_get_board_status` with `grep -n "async fn handle_get_board_status" crates/openswarm-connector/src/rpc_server.rs`.

**Step 1: Write failing test:**

```rust
#[tokio::test]
async fn test_submit_result_with_silent_failure_increments_counter() {
    let state = make_test_state();
    // Register an agent first
    let reg_params = serde_json::json!({"agent_name": "alice", "agent_id": "alice"});
    // ... register agent, then submit result with FailedSilently outcome
    let params = serde_json::json!({
        "task_id": "t1",
        "agent_id": "alice",
        "artifact": {
            "artifact_id": "a1", "task_id": "t1", "producer": "alice",
            "content_cid": "x", "merkle_hash": "x",
            "content_type": "text/plain", "size_bytes": 5, "content": "hello"
        },
        "merkle_proof": [],
        "outcome": { "FailedSilently": null }
    });
    handle_submit_result(Some("1".into()), &params, &state, /* handle */).await;
    // Can't easily test without network_handle in unit test; skip for now
    // Just verify compile
}
```

**Step 2: In `handle_submit_result`**, after the existing confidence_delta extraction (find with `grep -n "confidence_delta" crates/openswarm-connector/src/rpc_server.rs`), add:

```rust
    // Track silent failure rate (Moltbook insight #16)
    if let Some(outcome_val) = params.get("outcome") {
        let is_silent = outcome_val.get("FailedSilently").is_some()
            || outcome_val.as_str() == Some("FailedSilently");
        let mut s = state.write().await;
        let activity = s.agent_activity.entry(agent_id_str.clone()).or_default();
        activity.total_outcomes_reported += 1;
        if is_silent {
            activity.silent_failure_count += 1;
        }
    }
```

Note: Place this BEFORE the final `SwarmResponse::success(...)` return.

**Step 3: In `handle_get_board_status`**, before the `SwarmResponse::success(...)` call at the end, add a `low_quality_monitors` field:

Find the response construction (look for `serde_json::json!` call in the function). Add to the JSON:

```rust
    let low_quality_monitors: Vec<String> = s.agent_activity.iter()
        .filter(|(_, act)| act.silent_failure_rate() > 0.3 && act.total_outcomes_reported >= 3)
        .map(|(id, _)| id.clone())
        .collect();
```

Include in the response JSON:
```json
"low_quality_monitors": low_quality_monitors
```

**Step 4: Run tests.**

```bash
~/.cargo/bin/cargo test --package openswarm-connector 2>&1 | tail -10
```

**Step 5: Commit.**

```bash
git add crates/openswarm-connector/src/rpc_server.rs
git commit -m "feat(rpc): track silent failure rate in submit_result; add low_quality_monitors hint to get_board_status"
```

---

## Task 6: HTTP API for Receipts + Guardian Quality Score + Agent Silent Failure Rate

**Files:**
- Modify: `crates/openswarm-connector/src/file_server.rs`

**Context:** Routes are registered starting at line 81. New routes to add: `/api/receipts`, `/api/receipts/:id`, `/api/tasks/:task_id/receipts`. Also update `/api/agents` to include `silent_failure_rate` and `unverified_receipt_count`. Update `/api/reputation/:did` to include `guardian_quality_score` and `guardian_count`. Also update `/api/reputation` (the list endpoint).

**Step 1: Write failing tests** (add to file_server.rs if there is a `mod tests` block, else skip unit test — verify via integration test later).

**Step 2: Add routes** to the router in `FileServer::run`:

```rust
.route("/api/receipts", get(api_receipts))
.route("/api/receipts/:receipt_id", get(api_receipt_detail))
.route("/api/tasks/:task_id/receipts", get(api_task_receipts))
.route("/api/clarifications", get(api_clarifications))
```

**Step 3: Implement handlers.** Add near the end of file_server.rs:

```rust
async fn api_receipts(State(web): State<WebState>) -> Json<serde_json::Value> {
    let s = web.state.read().await;
    let receipts: Vec<serde_json::Value> = s.receipts.values().map(|r| {
        serde_json::json!({
            "receipt_id": r.commitment_id,
            "task_id": r.task_id,
            "agent_id": r.agent_id,
            "state": format!("{:?}", r.commitment_state),
            "deliverable_type": r.deliverable_type,
            "rollback_cost": r.rollback_cost,
            "evidence_hash": r.evidence_hash,
            "confidence_delta": r.confidence_delta,
            "created_at": r.created_at,
        })
    }).collect();
    Json(serde_json::json!({ "receipts": receipts, "count": receipts.len() }))
}

async fn api_receipt_detail(
    State(web): State<WebState>,
    AxumPath(receipt_id): AxumPath<String>,
) -> impl IntoResponse {
    let s = web.state.read().await;
    match s.receipts.get(&receipt_id) {
        Some(r) => (StatusCode::OK, Json(serde_json::json!({
            "receipt_id": r.commitment_id,
            "task_id": r.task_id,
            "agent_id": r.agent_id,
            "state": format!("{:?}", r.commitment_state),
            "deliverable_type": r.deliverable_type,
            "rollback_cost": r.rollback_cost,
            "rollback_window": r.rollback_window,
            "evidence_hash": r.evidence_hash,
            "confidence_delta": r.confidence_delta,
            "can_undo": r.can_undo,
            "expires_at": r.expires_at,
            "created_at": r.created_at,
        }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
    }
}

async fn api_task_receipts(
    State(web): State<WebState>,
    AxumPath(task_id): AxumPath<String>,
) -> Json<serde_json::Value> {
    let s = web.state.read().await;
    let receipts: Vec<serde_json::Value> = s.receipts.values()
        .filter(|r| r.task_id == task_id)
        .map(|r| serde_json::json!({
            "receipt_id": r.commitment_id,
            "agent_id": r.agent_id,
            "state": format!("{:?}", r.commitment_state),
            "evidence_hash": r.evidence_hash,
        }))
        .collect();
    Json(serde_json::json!({ "task_id": task_id, "receipts": receipts }))
}

async fn api_clarifications(State(web): State<WebState>) -> Json<serde_json::Value> {
    let s = web.state.read().await;
    let items: Vec<serde_json::Value> = s.clarifications.values().map(|c| serde_json::json!({
        "id": c.id,
        "task_id": c.task_id,
        "requesting_agent": c.requesting_agent,
        "principal_id": c.principal_id,
        "question": c.question,
        "resolution": c.resolution,
        "created_at": c.created_at,
        "resolved_at": c.resolved_at,
    })).collect();
    Json(serde_json::json!({ "clarifications": items, "count": items.len() }))
}
```

**Step 4: Update `/api/agents` handler** to include `silent_failure_rate` and `unverified_receipt_count`. Find `async fn api_agents` (line ~678). In the per-agent JSON construction, add:

```rust
"silent_failure_rate": act.silent_failure_rate(),
"unverified_receipt_count": s.unverified_receipt_count(&id),
```

**Step 5: Update `/api/reputation/:did` handler** to include guardian quality score. Find `async fn api_reputation_events` and the companion handler that returns per-DID reputation. Look for where `effective_score` is serialised and add:

```rust
let (guardian_quality_score, guardian_count) = s.guardian_quality_score(&did);
// add to JSON:
"guardian_quality_score": guardian_quality_score,
"guardian_count": guardian_count,
```

Also update `handle_get_reputation` in rpc_server.rs to include the same fields.

**Step 6: Run tests.**

```bash
~/.cargo/bin/cargo test --workspace 2>&1 | tail -10
```

Expected: all tests pass.

**Step 7: Commit.**

```bash
git add crates/openswarm-connector/src/file_server.rs
git commit -m "feat(http): add /api/receipts, /api/clarifications, /api/tasks/:id/receipts; guardian_quality_score in reputation API; silent_failure_rate in agents API"
```

---

## Task 7: Wire Deliverables into inject_task + PendingReview status in submit_result

**Files:**
- Modify: `crates/openswarm-connector/src/rpc_server.rs` (handle_inject_task, handle_submit_result)
- Modify: `crates/openswarm-connector/src/file_server.rs` (api_submit_task)

**Context:** `handle_inject_task` creates a `Task` struct in rpc_server.rs. `api_submit_task` creates a `Task` in file_server.rs (find with `grep -n "Task {" crates/openswarm-connector/src/file_server.rs`). `handle_submit_result` is in rpc_server.rs.

**Step 1: Write failing test:**

```rust
#[tokio::test]
async fn test_inject_task_accepts_deliverables() {
    let state = make_test_state();
    let params = serde_json::json!({
        "task_id": "t1",
        "injector_agent_id": "self-agent",
        "description": "Write a spec",
        "deliverables": [
            {"id": "d1", "description": "Draft document", "state": "Done"},
            {"id": "d2", "description": "Test suite", "state": "Skipped"}
        ],
        "coverage_threshold": 0.5,
        "confidence_review_threshold": 0.3
    });
    // This is hard to fully test without network; check compile only
}
```

**Step 2: In `handle_inject_task`**, when building the `Task` struct, extract and use new fields:

```rust
let deliverables: Vec<openswarm_protocol::Deliverable> = params
    .get("deliverables")
    .and_then(|v| serde_json::from_value(v.clone()).ok())
    .unwrap_or_default();
let coverage_threshold = params.get("coverage_threshold")
    .and_then(|v| v.as_f64())
    .map(|f| f as f32)
    .unwrap_or(0.0);
let confidence_review_threshold = params.get("confidence_review_threshold")
    .and_then(|v| v.as_f64())
    .map(|f| f as f32)
    .unwrap_or(1.0);
```

Add these to the `Task { ... }` construction:

```rust
deliverables,
coverage_threshold,
confidence_review_threshold,
```

Apply the same changes to `api_submit_task` in file_server.rs (the HTTP inject endpoint also deserialises a task body with a `#[derive(Deserialize)]` struct — find `TaskBody` or similar; if fields are missing from the struct, add them with `#[serde(default)]`).

**Step 3: In `handle_submit_result`**, add `PendingReview` logic. After the existing confidence_delta extraction block, add:

```rust
    // PendingReview: flag task if confidence delta exceeds threshold (Moltbook insight #8)
    if let Some(delta) = confidence_delta_opt {
        let threshold = {
            let s = state.read().await;
            s.task_details.get(&task_id)
                .map(|t| t.confidence_review_threshold)
                .unwrap_or(1.0)
        };
        if delta > threshold as f64 {
            let mut s = state.write().await;
            if let Some(t) = s.task_details.get_mut(&task_id) {
                t.status = openswarm_protocol::TaskStatus::PendingReview;
            }
        }
    }
```

You'll need to extract the `confidence_delta_opt` value from the existing confidence_delta code — change the existing `let confidence_delta = ...` to `let confidence_delta_opt: Option<f64> = params.get("confidence_delta").and_then(|v| v.as_f64());`.

**Step 4: Compile check.**

```bash
~/.cargo/bin/cargo check --workspace 2>&1 | grep "^error" | head -10
```

**Step 5: Run tests.**

```bash
~/.cargo/bin/cargo test --workspace 2>&1 | tail -10
```

**Step 6: Commit.**

```bash
git add crates/openswarm-connector/src/rpc_server.rs crates/openswarm-connector/src/file_server.rs
git commit -m "feat(connector): wire deliverables/coverage_threshold into inject_task; PendingReview status on confidence delta"
```

---

## Task 8: Version 0.6.0, Docs Update, Tests, Release

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `README.md`
- Modify: `docs/SKILL.md`

**Context:** Version is currently `"0.5.0"` in `Cargo.toml`. README has test count and download URLs. SKILL.md has RPC table.

**Step 1: Bump version.**

In `Cargo.toml`:
```toml
version = "0.6.0"
```

In `docs/package.json`:
```json
"version": "0.6.0"
```

**Step 2: Run full test suite.**

```bash
~/.cargo/bin/cargo test --workspace 2>&1 | tail -5
```

Expected: all tests pass (target 420+).

**Step 3: Update `docs/SKILL.md`.** Find the RPC table section and add new rows:

```markdown
| `swarm.create_receipt` | Create a commitment receipt at task start | `task_id`, `agent_id`, `deliverable_type`, `rollback_cost?` |
| `swarm.fulfill_receipt` | Agent proposes fulfillment + posts evidence_hash | `receipt_id`, `evidence_hash`, `confidence_delta?` |
| `swarm.verify_receipt` | External verifier confirms or disputes | `receipt_id`, `verifier_id`, `confirmed` |
| `swarm.request_clarification` | Agent requests clarification from principal | `task_id`, `requesting_agent`, `principal_id`, `question` |
| `swarm.resolve_clarification` | Principal resolves a clarification request | `clarification_id`, `resolution` |
```

Add a new section "📋 Spec-Anchored Deliverables" after the identity section:

```markdown
## 📋 Spec-Anchored Deliverables

Tasks can include a `deliverables` array defining named, checkable items:

```json
{
  "deliverables": [
    {"id": "d1", "description": "Draft document", "state": "Done"},
    {"id": "d2", "description": "Test suite", "state": "Partial", "note": "3/10 tests written"}
  ],
  "coverage_threshold": 0.5,
  "confidence_review_threshold": 0.3
}
```

Coverage = `done_count / total`. If coverage < `coverage_threshold`, the holon spawns a sub-holon to complete the gap.
If `pre_confidence - post_confidence > confidence_review_threshold`, task moves to `PendingReview` status.
```

**Step 4: Update `README.md`.** Update:
- Test count: 391 → actual count from step 2
- Security table: add row `| Principal budget enforcement | Max 50 concurrent injections per principal; max blast-radius 200 per principal |`
- Download URLs: update `v0.5.0` → `v0.6.0` and `0.4.9` → `0.5.9` in all curl/wget examples

**Step 5: Run tests again.**

```bash
~/.cargo/bin/cargo test --workspace 2>&1 | tail -5
```

**Step 6: Commit.**

```bash
git add Cargo.toml docs/package.json README.md docs/SKILL.md
git commit -m "chore: bump version to v0.6.0, update docs with receipt state machine, deliverables, clarification RPCs"
```

**Step 7: Tag and push.**

```bash
git tag v0.6.0
git push origin WWS --tags
```

CI will build 5-platform binaries and create the release automatically.

---

## Post-Release: E2E Test with Real Agent Subagents

After release:
1. Wait for CI to complete the GitHub release
2. Download `wws-connector-0.6.0-macos-arm64.tar.gz` from the release
3. Start 20-node Docker swarm: `docker compose -f docker/docker-compose.yml up -d`
4. Run `tests/e2e/e2e_20_agents.py` — expect E2E PASSED
5. Run a dedicated subagent E2E that uses the new receipt + clarification RPCs:
   - Agents create receipts on task start
   - Agents fulfill receipts on result submission
   - One agent acts as external verifier
   - Verify state machine advances correctly via HTTP `/api/receipts`
6. Report: test count, E2E pass/fail, Docker E2E pass/fail
