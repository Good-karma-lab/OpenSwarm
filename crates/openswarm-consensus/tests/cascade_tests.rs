//! Tests for the recursive decomposition cascade.
//!
//! Verifies (per §6.5 of the protocol spec):
//! - Winning plan subtasks are distributed to subordinates
//! - Recursion stops at atomic tasks or bottom tier
//! - Prime Orchestrator role assignment

use openswarm_consensus::cascade::{CascadeEngine, StopCondition};
use openswarm_protocol::types::{Plan, PlanSubtask};
use openswarm_protocol::identity::AgentId;

// ═══════════════════════════════════════════════════════════════
// § 6.5 Recursive Decomposition
// ═══════════════════════════════════════════════════════════════

#[test]
fn cascade_assigns_subtasks_to_agents() {
    let mut plan = Plan::new("task-1".into(), AgentId::new("proposer".into()), 1);
    for i in 0..3 {
        plan.subtasks.push(PlanSubtask {
            index: i,
            description: format!("Subtask {}", i),
            required_capabilities: vec![],
            estimated_complexity: 0.5,
        });
    }

    let agents = vec![
        AgentId::new("a1".into()),
        AgentId::new("a2".into()),
        AgentId::new("a3".into()),
    ];

    let assignments = CascadeEngine::assign_subtasks(&plan, &agents);
    assert_eq!(assignments.len(), 3, "Must produce one assignment per subtask");
    for (_i, (_agent, task)) in assignments.iter().enumerate() {
        assert_eq!(task.tier_level, 2, "Subtasks should be tier_level + 1");
        assert_eq!(task.parent_task_id.as_deref(), Some("task-1"));
    }
}

#[test]
fn cascade_stop_condition_atomic_task() {
    assert!(
        CascadeEngine::should_stop(StopCondition::AtomicTask),
        "Atomic tasks must stop cascade"
    );
}

#[test]
fn cascade_stop_condition_bottom_tier() {
    assert!(
        CascadeEngine::should_stop(StopCondition::BottomTier),
        "Bottom tier must stop cascade"
    );
}

#[test]
fn cascade_stop_condition_low_complexity() {
    assert!(
        CascadeEngine::should_stop(StopCondition::LowComplexity(0.05)),
        "Very low complexity should stop cascade"
    );
}

#[test]
fn cascade_stop_condition_normal_complexity() {
    assert!(
        !CascadeEngine::should_stop(StopCondition::LowComplexity(0.5)),
        "Normal complexity should not stop cascade"
    );
}

#[test]
fn cascade_prime_orchestrator_is_plan_proposer() {
    let plan = Plan::new(
        "task-1".into(),
        AgentId::new("did:swarm:winner".into()),
        1,
    );
    let orchestrator = CascadeEngine::prime_orchestrator(&plan);
    assert_eq!(orchestrator, &AgentId::new("did:swarm:winner".into()));
}

#[test]
fn cascade_subtask_count_matches_plan() {
    let mut plan = Plan::new("task-1".into(), AgentId::new("p".into()), 1);
    for i in 0..10 {
        plan.subtasks.push(PlanSubtask {
            index: i,
            description: format!("Sub {}", i),
            required_capabilities: vec![],
            estimated_complexity: 0.3,
        });
    }
    let agents: Vec<AgentId> = (0..10)
        .map(|i| AgentId::new(format!("agent-{}", i)))
        .collect();
    let assignments = CascadeEngine::assign_subtasks(&plan, &agents);
    assert_eq!(assignments.len(), 10);
}

#[test]
fn cascade_fewer_agents_than_subtasks() {
    let mut plan = Plan::new("task-1".into(), AgentId::new("p".into()), 1);
    for i in 0..10 {
        plan.subtasks.push(PlanSubtask {
            index: i,
            description: format!("Sub {}", i),
            required_capabilities: vec![],
            estimated_complexity: 0.3,
        });
    }
    // Only 5 agents for 10 subtasks
    let agents: Vec<AgentId> = (0..5)
        .map(|i| AgentId::new(format!("agent-{}", i)))
        .collect();
    let assignments = CascadeEngine::assign_subtasks(&plan, &agents);
    // Should assign available agents, possibly with some getting multiple tasks
    assert!(assignments.len() <= 10);
    assert!(assignments.len() >= 5, "At least one assignment per agent");
}
