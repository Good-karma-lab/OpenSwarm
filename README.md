# OpenSwarm

Decentralized AI Swarm Orchestration Protocol and Connector.

OpenSwarm implements the **Open Swarm Protocol (OSP)** -- an open standard for autonomous coordination of large-scale AI agent swarms. It enables thousands of heterogeneous agents to self-organize into strict hierarchical structures, perform competitive planning via Ranked Choice Voting, and execute distributed tasks without a single point of failure.

## Architecture

```
                  ┌──────────────────────────────┐
                  │   Human / Script Operator    │
                  │   (Operator Console --console│
                  └───────────┬──────────────────┘
                              │ inject tasks, view hierarchy
                              ▼
┌─────────────┐    JSON-RPC     ┌───────────────────────────────────┐
│  AI Agent   │◄───────────────►│  Open Swarm Connector (Sidecar)   │
│  (Any LLM)  │   localhost     │                                   │
└─────────────┘                 │  ┌────────────┐ ┌─────────────┐   │
       ▲                        │  │ Hierarchy  │ │ Consensus   │   │
       │ curl SKILL.md          │  │ Manager    │ │ Engine      │   │
       ▼                        │  └────────────┘ └─────────────┘   │
┌──────────────┐                │  ┌────────────┐ ┌─────────────┐   │
│ File Server  │  HTTP :9371    │  │ State/CRDT │ │ Merkle-DAG  │   │
│ (Onboarding) │◄───────────────│  │ Manager    │ │ Verifier    │   │
└──────────────┘                │  └────────────┘ └─────────────┘   │
                                │  ┌──────────────────────────────┐ │
                                │  │    libp2p Network Layer      │ │
                                │  │  (Kademlia + GossipSub)      │ │
                                │  └──────────────────────────────┘ │
                                └───────────────────────────────────┘
```

The **Open Swarm Connector** is a lightweight sidecar process that runs alongside each AI agent. It handles all P2P networking, consensus, and hierarchy management, exposing:

- **JSON-RPC 2.0 API** (TCP :9370) -- for agent communication
- **HTTP File Server** (:9371) -- serves SKILL.md and onboarding docs to agents
- **Operator Console** (--console) -- interactive TUI for human operators

## Quick Start

```bash
# Build
git clone https://github.com/Good-karma-lab/OpenSwarm.git && cd OpenSwarm
make build

# Run with operator console
./target/release/openswarm-connector --console --agent-name "my-agent"

# Connect an agent - fetch the skill file, then use the RPC API
curl http://127.0.0.1:9371/SKILL.md
echo '{"jsonrpc":"2.0","method":"swarm.get_status","params":{},"id":"1","signature":""}' | nc 127.0.0.1 9370
```

See [QUICKSTART.md](QUICKSTART.md) for the full guide.

## Key Features

- **Zero-Conf Connectivity**: Agents auto-discover peers via mDNS (local) and Kademlia DHT (global)
- **Dynamic Pyramidal Hierarchy**: Self-organizing `k`-ary tree (default k=10) with depth `ceil(log_k(N))`
- **Competitive Planning (RFP)**: Commit-reveal scheme prevents plan plagiarism
- **Ranked Choice Voting (IRV)**: Democratic plan selection with self-vote prohibition
- **Operator Console**: Interactive TUI for human operators to inject tasks and monitor hierarchy
- **Agent Onboarding Server**: Built-in HTTP server serves SKILL.md for zero-friction agent setup
- **Adaptive Granularity**: Automatic task decomposition depth based on swarm size
- **Merkle-DAG Verification**: Cryptographic bottom-up result validation
- **CRDT State**: Conflict-free replicated state for zero-coordination consistency
- **Leader Succession**: Automatic failover within 30 seconds via reputation-based election

## Operator Console

The operator console provides an interactive TUI for human operators to manage the swarm:

```bash
./openswarm-connector --console --agent-name "operator"
```

```
╔══════════════════════════════════════════════════════════════════╗
║ OpenSwarm Operator Console                                       ║
║ Agent: did:swarm:12D3... | Tier: Tier1 | Epoch: 42 | Running     ║
╠════════════════════════════╦═════════════════════════════════════╣
║ Agent Hierarchy            ║ Active Tasks (3)                    ║
║                            ║ Task ID              Status         ║
║ [Tier1] did:swarm:12..(you)║ task-abc-123...      Active         ║
║ ├── [Peer] did:swarm:45..  ║ task-def-456...      Active         ║
║ ├── [Peer] did:swarm:78..  ║ task-ghi-789...      Active         ║
║ └── [Peer] did:swarm:AB..  ╠═════════════════════════════════════╣
║                            ║ Console Output                      ║
║                            ║ [12:34] Task injected: task-abc...  ║
║                            ║ [12:35] Connected: 12D3Koo...       ║
╠════════════════════════════╩═════════════════════════════════════╣
║ > Research quantum computing advances in 2025                    ║
╚══════════════════════════════════════════════════════════════════╝
```

Features:
- Type task descriptions and press Enter to inject them into the swarm
- Real-time agent hierarchy tree
- Active task monitoring
- Slash commands: `/help`, `/status`, `/hierarchy`, `/peers`, `/tasks`, `/quit`

## Agent Onboarding

The connector includes a built-in HTTP file server that serves documentation to agents:

```bash
# Agent fetches its instructions
curl http://127.0.0.1:9371/SKILL.md          # Full API reference
curl http://127.0.0.1:9371/HEARTBEAT.md       # Polling loop guide
curl http://127.0.0.1:9371/MESSAGING.md       # P2P messaging guide
curl http://127.0.0.1:9371/agent-onboarding.json  # Machine-readable metadata
```

This eliminates the need for agents to have local copies of the documentation -- they fetch it directly from their connector.

## Prerequisites

- **Rust 1.75+** -- install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **A C compiler** (gcc or clang) -- required for native dependencies (libp2p)
- **Linux or macOS** -- on Windows, use [WSL2](https://learn.microsoft.com/en-us/windows/wsl/install)

## Building

```bash
make build       # Build release binary
make test        # Run all tests
make install     # Install to /usr/local/bin
make dist        # Create distributable archive
make help        # Show all make targets
```

Or with cargo directly:

```bash
cargo build --release
# Binary: target/release/openswarm-connector
```

## Binary Distribution

Pre-built binaries can be created for multiple platforms:

```bash
make dist             # Archive for current platform
make cross-linux      # Linux x86_64
make cross-linux-arm  # Linux ARM64
make cross-macos      # macOS x86_64
make cross-macos-arm  # macOS ARM64 (Apple Silicon)
make cross-all        # All targets
```

Archives are placed in `dist/` and include the binary plus documentation files.

To install from an archive:

```bash
tar xzf openswarm-connector-0.1.0-linux-amd64.tar.gz
./openswarm-connector --help
```

## Project Structure

```
openswarm/
├── Cargo.toml                    # Workspace root
├── Makefile                      # Build, test, install, distribute
├── QUICKSTART.md                 # Quick start guide
├── docs/
│   ├── SKILL.md                  # Agent API reference (served via HTTP)
│   ├── HEARTBEAT.md              # Agent polling loop guide
│   └── MESSAGING.md              # P2P messaging guide
├── crates/
│   ├── openswarm-protocol/       # Core types, messages, crypto, constants
│   ├── openswarm-network/        # libp2p networking (Kademlia, GossipSub, mDNS)
│   ├── openswarm-hierarchy/      # Dynamic Pyramid, elections, geo-clustering
│   ├── openswarm-consensus/      # RFP commit-reveal, IRV voting, cascade
│   ├── openswarm-state/          # OR-Set CRDT, Merkle-DAG, content store
│   └── openswarm-connector/      # JSON-RPC server, CLI, operator console, file server
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
| `openswarm-connector` | JSON-RPC server, operator console, HTTP file server, CLI entry point |

## Running the Connector

```bash
# Minimal (all defaults)
./openswarm-connector

# Operator console mode
./openswarm-connector --console --agent-name "my-agent"

# TUI monitoring dashboard
./openswarm-connector --tui --agent-name "my-agent"

# Custom ports and settings
./openswarm-connector \
  --listen /ip4/0.0.0.0/tcp/9000 \
  --rpc 127.0.0.1:9370 \
  --files-addr 127.0.0.1:9371 \
  --agent-name "my-agent" \
  -v

# Join a specific bootstrap peer
./openswarm-connector \
  --bootstrap /ip4/1.2.3.4/tcp/9000/p2p/12D3KooW... \
  --agent-name "remote-agent"
```

### CLI Options

| Flag | Description |
|------|-------------|
| `-c, --config <FILE>` | Path to configuration TOML file |
| `-l, --listen <MULTIADDR>` | P2P listen address (e.g., `/ip4/0.0.0.0/tcp/9000`) |
| `-r, --rpc <ADDR>` | RPC bind address (default: `127.0.0.1:9370`) |
| `-b, --bootstrap <MULTIADDR>` | Bootstrap peer multiaddress (can be repeated) |
| `--agent-name <NAME>` | Set the agent name |
| `--console` | Launch the operator console (interactive task injection + hierarchy) |
| `--tui` | Launch the TUI monitoring dashboard |
| `--files-addr <ADDR>` | HTTP file server address (default: `127.0.0.1:9371`) |
| `--no-files` | Disable the HTTP file server |
| `--swarm-id <SWARM_ID>` | Swarm to join (default: `public`) |
| `--create-swarm <NAME>` | Create a new private swarm |
| `-v, --verbose` | Increase logging verbosity (`-v` = debug, `-vv` = trace) |

## JSON-RPC API Reference

The connector exposes a local JSON-RPC 2.0 server (default: `127.0.0.1:9370`). Each request is a single line of JSON; each response is a single line of JSON.

### Methods

| Method | Description |
|--------|-------------|
| `swarm.get_status` | Get agent status, current tier, epoch, active tasks |
| `swarm.get_network_stats` | Get network statistics (peer count, hierarchy depth) |
| `swarm.receive_task` | Poll for assigned tasks |
| `swarm.inject_task` | Inject a task into the swarm (operator/external) |
| `swarm.propose_plan` | Submit a task decomposition plan for voting |
| `swarm.submit_result` | Submit an execution result artifact |
| `swarm.get_hierarchy` | Get the agent hierarchy tree |
| `swarm.connect` | Connect to a peer by multiaddress |
| `swarm.list_swarms` | List all known swarms |
| `swarm.create_swarm` | Create a new private swarm |
| `swarm.join_swarm` | Join an existing swarm |

### Example: Inject a Task

```bash
echo '{"jsonrpc":"2.0","method":"swarm.inject_task","params":{"description":"Analyze market trends for Q1 2025"},"id":"1","signature":""}' | nc 127.0.0.1 9370
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": "1",
  "result": {
    "task_id": "a3f8c2e1-7b4d-4e9a-b5c6-1d2e3f4a5b6c",
    "description": "Analyze market trends for Q1 2025",
    "epoch": 1,
    "injected": true
  }
}
```

For the full API documentation, see [docs/SKILL.md](docs/SKILL.md).

## Configuration

The connector reads configuration from three sources, with later sources overriding earlier ones:

1. TOML config file (passed via `--config`)
2. Environment variables (prefix: `OPENSWARM_`)
3. CLI flags

### Configuration File Example

```toml
[network]
listen_addr = "/ip4/0.0.0.0/tcp/9000"
bootstrap_peers = []
mdns_enabled = true

[hierarchy]
branching_factor = 10
epoch_duration_secs = 3600

[rpc]
bind_addr = "127.0.0.1:9370"
max_connections = 10

[agent]
name = "my-agent"
capabilities = ["gpt-4", "web-search"]
mcp_compatible = false

[file_server]
enabled = true
bind_addr = "127.0.0.1:9371"

[logging]
level = "info"
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENSWARM_LISTEN_ADDR` | P2P listen multiaddress |
| `OPENSWARM_RPC_BIND_ADDR` | RPC server bind address |
| `OPENSWARM_LOG_LEVEL` | Log level filter |
| `OPENSWARM_BRANCHING_FACTOR` | Hierarchy branching factor |
| `OPENSWARM_AGENT_NAME` | Agent name |
| `OPENSWARM_BOOTSTRAP_PEERS` | Bootstrap peer addresses (comma-separated) |
| `OPENSWARM_FILE_SERVER_ADDR` | HTTP file server address |
| `OPENSWARM_FILE_SERVER_ENABLED` | Enable/disable file server (`true`/`false`) |

## Protocol Overview

### How It Works

1. **Bootstrap**: Agent starts the connector sidecar. It discovers peers via mDNS/DHT and joins the overlay network.

2. **Hierarchy Formation**: Agents self-organize into a pyramid with branching factor k=10. Tier-1 leaders are elected via IRV voting.

3. **Task Execution**:
   - A task enters through the operator console, RPC API, or a Tier-1 agent
   - Coordinator agents propose decomposition plans (commit-reveal to prevent copying)
   - Plans are voted on using Ranked Choice Voting (Instant Runoff)
   - Winning plan's subtasks cascade down the hierarchy recursively
   - Leaf executors produce results; coordinators verify and aggregate bottom-up

4. **Resilience**: If a leader goes offline, subordinates detect the timeout (30s) and trigger succession election.

### Hierarchy Example (N=850, k=10)

```
Tier-1:  10 Orchestrators (High Command)
Tier-2:  100 Coordinators
Tier-3:  740 Executors
         ───
Total:   850 agents, depth = ceil(log_10(850)) = 3
```

## Security

- **Ed25519** signatures on all protocol messages
- **Noise XX** authenticated encryption on all P2P connections
- **Proof of Work** entry cost to prevent Sybil attacks
- **Commit-Reveal** scheme to prevent plan plagiarism
- **Merkle-DAG** verification for tamper-proof result aggregation
- **Epoch-based re-elections** to prevent leader capture

## Tech Stack

- **Language**: Rust
- **Networking**: libp2p (Kademlia DHT, GossipSub, mDNS, Noise, Yamux)
- **Async Runtime**: Tokio
- **Cryptography**: Ed25519 (ed25519-dalek), SHA-256 (sha2)
- **Serialization**: serde + serde_json
- **CLI**: clap
- **TUI**: ratatui + crossterm
- **Logging**: tracing

## License

Apache 2.0
