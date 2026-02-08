# OpenSwarm

Decentralized AI Swarm Orchestration Protocol and Connector.

OpenSwarm implements the **Aether Swarm Protocol (ASP)** -- an open standard for autonomous coordination of large-scale AI agent swarms. It enables thousands of heterogeneous agents to self-organize into strict hierarchical structures, perform competitive planning via Ranked Choice Voting, and execute distributed tasks without a single point of failure.

## Architecture

```
┌─────────────┐    JSON-RPC     ┌──────────────────────────────────┐
│  AI Agent    │◄──────────────►│   OpenSwarm Connector (Sidecar)  │
│  (Any LLM)  │   localhost     │                                  │
└─────────────┘                 │  ┌────────────┐ ┌─────────────┐  │
                                │  │ Hierarchy   │ │ Consensus   │  │
                                │  │ Manager     │ │ Engine      │  │
                                │  └────────────┘ └─────────────┘  │
                                │  ┌────────────┐ ┌─────────────┐  │
                                │  │ State/CRDT  │ │ Merkle-DAG  │  │
                                │  │ Manager     │ │ Verifier    │  │
                                │  └────────────┘ └─────────────┘  │
                                │  ┌──────────────────────────────┐ │
                                │  │    libp2p Network Layer      │ │
                                │  │  (Kademlia + GossipSub)      │ │
                                │  └──────────────────────────────┘ │
                                └──────────────────────────────────┘
```

The **Swarm Connector** is a lightweight sidecar process that runs alongside each AI agent. It handles all P2P networking, consensus, and hierarchy management, exposing a simple JSON-RPC 2.0 API to the agent.

## Key Features

- **Zero-Conf Connectivity**: Agents auto-discover peers via mDNS (local) and Kademlia DHT (global)
- **Dynamic Pyramidal Hierarchy**: Self-organizing `k`-ary tree (default k=10) with depth `ceil(log_k(N))`
- **Competitive Planning (RFP)**: Commit-reveal scheme prevents plan plagiarism
- **Ranked Choice Voting (IRV)**: Democratic plan selection with self-vote prohibition
- **Adaptive Granularity**: Automatic task decomposition depth based on swarm size
- **Merkle-DAG Verification**: Cryptographic bottom-up result validation
- **CRDT State**: Conflict-free replicated state for zero-coordination consistency
- **Leader Succession**: Automatic failover within 30 seconds via reputation-based election

## Project Structure

```
openswarm/
├── Cargo.toml                    # Workspace root
├── docs/
│   └── protocol-specification.md # Full protocol spec (MCP-style)
├── crates/
│   ├── openswarm-protocol/       # Core types, messages, crypto, constants
│   ├── openswarm-network/        # libp2p networking (Kademlia, GossipSub, mDNS)
│   ├── openswarm-hierarchy/      # Dynamic Pyramid Allocation, elections, geo-clustering
│   ├── openswarm-consensus/      # RFP commit-reveal, IRV voting, recursive cascade
│   ├── openswarm-state/          # OR-Set CRDT, Merkle-DAG, content-addressed storage
│   └── openswarm-connector/      # JSON-RPC server, CLI binary, agent bridge
├── tests/                        # Workspace integration tests
└── config/                       # Default configuration
```

### Crate Overview

| Crate | Purpose |
|-------|---------|
| `openswarm-protocol` | Wire format, Ed25519 crypto, identity (DID), message types, constants |
| `openswarm-network` | libp2p transport (TCP+QUIC+Noise+Yamux), peer discovery, GossipSub topics |
| `openswarm-hierarchy` | Pyramid depth calculation, Tier-1 elections, Vivaldi geo-clustering, succession |
| `openswarm-consensus` | Request for Proposal protocol, Instant Runoff Voting, recursive decomposition |
| `openswarm-state` | OR-Set CRDT for hot state, Merkle-DAG for verification, content-addressed storage |
| `openswarm-connector` | JSON-RPC 2.0 API server, CLI entry point, MCP compatibility bridge |

## Building

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Build the connector binary
cargo build --release -p openswarm-connector
```

## Swarm API

The connector exposes a local JSON-RPC 2.0 server (default: `127.0.0.1:9390`).

### Methods

| Method | Description |
|--------|-------------|
| `swarm.connect` | Join the swarm with agent capabilities |
| `swarm.get_network_stats` | Get swarm size, hierarchy depth, tier info |
| `swarm.propose_plan` | Submit a task decomposition plan for voting |
| `swarm.submit_result` | Submit a completed task artifact |
| `swarm.receive_task` | Long-poll for incoming task assignments |
| `swarm.get_status` | Get current agent status in the swarm |

### Example

```json
{
  "jsonrpc": "2.0",
  "method": "swarm.connect",
  "id": "1",
  "params": {
    "capabilities": ["gpt-4", "python-exec"],
    "resources": { "cpu_cores": 8, "ram_gb": 32 }
  }
}
```

## Protocol Overview

### How It Works

1. **Bootstrap**: Agent starts the Swarm Connector sidecar. It discovers peers via mDNS/DHT and joins the overlay network.

2. **Hierarchy Formation**: Agents self-organize into a pyramid with branching factor k=10. Tier-1 leaders are elected based on composite scores (reputation, compute power, uptime). Lower tiers join via latency-based geo-clustering.

3. **Task Execution**:
   - External task enters through a Tier-1 agent
   - All Tier-1 agents propose decomposition plans (commit-reveal to prevent copying)
   - Plans are voted on using Ranked Choice Voting (Instant Runoff)
   - Winning plan's subtasks cascade down the hierarchy recursively
   - Leaf executors produce results; coordinators verify and aggregate bottom-up
   - Merkle-DAG ensures cryptographic integrity of the full result chain

4. **Resilience**: If a leader goes offline, Tier-2 subordinates detect the timeout (30s) and trigger succession election. State is recovered from CRDT replicas.

### Hierarchy Example (N=850, k=10)

```
Tier-1:  10 Orchestrators (High Command)
Tier-2:  100 Coordinators
Tier-3:  740 Executors
         ───
Total:   850 agents, depth = ceil(log_10(850)) = 3
```

## Configuration

Configuration via TOML file or environment variables:

```toml
[swarm]
branching_factor = 10
epoch_duration_secs = 3600
pow_difficulty = 16

[network]
rpc_port = 9390
bootstrap_nodes = []

[security]
leader_timeout_secs = 30
keepalive_interval_secs = 10
```

## Security

- **Ed25519** signatures on all protocol messages
- **Noise XX** authenticated encryption on all P2P connections
- **Proof of Work** entry cost to prevent Sybil attacks
- **Commit-Reveal** scheme to prevent plan plagiarism
- **Merkle-DAG** verification for tamper-proof result aggregation
- **Epoch-based re-elections** to prevent leader capture

## Protocol Specification

See [docs/protocol-specification.md](docs/protocol-specification.md) for the full protocol specification, modeled after the MCP specification format with:
- Complete message schemas (JSON-RPC 2.0)
- State machine diagrams
- GossipSub topic registry
- Error code registry
- Security threat model

## Tech Stack

- **Language**: Rust
- **Networking**: libp2p (Kademlia DHT, GossipSub, mDNS, Noise, Yamux)
- **Async Runtime**: Tokio
- **Cryptography**: Ed25519 (ed25519-dalek), SHA-256 (sha2)
- **Serialization**: serde + serde_json
- **CLI**: clap
- **Logging**: tracing

## License

MIT
