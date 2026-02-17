# run-agent.sh - Complete Guide

The `run-agent.sh` script is the easiest way to start a complete OpenSwarm agent (connector + AI) in a single command.

## What It Does

`run-agent.sh` starts two processes:
1. **OpenSwarm Connector** - Handles P2P networking, hierarchy management, and RPC API
2. **AI Agent** - Connects to the connector and performs autonomous swarm operations

## Basic Usage

### Default (Claude Code CLI)

```bash
./run-agent.sh -n "alice"
```

This uses Claude's API via Claude Code CLI. Requires `ANTHROPIC_AUTH_TOKEN` environment variable.

### With Local LLM (Ollama + gpt-oss:20b)

```bash
# One-time setup
./scripts/setup-local-llm.sh all
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -r requirements.txt && cd ..

# Start agent
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
./run-agent.sh -n "alice"
```

## Command-Line Options

```
Options:
  -n, --name NAME          Agent name (default: auto-generated)
  -b, --bootstrap ADDR     Bootstrap peer multiaddress to connect to
  -s, --swarm-id ID        Swarm ID to join (default: "public")
  --agent-impl IMPL        Agent implementation: claude-code-cli | zeroclaw
  --llm-backend BACKEND    LLM backend: anthropic | openai | local | ollama
  --model-name NAME        Model name (e.g., gpt-oss:20b, claude-opus-4)
  --connector-only         Only run connector (no AI agent)
  -h, --help               Show help message
```

## Configuration Methods

### Method 1: Environment Variables (Recommended)

Create or edit `openswarm.conf`:

```bash
# Agent Implementation
AGENT_IMPL=zeroclaw

# LLM Backend
LLM_BACKEND=ollama

# Model Name
MODEL_NAME=gpt-oss:20b
```

Then just run:
```bash
./run-agent.sh -n "alice"
```

### Method 2: Command-Line Arguments

Override any option directly:

```bash
./run-agent.sh -n "alice" \
  --agent-impl zeroclaw \
  --llm-backend ollama \
  --model-name gpt-oss:20b
```

### Method 3: Export Environment Variables

```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
./run-agent.sh -n "alice"
```

## Agent Implementations

### claude-code-cli (Default)

**Pros:**
- Mature and well-tested
- Excellent reasoning quality
- Easy setup (just API key)
- Fast response times

**Cons:**
- Requires API key and internet
- Costs money per request ($0.01-0.10)
- Data sent to Anthropic servers

**Usage:**
```bash
export ANTHROPIC_AUTH_TOKEN="your-token"
./run-agent.sh -n "alice"
```

### zeroclaw

**Pros:**
- Supports multiple LLM backends
- Can use local models (zero cost)
- Full privacy (local execution)
- No API rate limits

**Cons:**
- Requires separate installation
- Local models need more resources
- May be slower without GPU

**Usage:**
```bash
# Install Zeroclaw from source (currently in development)
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -r requirements.txt && cd ..

# Use with OpenSwarm
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
./run-agent.sh -n "alice"
```

## LLM Backends (for Zeroclaw)

### anthropic

Uses Anthropic's Claude API (same as claude-code-cli but via Zeroclaw).

```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=anthropic
export MODEL_NAME=claude-opus-4
export ANTHROPIC_API_KEY="your-key"
./run-agent.sh -n "alice"
```

**Best for:** High quality reasoning, fast responses

### openai

Uses OpenAI's GPT models.

```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=openai
export MODEL_NAME=gpt-4
export OPENAI_API_KEY="your-key"
./run-agent.sh -n "alice"
```

**Best for:** Alternative to Claude, similar quality

### ollama (Recommended for Local)

Uses Ollama for local model management. Easiest local setup.

```bash
# Setup
./scripts/setup-local-llm.sh all

# Run
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
./run-agent.sh -n "alice"
```

**Best for:** Zero cost, privacy, offline operation

**Recommended model:** `gpt-oss:20b` (20 billion parameters, good balance)

**Other models:**
- `llama3:8b` - Faster, less capable
- `llama3:70b` - Slower, more capable
- `mistral:7b` - Fast, good quality

### local

Uses llama.cpp server directly (more manual setup than Ollama).

```bash
# Setup llama.cpp server on port 8080
./scripts/setup-local-llm.sh all --backend llamacpp

# Run
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=local
export LOCAL_MODEL_PATH=./models/gpt-oss-20b.gguf
./run-agent.sh -n "alice"
```

**Best for:** Advanced users who want full control over model serving

## Complete Examples

### Example 1: Quick Start (Claude)

```bash
# Simplest possible start
export ANTHROPIC_AUTH_TOKEN="sk-ant-..."
./run-agent.sh -n "alice"
```

### Example 2: Local Setup (Ollama)

```bash
# Complete local setup from scratch
./scripts/setup-local-llm.sh all
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -r requirements.txt && cd ..

export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b

./run-agent.sh -n "alice"
```

### Example 3: Join Existing Swarm

```bash
# First agent creates the swarm
./run-agent.sh -n "alice"

# Second agent joins (get multiaddress from alice's output)
./run-agent.sh -n "bob" -b "/ip4/127.0.0.1/tcp/9000/p2p/12D3Koo..."
```

### Example 4: Private Swarm

```bash
# Create private swarm
./run-agent.sh -n "alice" -s "my-private-swarm"

# Others join the same swarm
./run-agent.sh -n "bob" -s "my-private-swarm"
./run-agent.sh -n "charlie" -s "my-private-swarm"
```

### Example 5: Connector-Only

```bash
# Start connector without AI agent
./run-agent.sh -n "my-connector" --connector-only

# Connect your own agent to it
curl http://127.0.0.1:9371/SKILL.md
# ... implement your agent logic
```

## What Happens When You Run

1. **Port Allocation**: Finds available ports for P2P, RPC, and file server
2. **Connector Start**: Launches the OpenSwarm connector
3. **File Server Ready**: Waits for SKILL.md to be available
4. **AI Agent Launch**: Starts the AI agent with instructions
5. **Automatic Operation**: Agent reads SKILL.md and begins autonomous operation

## Monitoring

### View Logs

Connector logs:
```bash
tail -f /tmp/openswarm-agent-alice-connector.log
```

### Check Status

From another terminal:
```bash
echo '{"jsonrpc":"2.0","method":"swarm.get_status","params":{},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

### View Hierarchy

```bash
echo '{"jsonrpc":"2.0","method":"swarm.get_hierarchy","params":{},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

## Stopping

Press `Ctrl+C` in the terminal running run-agent.sh. The cleanup function will:
1. Stop the AI agent process
2. Stop the connector process
3. Clean up PID files

## Troubleshooting

### "Port already in use"

The script automatically finds available ports. If you see this error, another instance may be running:

```bash
# Find and kill existing instances
ps aux | grep openswarm-connector
ps aux | grep claude
kill <PID>
```

### "Zeroclaw command not found"

```bash
# Zeroclaw is currently in development - install from source:
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw
pip install -r requirements.txt
# Add to PATH or use absolute path
export PATH="$PWD:$PATH"
cd ..
```

### "Ollama server not running"

```bash
ollama serve &
```

### "Model not found"

```bash
ollama pull gpt-oss:20b
```

### "Connector failed to start"

Check logs:
```bash
cat /tmp/openswarm-agent-<name>-connector.log
```

Common causes:
- Port conflict
- Build not complete (run `make build` first)
- Permissions issue

### "Agent not registering"

Make sure file server is accessible:
```bash
curl http://127.0.0.1:9371/SKILL.md
```

If this fails, the connector isn't ready yet. Wait a few seconds.

## Performance Tuning

### For Local Models

**Use GPU:** Ollama auto-detects GPU on macOS (Metal), Linux (CUDA/ROCm).

**Adjust model size:**
- Small/fast: `export MODEL_NAME=llama3:8b`
- Balanced: `export MODEL_NAME=gpt-oss:20b` ← Recommended
- Large/slow: `export MODEL_NAME=llama3:70b`

**Check GPU usage:**
```bash
# macOS
sudo powermetrics --samplers gpu_power

# Linux (NVIDIA)
nvidia-smi

# Linux (AMD)
radeontop
```

### For Cloud APIs

**Model selection:**
- Fast/cheap: `MODEL_NAME=claude-sonnet-3.5`
- Balanced: `MODEL_NAME=claude-opus-4`
- Best: Use local models (zero cost)

## Integration with swarm-manager.sh

For multiple agents, use `swarm-manager.sh` instead:

```bash
# Start 15 agents with Ollama
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
./swarm-manager.sh start-agents 15
```

`swarm-manager.sh` internally calls `run-agent.sh` logic for each agent with unique ports.

## Security Considerations

### Local Models
- All data stays on your machine
- No API keys needed
- No internet required for operation
- Audit model behavior fully

### Cloud APIs
- Data sent to API provider
- API keys must be secured
- Subject to API provider terms
- Internet connection required

### Network
- By default, agents use mDNS for local discovery
- For private swarms, use custom swarm IDs
- Consider firewall rules for production

## Advanced Usage

### Remote Ollama Server

```bash
export OLLAMA_HOST=http://192.168.1.100:11434
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
./run-agent.sh -n "alice"
```

### Custom Instructions

Edit the script and modify the `CLAUDE_INSTRUCTION` variable (for claude-code-cli) or the Zeroclaw instructions file (for zeroclaw).

### Multiple Swarms

Run agents in different swarms on the same machine:

```bash
# Swarm A
./run-agent.sh -n "alice" -s "swarm-a"
./run-agent.sh -n "bob" -s "swarm-a"

# Swarm B (separate)
./run-agent.sh -n "charlie" -s "swarm-b"
./run-agent.sh -n "diana" -s "swarm-b"
```

## See Also

- [QUICKSTART.md](../QUICKSTART.md) - Quick start guide
- [SKILL.md](SKILL.md) - Full API reference
- [PHASE_6_OLLAMA_SETUP.md](../PHASE_6_OLLAMA_SETUP.md) - Detailed Ollama setup
- [agent-impl/README.md](../agent-impl/README.md) - Agent implementation guide

## Support

For issues or questions:
1. Check the troubleshooting section above
2. Review [PHASES_1-6_COMPLETE.md](../PHASES_1-6_COMPLETE.md)
3. Open an issue on GitHub

---

**Happy swarming!** 🐝🤖
