//! Adaptive Granularity Algorithm for optimal task decomposition depth.
//!
//! Determines how many subtasks to create when decomposing a task,
//! and whether to continue decomposing or execute atomically.
//!
//! The core utilisation formula:
//!   S = min(k, max(1, N_branch / k))
//!
//! Where:
//! - N_branch = number of agents in the current coordinator's branch
//! - k = branching factor of the hierarchy
//!
//! Additional rules:
//! - If the task is atomic but multiple agents are available,
//!   use redundant execution for reliability.
//! - Strategy selection depends on the ratio of N_branch to k.

use std::cmp;

use openswarm_protocol::DEFAULT_BRANCHING_FACTOR;

// ═══════════════════════════════════════════════════════════════
// Static GranularityEngine (used by tests and protocol layer)
// ═══════════════════════════════════════════════════════════════

/// Strategy for how a task should be decomposed at a given tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecompositionStrategy {
    /// N_branch >> k^2: massive fan-out across many coordinators.
    MassiveParallelism,
    /// k < N_branch <= k^2: standard recursive decomposition.
    StandardDecomposition,
    /// N_branch <= k: assign directly to subordinate executors.
    DirectAssignment,
    /// Atomic task with multiple agents: run on several executors
    /// and accept the first (or majority) result.
    RedundantExecution,
}

/// Static utility for computing decomposition parameters.
///
/// All methods are stateless and take the branch size and branching
/// factor as arguments. Use this when you do not need instance-level
/// configuration (e.g., from tests or protocol-level code).
pub struct GranularityEngine;

impl GranularityEngine {
    /// Compute the optimal number of subtasks.
    ///
    /// Formula: `min(k, max(1, N_branch / k))`
    ///
    /// Ensures the count is always at least 1 and never exceeds k.
    pub fn optimal_subtask_count(n_branch: u64, k: u32) -> u32 {
        let k_u64 = k as u64;
        let raw = if k_u64 > 0 { n_branch / k_u64 } else { 1 };
        cmp::min(k, cmp::max(1, raw as u32))
    }

    /// Select the decomposition strategy based on swarm parameters.
    ///
    /// Decision tree:
    /// 1. Atomic task with N_branch > 1 => RedundantExecution
    /// 2. Atomic task with N_branch <= 1 => DirectAssignment
    /// 3. N_branch > k^2 => MassiveParallelism
    /// 4. N_branch > k   => StandardDecomposition
    /// 5. Otherwise       => DirectAssignment
    pub fn select_strategy(n_branch: u64, k: u32, is_atomic: bool) -> DecompositionStrategy {
        if is_atomic {
            if n_branch > 1 {
                return DecompositionStrategy::RedundantExecution;
            } else {
                return DecompositionStrategy::DirectAssignment;
            }
        }

        let k_u64 = k as u64;
        if n_branch > k_u64 * k_u64 {
            DecompositionStrategy::MassiveParallelism
        } else if n_branch > k_u64 {
            DecompositionStrategy::StandardDecomposition
        } else {
            DecompositionStrategy::DirectAssignment
        }
    }

    /// Compute how many executors should redundantly execute an atomic task.
    ///
    /// Returns `min(N_branch, k)`, with a floor of 1.
    pub fn redundant_execution_count(n_branch: u64, k: u32) -> u32 {
        let capped = cmp::min(n_branch, k as u64);
        cmp::max(1, capped as u32)
    }
}

// ═══════════════════════════════════════════════════════════════
// Instance-based GranularityAlgorithm (used by connector)
// ═══════════════════════════════════════════════════════════════

/// Configuration for the instance-based granularity algorithm.
#[derive(Debug, Clone)]
pub struct GranularityConfig {
    /// Branching factor (k) of the hierarchy.
    pub branching_factor: u32,
    /// Minimum subtask count below which atomic execution is preferred.
    pub min_subtasks: u32,
    /// Maximum subtask count (capped at branching factor).
    pub max_subtasks: u32,
    /// Number of redundant executors for atomic tasks.
    pub redundancy_factor: u32,
    /// Complexity threshold above which decomposition is always preferred.
    pub decompose_complexity_threshold: f64,
    /// Minimum branch size for decomposition to make sense.
    pub min_branch_size_for_decomposition: u64,
}

impl Default for GranularityConfig {
    fn default() -> Self {
        Self {
            branching_factor: DEFAULT_BRANCHING_FACTOR,
            min_subtasks: 2,
            max_subtasks: DEFAULT_BRANCHING_FACTOR,
            redundancy_factor: 3,
            decompose_complexity_threshold: 0.7,
            min_branch_size_for_decomposition: 3,
        }
    }
}

/// Decision from the instance-based granularity algorithm.
#[derive(Debug, Clone, PartialEq)]
pub enum GranularityDecision {
    /// Decompose the task into the specified number of subtasks.
    Decompose {
        subtask_count: u32,
        agents_per_subtask: u64,
    },
    /// Execute the task atomically with redundant executors.
    ExecuteAtomic {
        redundancy: u32,
    },
}

/// Instance-based granularity algorithm with configuration.
///
/// Uses branch size, branching factor, and task complexity
/// to determine whether to decompose further or execute atomically.
pub struct GranularityAlgorithm {
    config: GranularityConfig,
}

impl GranularityAlgorithm {
    /// Create a new granularity algorithm with the given configuration.
    pub fn new(config: GranularityConfig) -> Self {
        Self { config }
    }

    /// Compute the optimal decomposition for a task.
    pub fn compute(
        &self,
        branch_size: u64,
        estimated_complexity: f64,
        current_depth: u32,
        max_depth: u32,
    ) -> GranularityDecision {
        let k = self.config.branching_factor as u64;

        if current_depth >= max_depth {
            return GranularityDecision::ExecuteAtomic {
                redundancy: self.compute_redundancy(branch_size),
            };
        }

        if branch_size < self.config.min_branch_size_for_decomposition {
            return GranularityDecision::ExecuteAtomic {
                redundancy: self.compute_redundancy(branch_size),
            };
        }

        let raw_subtasks = branch_size / k;
        let force_decompose =
            estimated_complexity >= self.config.decompose_complexity_threshold;

        if raw_subtasks < self.config.min_subtasks as u64 && !force_decompose {
            return GranularityDecision::ExecuteAtomic {
                redundancy: self.compute_redundancy(branch_size),
            };
        }

        let subtask_count = raw_subtasks
            .max(self.config.min_subtasks as u64)
            .min(self.config.max_subtasks as u64) as u32;

        let agents_per_subtask = branch_size / subtask_count as u64;

        GranularityDecision::Decompose {
            subtask_count,
            agents_per_subtask,
        }
    }

    fn compute_redundancy(&self, branch_size: u64) -> u32 {
        self.config
            .redundancy_factor
            .min(branch_size.max(1) as u32)
    }

    /// Get the configuration.
    pub fn config(&self) -> &GranularityConfig {
        &self.config
    }
}

impl Default for GranularityAlgorithm {
    fn default() -> Self {
        Self::new(GranularityConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimal_subtask_count_large() {
        assert_eq!(GranularityEngine::optimal_subtask_count(1000, 10), 10);
    }

    #[test]
    fn test_optimal_subtask_count_medium() {
        assert_eq!(GranularityEngine::optimal_subtask_count(50, 10), 5);
    }

    #[test]
    fn test_optimal_subtask_count_small() {
        let count = GranularityEngine::optimal_subtask_count(5, 10);
        assert!(count >= 1);
    }

    #[test]
    fn test_strategy_massive() {
        assert_eq!(
            GranularityEngine::select_strategy(1000, 10, false),
            DecompositionStrategy::MassiveParallelism
        );
    }

    #[test]
    fn test_strategy_standard() {
        assert_eq!(
            GranularityEngine::select_strategy(50, 10, false),
            DecompositionStrategy::StandardDecomposition
        );
    }

    #[test]
    fn test_strategy_direct() {
        assert_eq!(
            GranularityEngine::select_strategy(8, 10, false),
            DecompositionStrategy::DirectAssignment
        );
    }

    #[test]
    fn test_strategy_redundant() {
        assert_eq!(
            GranularityEngine::select_strategy(50, 10, true),
            DecompositionStrategy::RedundantExecution
        );
    }

    #[test]
    fn test_redundant_count() {
        assert_eq!(GranularityEngine::redundant_execution_count(100, 10), 10);
        assert_eq!(GranularityEngine::redundant_execution_count(3, 10), 3);
        assert!(GranularityEngine::redundant_execution_count(1, 10) >= 1);
    }

    #[test]
    fn test_instance_algo_decompose() {
        let algo = GranularityAlgorithm::default();
        let decision = algo.compute(100, 0.5, 1, 5);
        assert!(matches!(
            decision,
            GranularityDecision::Decompose { subtask_count: 10, .. }
        ));
    }
}
