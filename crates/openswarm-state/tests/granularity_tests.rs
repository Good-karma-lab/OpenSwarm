//! Tests for the Adaptive Granularity Algorithm.
//!
//! Verifies (per §9 of the protocol spec):
//! - Utilization formula: S ≈ min(k, N_branch / k)
//! - Decomposition strategy selection
//! - Redundant execution for atomic tasks

use openswarm_state::granularity::{GranularityEngine, DecompositionStrategy};

// ═══════════════════════════════════════════════════════════════
// § 9.2 Utilization Formula
// ═══════════════════════════════════════════════════════════════

#[test]
fn subtask_count_large_branch() {
    // N_branch=1000, k=10: min(10, 1000/10) = min(10, 100) = 10
    let count = GranularityEngine::optimal_subtask_count(1000, 10);
    assert_eq!(count, 10);
}

#[test]
fn subtask_count_medium_branch() {
    // N_branch=50, k=10: min(10, 50/10) = min(10, 5) = 5
    let count = GranularityEngine::optimal_subtask_count(50, 10);
    assert_eq!(count, 5);
}

#[test]
fn subtask_count_small_branch() {
    // N_branch=5, k=10: min(10, 5/10) = min(10, 0) → at least 1
    let count = GranularityEngine::optimal_subtask_count(5, 10);
    assert!(count >= 1, "Must create at least 1 subtask");
    assert!(count <= 5, "Cannot create more subtasks than agents");
}

#[test]
fn subtask_count_single_agent() {
    let count = GranularityEngine::optimal_subtask_count(1, 10);
    assert_eq!(count, 1, "Single agent: 1 subtask (itself)");
}

#[test]
fn subtask_count_never_exceeds_k() {
    for n_branch in [10, 100, 1000, 10000, 100000] {
        let count = GranularityEngine::optimal_subtask_count(n_branch, 10);
        assert!(
            count <= 10,
            "Subtask count {} must not exceed k=10 for N_branch={}",
            count,
            n_branch
        );
    }
}

#[test]
fn subtask_count_at_least_one() {
    for n_branch in [0, 1, 2, 5, 10, 100] {
        let count = GranularityEngine::optimal_subtask_count(n_branch, 10);
        if n_branch > 0 {
            assert!(count >= 1, "Must have at least 1 subtask for N_branch={}", n_branch);
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// § 9.3 Decomposition Strategy Selection
// ═══════════════════════════════════════════════════════════════

#[test]
fn strategy_massive_parallelism() {
    // N_branch > k^2 = 100 when k=10
    let strategy = GranularityEngine::select_strategy(1000, 10, false);
    assert_eq!(
        strategy,
        DecompositionStrategy::MassiveParallelism,
        "N_branch > k² should trigger massive parallelism"
    );
}

#[test]
fn strategy_standard_decomposition() {
    // k < N_branch ≤ k²
    let strategy = GranularityEngine::select_strategy(50, 10, false);
    assert_eq!(
        strategy,
        DecompositionStrategy::StandardDecomposition,
        "k < N_branch ≤ k² should use standard decomposition"
    );
}

#[test]
fn strategy_direct_assignment() {
    // N_branch ≤ k
    let strategy = GranularityEngine::select_strategy(8, 10, false);
    assert_eq!(
        strategy,
        DecompositionStrategy::DirectAssignment,
        "N_branch ≤ k should use direct assignment"
    );
}

#[test]
fn strategy_redundant_execution() {
    // Task is atomic but branch has multiple agents
    let strategy = GranularityEngine::select_strategy(50, 10, true);
    assert_eq!(
        strategy,
        DecompositionStrategy::RedundantExecution,
        "Atomic task with available agents should use redundant execution"
    );
}

#[test]
fn strategy_atomic_single_agent() {
    // Atomic task with single agent: just direct assignment
    let strategy = GranularityEngine::select_strategy(1, 10, true);
    assert_eq!(
        strategy,
        DecompositionStrategy::DirectAssignment,
        "Atomic task with single agent should use direct assignment"
    );
}

// ═══════════════════════════════════════════════════════════════
// § 9.4 Redundant Execution
// ═══════════════════════════════════════════════════════════════

#[test]
fn redundant_count_capped_at_k() {
    let count = GranularityEngine::redundant_execution_count(100, 10);
    assert!(count <= 10, "Redundant count must not exceed k");
}

#[test]
fn redundant_count_capped_at_n_branch() {
    let count = GranularityEngine::redundant_execution_count(3, 10);
    assert!(count <= 3, "Redundant count must not exceed N_branch");
}

#[test]
fn redundant_count_at_least_one() {
    let count = GranularityEngine::redundant_execution_count(1, 10);
    assert!(count >= 1);
}
