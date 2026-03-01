function scrub(s) {
  return String(s || '').replace(/did:swarm:[A-Za-z0-9]+/g, m => '[' + m.slice(-6) + ']')
}

function healthLabel(a) {
  if (!a.connected) return { text: 'DOWN', cls: 'badge-coral' }
  if (!a.loop_active) return { text: 'DEGRADED', cls: 'badge-amber' }
  return { text: 'HEALTHY', cls: 'badge-teal' }
}

function ReputationBar({ score, tasksCompleted }) {
  const s = score || 0
  // Color: teal once inject-eligible (≥ 5 tasks = 0.5 rep), amber if progressing, coral if new
  const color = s >= 0.5 ? 'var(--teal)' : s > 0 ? '#ffaa00' : 'var(--coral)'
  // Bar width: scale relative to inject threshold (5 tasks = 100% width base, grows beyond)
  const barPct = Math.min(100, (s / 0.5) * 100)
  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
        <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Reputation</span>
        <span style={{ fontSize: 13, fontWeight: 700, color, fontFamily: 'var(--font-mono)' }}>
          {s.toFixed(2)} <span style={{ fontWeight: 400, fontSize: 11 }}>({tasksCompleted ?? 0} tasks)</span>
        </span>
      </div>
      <div style={{ background: 'var(--border)', borderRadius: 4, height: 6, overflow: 'hidden' }}>
        <div style={{ width: `${barPct}%`, background: color, height: '100%', borderRadius: 4, transition: 'width 0.4s ease' }} />
      </div>
    </div>
  )
}

export default function AgentDetailPanel({ agent, tasks, onTaskClick }) {
  if (!agent) return null
  const health = healthLabel(agent)
  const taskList = (tasks?.tasks || []).filter(t =>
    t.assigned_to === agent.agent_id || t.assigned_to_name === agent.name
  )

  return (
    <div>
      {/* Header meta */}
      <div className="detail-meta" style={{ marginBottom: 16 }}>
        <span>ID: <strong>{scrub(agent.agent_id)}</strong></span>
        <span>Name: <strong>{scrub(agent.name)}</strong></span>
        <span>Tier: <strong>{agent.tier}</strong></span>
        <span className={`badge ${health.cls}`}>{health.text}</span>
        {agent.can_inject_tasks
          ? <span className="badge badge-teal" title="Can submit tasks to the swarm">✓ Can inject tasks</span>
          : <span className="badge badge-amber" title="Must complete at least 5 tasks first">⚠ No inject rights</span>
        }
      </div>

      {/* Reputation */}
      <div className="detail-section">
        <div className="detail-section-title">Reputation</div>
        <ReputationBar score={agent.reputation_score} tasksCompleted={agent.tasks_processed_count} />
        <div style={{ fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.6 }}>
          Score = tasks_completed × 0.1 — grows without limit.<br />
          Inject rights unlock at score ≥ 0.50 (5 completed tasks).
        </div>
      </div>

      {/* Stats */}
      <div className="detail-section">
        <div className="detail-section-title">Activity</div>
        <table className="data-table">
          <thead>
            <tr>
              <th>Metric</th>
              <th>Value</th>
            </tr>
          </thead>
          <tbody>
            <tr><td>Connected</td><td>{agent.connected ? 'yes' : 'no'}</td></tr>
            <tr><td>Loop active</td><td>{agent.loop_active ? 'yes' : 'no'}</td></tr>
            <tr><td>Tasks assigned</td><td>{agent.tasks_assigned_count ?? 0}</td></tr>
            <tr><td>Tasks processed</td><td>{agent.tasks_processed_count ?? 0}</td></tr>
            <tr><td>Plans proposed</td><td>{agent.plans_proposed_count ?? 0}</td></tr>
            <tr><td>Plans revealed</td><td>{agent.plans_revealed_count ?? 0}</td></tr>
            <tr><td>Votes cast</td><td>{agent.votes_cast_count ?? 0}</td></tr>
            <tr><td>Last poll (s)</td><td>{agent.last_task_poll_secs ?? '—'}</td></tr>
            <tr><td>Last result (s)</td><td>{agent.last_result_secs ?? '—'}</td></tr>
          </tbody>
        </table>
      </div>

      {/* Assigned tasks */}
      {taskList.length > 0 && (
        <div className="detail-section">
          <div className="detail-section-title">Assigned Tasks</div>
          {taskList.map(t => (
            <div
              key={t.task_id}
              onClick={() => onTaskClick && onTaskClick(t)}
              style={{
                padding: '6px 10px',
                background: 'var(--surface-2)',
                border: '1px solid var(--border)',
                borderRadius: 5,
                marginBottom: 4,
                cursor: 'pointer',
                fontFamily: 'var(--font-mono)',
                fontSize: 11,
              }}
            >
              <span style={{ color: 'var(--teal)' }}>{(t.task_id || '').slice(0, 12)}…</span>
              {' '}
              <span style={{ color: 'var(--text-muted)' }}>{t.status}</span>
              {' '}
              <span>{t.description?.slice(0, 60) || ''}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
