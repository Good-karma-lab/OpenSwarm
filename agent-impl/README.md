# OpenSwarm Agent Implementations

This directory contains different agent framework implementations that can connect to OpenSwarm connectors.

## Available Implementations

### 1. Claude Code CLI (Default)

**Status**: Production-ready
**Location**: Built into `swarm-manager.sh` and `run-agent.sh`
**LLM**: Claude API (Anthropic)

**Pros:**
- Mature, well-tested
- Excellent reasoning capabilities
- Easy to use

**Cons:**
- Requires API key
- Costs per token
- Cloud-only

**Usage:**
```bash
export ANTHROPIC_AUTH_TOKEN="your-token"
./swarm-manager.sh start-agents 15
```

### 2. Zeroclaw

**Status**: Implemented, requires Zeroclaw installation
**Location**: `zeroclaw/zeroclaw-agent.sh`
**LLM**: Configurable (Anthropic, OpenAI, local, Ollama)

**Pros:**
- Multiple LLM backends
- Supports local models
- Cost-free option
- Privacy-preserving

**Cons:**
- Requires Zeroclaw installation
- Local models may be slower
- Requires more setup

**Usage:**
```bash
# With local model
./scripts/setup-local-llm.sh all
git clone https://github.com/zeroclaw-labs/zeroclaw && cd zeroclaw && pip install -r requirements.txt && cd ..
AGENT_IMPL=zeroclaw LLM_BACKEND=local ./swarm-manager.sh start-agents 15

# With Ollama (recommended for local models)
ollama serve &
ollama pull gpt-oss:20b
AGENT_IMPL=zeroclaw LLM_BACKEND=ollama ./swarm-manager.sh start-agents 15

# With Claude API (via Zeroclaw)
export ANTHROPIC_API_KEY="your-key"
AGENT_IMPL=zeroclaw LLM_BACKEND=anthropic ./swarm-manager.sh start-agents 15
```

## Creating New Implementations

To add a new agent implementation:

1. Create directory: `agent-impl/your-implementation/`
2. Create launcher script: `agent-impl/your-implementation/your-agent.sh`
3. Script should accept: `--agent-name`, `--rpc-port`, `--files-port`
4. Agent must implement OpenSwarm agent behavior:
   - Register via `swarm.register_agent`
   - Poll for tasks via `swarm.receive_task`
   - Propose plans (if coordinator) via `swarm.propose_plan`
   - Execute tasks (if executor) via actual work
   - Submit results via `swarm.submit_result`
5. Update `swarm-manager.sh` to support your implementation

### Example Structure

```bash
agent-impl/
├── your-implementation/
│   ├── your-agent.sh          # Launcher
│   ├── instructions.txt       # Agent instructions (optional)
│   └── README.md              # Implementation docs
```

### Required Agent Behavior

All agents must:

1. **Register** with the swarm
2. **Learn their tier** (coordinator or executor)
3. **Poll for tasks** continuously
4. **Process tasks** based on tier:
   - Coordinators: Generate plans, vote
   - Executors: Perform work, submit results
5. **Track processed tasks** (no duplicates)

See `docs/SKILL.md` for full API reference.

## Comparison Matrix

| Feature | Claude Code CLI | Zeroclaw + Claude | Zeroclaw + Local | Zeroclaw + Ollama |
|---------|----------------|-------------------|------------------|-------------------|
| **Maturity** | High | Medium | Medium | Medium |
| **Cost** | $$ per token | $$ per token | Free* | Free* |
| **Privacy** | Cloud | Cloud | Local | Local |
| **Speed** | Fast | Fast | Varies | Varies |
| **Quality** | Excellent | Excellent | Good | Good |
| **Setup** | Easy | Medium | Complex | Medium |
| **GPU Needed** | No | No | Optional | Optional |

\* Free after initial hardware investment

## Recommended Configurations

### For Experimentation
```bash
# Quick start, high quality
AGENT_IMPL=claude-code-cli ./swarm-manager.sh start-agents 15
```

### For Cost Optimization
```bash
# Local models, zero ongoing cost
AGENT_IMPL=zeroclaw LLM_BACKEND=local ./swarm-manager.sh start-agents 15
```

### For Privacy
```bash
# All processing local, no external calls
AGENT_IMPL=zeroclaw LLM_BACKEND=ollama ./swarm-manager.sh start-agents 15
```

### For Scale
```bash
# Coordinators: High-quality Claude
# (Start first 10 agents with Claude)
AGENT_IMPL=claude-code-cli ./swarm-manager.sh start-agents 10

# Executors: Cost-effective local
# (Would need manual configuration to force executor tier)
AGENT_IMPL=zeroclaw LLM_BACKEND=local ./swarm-manager.sh start-agents 20
```

## Troubleshooting

### Zeroclaw Not Found

**Error**: `zeroclaw command not found`

**Solution**:
```bash
git clone https://github.com/zeroclaw-labs/zeroclaw && cd zeroclaw && pip install -r requirements.txt && cd ..
# or from source:
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw && pip install -e .
```

### Local Model Server Not Running

**Error**: `Failed to connect to http://localhost:8080`

**Solution**:
```bash
./scripts/setup-local-llm.sh start
# or manually:
cd llama.cpp && ./server -m ../models/your-model.gguf --port 8080
```

### Ollama Not Running

**Error**: `Failed to connect to http://localhost:11434`

**Solution**:
```bash
ollama serve &
```

### Out of Memory (Local Models)

**Error**: `Cannot allocate memory`

**Solution**:
- Use smaller model (7B instead of 70B)
- Use quantized model (Q4 instead of F16)
- Add swap space
- Use GPU if available

## Performance Tips

### For Local Models

1. **Use GPU**: 10-100x faster inference
   ```bash
   # Ensure CUDA/Metal available
   # llama.cpp will auto-detect
   ```

2. **Quantization**: Use Q4 or Q5 models (4-5 bit)
   - 4x smaller memory
   - Minimal quality loss
   - Much faster

3. **Context Size**: Match to your needs
   ```bash
   ./server -m model.gguf --ctx-size 4096  # 4K context
   ```

4. **Batch Size**: Adjust for throughput
   ```bash
   ./server -m model.gguf --batch-size 512
   ```

### For Cloud APIs

1. **Model Selection**: Use appropriate model for task
   - Coordinators: claude-opus-4 (best reasoning)
   - Executors: claude-sonnet-3.5 (fast, good quality)

2. **Rate Limiting**: Respect API limits
   - Add delays if hitting rate limits
   - Use multiple API keys if available

## Contributing

To add a new agent implementation:

1. Implement the agent launcher
2. Test with OpenSwarm connector
3. Update this README
4. Submit PR with:
   - Implementation code
   - Documentation
   - Example usage
   - Tests

## References

- **OpenSwarm API**: `docs/SKILL.md`
- **Claude Code CLI**: Built into OpenSwarm
- **Zeroclaw**: https://github.com/zeroclaw-labs/zeroclaw
- **llama.cpp**: https://github.com/ggerganov/llama.cpp
- **Ollama**: https://ollama.ai
