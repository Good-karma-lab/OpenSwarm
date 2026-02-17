# Zeroclaw Installation Guide

## Overview

Zeroclaw is currently in active development and must be installed from source. This guide provides detailed installation instructions.

## Prerequisites

- **Python 3.8+**
- **Git**
- **pip** or **pip3**

## Quick Install

```bash
# Clone the repository
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw

# Install dependencies
pip install -r requirements.txt

# Make zeroclaw available in PATH (optional)
export PATH="$PWD:$PATH"

# Return to OpenSwarm directory
cd ..
```

## Detailed Installation Steps

### Step 1: Clone Repository

```bash
git clone https://github.com/zeroclaw-labs/zeroclaw
```

This downloads the Zeroclaw source code from GitHub.

### Step 2: Install Dependencies

```bash
cd zeroclaw
pip install -r requirements.txt
```

**If you encounter permission errors:**
```bash
pip install --user -r requirements.txt
```

**If pip is not found:**
```bash
pip3 install -r requirements.txt
```

### Step 3: Verify Installation

Check if zeroclaw command is available:

```bash
# If installed to PATH
zeroclaw --version

# Or use absolute path
python3 zeroclaw --version
```

### Step 4: Make Globally Available (Optional)

**Option A: Add to PATH (temporary, for current session)**
```bash
export PATH="$PWD:$PATH"
```

**Option B: Add to PATH (permanent)**

Add to your `~/.bashrc` or `~/.zshrc`:
```bash
export PATH="/path/to/zeroclaw:$PATH"
```

**Option C: Create symbolic link**
```bash
sudo ln -s /path/to/zeroclaw/zeroclaw /usr/local/bin/zeroclaw
```

**Option D: Use absolute path in OpenSwarm**

You can modify `agent-impl/zeroclaw/zeroclaw-agent.sh` to use absolute path:
```bash
exec /path/to/zeroclaw/zeroclaw \
    $LLM_CONFIG \
    --instructions "$INSTRUCTIONS_FILE" \
    --agent-name "$AGENT_NAME" \
    --autonomous \
    --verbose
```

## Using with OpenSwarm

After installation, configure OpenSwarm to use Zeroclaw:

### Method 1: Environment Variables

```bash
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
./run-agent.sh -n "alice"
```

### Method 2: Configuration File

Edit `openswarm.conf`:
```bash
AGENT_IMPL=zeroclaw
LLM_BACKEND=ollama
MODEL_NAME=gpt-oss:20b
```

Then run:
```bash
./run-agent.sh -n "alice"
```

### Method 3: Command-Line Arguments

```bash
./run-agent.sh -n "alice" \
  --agent-impl zeroclaw \
  --llm-backend ollama \
  --model-name gpt-oss:20b
```

## Troubleshooting

### "zeroclaw: command not found"

**Cause:** Zeroclaw is not in your PATH.

**Solutions:**
1. Add to PATH: `export PATH="/path/to/zeroclaw:$PATH"`
2. Use absolute path in zeroclaw-agent.sh
3. Create symbolic link (see Step 4 above)

### "No module named 'X'"

**Cause:** Missing Python dependencies.

**Solution:**
```bash
cd zeroclaw
pip install -r requirements.txt
```

### "Permission denied"

**Cause:** Trying to install system-wide without permissions.

**Solutions:**
1. Use `--user` flag: `pip install --user -r requirements.txt`
2. Use virtual environment (see below)
3. Use sudo (not recommended): `sudo pip install -r requirements.txt`

### "git: command not found"

**Cause:** Git is not installed.

**Solutions:**

**macOS:**
```bash
xcode-select --install
# or
brew install git
```

**Ubuntu/Debian:**
```bash
sudo apt update && sudo apt install git
```

**CentOS/RHEL:**
```bash
sudo yum install git
```

## Using Virtual Environment (Recommended)

To avoid conflicts with system Python packages:

```bash
# Create virtual environment
python3 -m venv zeroclaw-env

# Activate it
source zeroclaw-env/bin/activate  # Linux/macOS
# or
zeroclaw-env\Scripts\activate  # Windows

# Install Zeroclaw
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw
pip install -r requirements.txt

# Use with OpenSwarm (while venv is activated)
cd /path/to/OpenSwarm
export AGENT_IMPL=zeroclaw
./run-agent.sh -n "alice"

# Deactivate when done
deactivate
```

## Updating Zeroclaw

To get the latest version:

```bash
cd zeroclaw
git pull origin main
pip install -r requirements.txt
cd ..
```

## Uninstalling

```bash
# Remove the zeroclaw directory
rm -rf zeroclaw

# Remove from PATH (if added permanently)
# Edit ~/.bashrc or ~/.zshrc and remove the PATH line

# Remove symbolic link (if created)
sudo rm /usr/local/bin/zeroclaw
```

## Alternative: Wait for PyPI Release

In the future, Zeroclaw may be available on PyPI:

```bash
# This will work once Zeroclaw is published
pip install zeroclaw
```

Until then, source installation is required.

## Verifying Installation

Test that Zeroclaw works with OpenSwarm:

```bash
# Setup Ollama
./scripts/setup-local-llm.sh all

# Test with single agent
export AGENT_IMPL=zeroclaw
export LLM_BACKEND=ollama
export MODEL_NAME=gpt-oss:20b
./run-agent.sh -n "test-agent"

# You should see:
# - Connector starts successfully
# - Zeroclaw agent launches
# - Agent reads SKILL.md
# - Agent registers with swarm
# - Agent begins polling for tasks
```

## Development Setup

If you want to contribute to Zeroclaw:

```bash
# Clone with development flag
git clone https://github.com/zeroclaw-labs/zeroclaw
cd zeroclaw

# Install in editable mode
pip install -e .

# Install development dependencies
pip install -r requirements-dev.txt  # if available

# Run tests
pytest  # if tests are available

# Make changes and test immediately
# Changes to zeroclaw source are reflected without reinstalling
```

## Integration Status

✅ **Supported Backends:**
- Anthropic (Claude API)
- OpenAI (GPT API)
- Ollama (Local - recommended)
- llama.cpp (Local - advanced)

✅ **Works with:**
- `./run-agent.sh` (single agent)
- `./swarm-manager.sh` (multi-agent)
- All OpenSwarm RPC methods
- Autonomous agent behavior

⚠️ **Current Limitations:**
- Must be installed from source (not on PyPI yet)
- May have breaking changes during development
- Documentation may lag behind implementation

## Support

For Zeroclaw-specific issues:
- **Repository:** https://github.com/zeroclaw-labs/zeroclaw
- **Issues:** https://github.com/zeroclaw-labs/zeroclaw/issues

For OpenSwarm integration issues:
- Check [docs/RUN_AGENT.md](docs/RUN_AGENT.md)
- Check [QUICK_REFERENCE.md](QUICK_REFERENCE.md)
- Open issue on OpenSwarm repository

## Quick Reference

| Task | Command |
|------|---------|
| **Install** | `git clone https://github.com/zeroclaw-labs/zeroclaw && cd zeroclaw && pip install -r requirements.txt` |
| **Update** | `cd zeroclaw && git pull && pip install -r requirements.txt` |
| **Test** | `export AGENT_IMPL=zeroclaw && ./run-agent.sh -n test` |
| **Uninstall** | `rm -rf zeroclaw` |

---

**Once installed, Zeroclaw provides a powerful, flexible AI agent framework for OpenSwarm!** 🚀
