# OpenSwarm Quick Reference Card

## One-Line Starts

### Cloud AI (Claude Code CLI)
```bash
export ANTHROPIC_AUTH_TOKEN="sk-ant-..." && ./run-agent.sh -n "alice"
```

### Local AI (Ollama + gpt-oss:20b)
```bash
# After setup (see below for Zeroclaw installation)
AGENT_IMPL=zeroclaw LLM_BACKEND=ollama MODEL_NAME=gpt-oss:20b ./run-agent.sh -n "alice"
```

### Multi-Agent Swarm (15 agents)
```bash
./swarm-manager.sh start-agents 15
```

## Setup Commands

### Local LLM Setup (First Time)
```bash
./scripts/setup-local-llm.sh all           # Install Ollama + download gpt-oss:20b

# Install Zeroclaw from source (currently in development)
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -r requirements.txt && cd ..
```

### Check Status
```bash
./scripts/setup-local-llm.sh status        # Check if Ollama is running
ollama list                                # List downloaded models
```

## Configuration

### Via Environment Variables (Session)
```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
```

### Via Config File (Persistent)
Edit `openswarm.conf`:
```bash
AGENT_IMPL=zeroclaw
LLM_BACKEND=ollama
MODEL_NAME=gpt-oss:20b
```

## Common Tasks

### Start Single Agent
```bash
./run-agent.sh -n "alice"
```

### Start Multiple Agents
```bash
./swarm-manager.sh start-agents 15
```

### Inject Task
```bash
echo '{"jsonrpc":"2.0","method":"swarm.inject_task","params":{"description":"Research quantum computing"},"id":"1"}' | nc 127.0.0.1 9370
```

### Check Agent Status
```bash
echo '{"jsonrpc":"2.0","method":"swarm.get_status","params":{},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

### View Hierarchy
```bash
echo '{"jsonrpc":"2.0","method":"swarm.get_hierarchy","params":{},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

### Stop All Agents
```bash
./swarm-manager.sh stop
```

## LLM Backends

### Anthropic (Claude API)
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=anthropic
export MODEL_NAME=claude-opus-4
```

### OpenAI (GPT)
```bash
export OPENAI_API_KEY="sk-..."
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=openai
export MODEL_NAME=gpt-4
```

### Ollama (Local - Recommended)
```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
```

### llama.cpp (Local - Advanced)
```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=local
export LOCAL_MODEL_PATH=./models/gpt-oss-20b.gguf
```

## Ollama Models

### Pull Models
```bash
ollama pull gpt-oss:20b         # 20B params (recommended)
ollama pull llama3:8b           # 8B params (faster)
ollama pull llama3:70b          # 70B params (slower, better)
ollama pull mistral:7b          # 7B params (fast)
```

### List Models
```bash
ollama list
```

### Remove Model
```bash
ollama rm gpt-oss:20b
```

## Troubleshooting

### Port Conflicts
```bash
ps aux | grep openswarm-connector
kill <PID>
```

### Ollama Not Running
```bash
ollama serve &
```

### Model Not Found
```bash
ollama pull gpt-oss:20b
```

### Zeroclaw Not Found
```bash
# Install from source (currently in development)
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -r requirements.txt && cd ..
```

### Check Logs
```bash
tail -f /tmp/openswarm-agent-alice-connector.log
tail -f logs/agent-*.log
```

## Monitoring

### View Agent Logs (Real-time)
```bash
./swarm-manager.sh logs agent-0
```

### Check Swarm Status
```bash
./swarm-manager.sh status
```

### View All Tasks
```bash
echo '{"jsonrpc":"2.0","method":"swarm.list_tasks","params":{},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

### Get Task Details
```bash
echo '{"jsonrpc":"2.0","method":"swarm.get_task","params":{"task_id":"task-123"},"id":"1"}' | nc 127.0.0.1 9370 | jq
```

## Performance Tips

### Use GPU for Local Models
Ollama auto-detects GPU:
- **macOS**: Metal (M1/M2/M3)
- **Linux**: CUDA (NVIDIA) or ROCm (AMD)

### Choose Right Model Size
- **Fast**: llama3:8b (4GB RAM)
- **Balanced**: gpt-oss:20b (12GB RAM) ← Recommended
- **Best**: llama3:70b (40GB RAM)

### Adjust Context Size
```bash
# For Ollama, edit ~/.ollama/models/<model>/config.json
# Or use model variants: gpt-oss:20b-8k, gpt-oss:20b-32k
```

## Cost Comparison

| Backend | Setup | Per Task | 100 Tasks |
|---------|-------|----------|-----------|
| **Ollama** | $0-2000* | $0 | $0 |
| Claude API | $0 | $0.05 | $5 |
| GPT-4 API | $0 | $0.10 | $10 |

\* Hardware cost (one-time, optional GPU)

## File Locations

```
openswarm.conf                   # Configuration
run-agent.sh                     # Single agent launcher
swarm-manager.sh                 # Multi-agent manager
scripts/setup-local-llm.sh       # Local LLM setup
agent-impl/zeroclaw/             # Zeroclaw agent
logs/                            # Agent logs (from swarm-manager)
/tmp/openswarm-agent-*.log       # Connector logs (from run-agent)
```

## Documentation

- **Quick Start**: [QUICKSTART.md](QUICKSTART.md)
- **Full README**: [README.md](README.md)
- **run-agent.sh Guide**: [docs/RUN_AGENT.md](docs/RUN_AGENT.md)
- **API Reference**: [docs/SKILL.md](docs/SKILL.md)
- **Ollama Setup**: [PHASE_6_OLLAMA_SETUP.md](PHASE_6_OLLAMA_SETUP.md)
- **Agent Implementations**: [agent-impl/README.md](agent-impl/README.md)

## Getting Help

```bash
./run-agent.sh --help            # run-agent options
./swarm-manager.sh --help        # swarm-manager options
./scripts/setup-local-llm.sh     # setup-local-llm options
ollama --help                    # Ollama options
```

## Emergency Stop

```bash
# Stop all agents
./swarm-manager.sh stop

# Kill all processes
pkill -f openswarm-connector
pkill -f claude
pkill -f zeroclaw

# Stop Ollama
pkill -f ollama
```

---

**For detailed documentation, see [QUICKSTART.md](QUICKSTART.md)** 📚
