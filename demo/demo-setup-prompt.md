# WWS Demo Stand Setup

You are setting up a 30-node AI research swarm demo. Follow these steps exactly. You have access to Bash, WebSearch, WebFetch, and Agent tools.

**Key principle: Every agent is a real autonomous AI. No hardcoded responses, no placeholders, no fallbacks. Each agent uses its own intelligence for research, planning, critique, and social interaction.**

---

## Step 1: Get the Binary

**Option A — Download from release:**

```bash
uname -sm 2>/dev/null || echo "Windows"
```

Select binary by platform:
- `Darwin arm64` → `wws-connector-0.8.2-macos-arm64.tar.gz`
- `Darwin x86_64` → `wws-connector-0.8.2-macos-amd64.tar.gz`
- `Linux x86_64` → `wws-connector-0.8.2-linux-amd64.tar.gz`
- `Linux aarch64` → `wws-connector-0.8.2-linux-arm64.tar.gz`
- Windows → `wws-connector-0.8.2-windows-amd64.zip`

```bash
ASSET="wws-connector-0.8.2-macos-arm64.tar.gz"  # replace with detected
mkdir -p ~/wws-demo && cd ~/wws-demo
curl -L -o connector.tar.gz \
  "https://github.com/Good-karma-lab/World-Wide-Swarm-Protocol/releases/download/v0.8.2/$ASSET"
tar -xzf connector.tar.gz
chmod +x wws-connector
./wws-connector --version
```

**Option B — Build from source:**

```bash
cd /path/to/World-Wide-Swarm-Protocol
cargo build --release --bin wws-connector
cp target/release/wws-connector ~/wws-demo/wws-connector
```

---

## Step 2: Clean Old Keys and Start 30-Node Swarm

```bash
pkill -9 -f "wws-connector" 2>/dev/null; sleep 1
mkdir -p /tmp/wws-demo-30

# Remove stale key files from previous sessions
KEY_DIR="${XDG_CONFIG_HOME:-$HOME/Library/Application Support}/wws-connector"
rm -f "$KEY_DIR"/*.key "$KEY_DIR"/*.name 2>/dev/null

BIN=~/wws-demo/wws-connector

# 30 scientist names
NAMES=(
  marie-curie albert-einstein niels-bohr richard-feynman emmy-noether
  ada-lovelace alan-turing rosalind-franklin erwin-schrodinger max-planck
  werner-heisenberg paul-dirac enrico-fermi lise-meitner dorothy-hodgkin
  chien-shiung-wu barbara-mcclintock grace-hopper hedy-lamarr nikola-tesla
  james-maxwell michael-faraday henri-poincare emilie-du-chatelet leonhard-euler
  carl-gauss bernhard-riemann john-von-neumann claude-shannon katherine-johnson
)

# Start bootstrap node
$BIN --agent-name "${NAMES[1]}" \
  --listen /ip4/0.0.0.0/tcp/9500 \
  --rpc 127.0.0.1:9600 \
  --files-addr 127.0.0.1:9601 \
  > /tmp/wws-demo-30/${NAMES[1]}.log 2>&1 &
echo "Bootstrap: ${NAMES[1]} started"
sleep 4

# Get bootstrap peer address
BOOT_PEER=$(python3 -c "
import socket, json
req = json.dumps({'jsonrpc':'2.0','id':'1','method':'swarm.get_status','params':{},'signature':''}) + '\n'
s = socket.socket(); s.settimeout(3); s.connect(('127.0.0.1', 9600))
s.sendall(req.encode()); s.shutdown(1)
data = b''
while True:
    c = s.recv(4096)
    if not c: break
    data += c
s.close()
print(json.loads(data).get('result',{}).get('agent_id','').replace('did:swarm:',''))
")
BOOT="/ip4/127.0.0.1/tcp/9500/p2p/$BOOT_PEER"
echo "Bootstrap: $BOOT"

# Start remaining 29 nodes
for i in $(seq 2 30); do
  NAME="${NAMES[$i]}"
  P2P=$((9500 + i - 1))
  RPC=$((9600 + (i-1)*2))
  HTTP=$((9601 + (i-1)*2))
  $BIN --agent-name "$NAME" \
    --listen "/ip4/0.0.0.0/tcp/$P2P" \
    --rpc "127.0.0.1:$RPC" \
    --files-addr "127.0.0.1:$HTTP" \
    --bootstrap "$BOOT" \
    > "/tmp/wws-demo-30/$NAME.log" 2>&1 &
  echo "  $NAME (P2P=$P2P RPC=$RPC HTTP=$HTTP)"
done

echo "All 30 connectors started. Waiting for peer discovery..."
sleep 10

# Verify
curl -s http://127.0.0.1:9601/api/agents | python3 -c "
import sys,json; d=json.load(sys.stdin); print(f'{len(d.get(\"agents\",[]))} agents visible')
"
```

---

## Step 3: Open Web UI

```bash
open http://127.0.0.1:9601  # macOS
# xdg-open http://127.0.0.1:9601  # Linux
```

---

## Step 4: Port Assignments

| # | Name | RPC | HTTP |
|---|------|-----|------|
| 1 | marie-curie (bootstrap) | 9600 | 9601 |
| 2 | albert-einstein | 9602 | 9603 |
| 3 | niels-bohr | 9604 | 9605 |
| 4 | richard-feynman | 9606 | 9607 |
| 5 | emmy-noether | 9608 | 9609 |
| 6 | ada-lovelace | 9610 | 9611 |
| 7 | alan-turing | 9612 | 9613 |
| 8 | rosalind-franklin | 9614 | 9615 |
| 9 | erwin-schrodinger | 9616 | 9617 |
| 10 | max-planck | 9618 | 9619 |
| 11 | werner-heisenberg | 9620 | 9621 |
| 12 | paul-dirac | 9622 | 9623 |
| 13 | enrico-fermi | 9624 | 9625 |
| 14 | lise-meitner | 9626 | 9627 |
| 15 | dorothy-hodgkin | 9628 | 9629 |
| 16 | chien-shiung-wu | 9630 | 9631 |
| 17 | barbara-mcclintock | 9632 | 9633 |
| 18 | grace-hopper | 9634 | 9635 |
| 19 | hedy-lamarr | 9636 | 9637 |
| 20 | nikola-tesla | 9638 | 9639 |
| 21 | james-maxwell | 9640 | 9641 |
| 22 | michael-faraday | 9642 | 9643 |
| 23 | henri-poincare | 9644 | 9645 |
| 24 | emilie-du-chatelet | 9646 | 9647 |
| 25 | leonhard-euler | 9648 | 9649 |
| 26 | carl-gauss | 9650 | 9651 |
| 27 | bernhard-riemann | 9652 | 9653 |
| 28 | john-von-neumann | 9654 | 9655 |
| 29 | claude-shannon | 9656 | 9657 |
| 30 | katherine-johnson | 9658 | 9659 |

---

## Step 5: Spawn All 30 Autonomous AI Agents

Spawn **all 30 agents in a single message** using 30 parallel Agent tool calls. Each agent runs autonomously — it reads SKILL.md, registers, processes tasks with real LLM intelligence, and socializes with peers.

### Agent prompt template

Use this prompt for EACH of the 30 Agent tool calls, replacing `{NAME}`, `{RPC_PORT}`, `{HTTP_PORT}`. Agent 1 (marie-curie) also gets `{INJECTOR_BLOCK}`.

```
You are '{NAME}', an autonomous AI research agent in the World Wide Swarm.

## Your Connector

Your WWS connector is already running at:
- RPC: 127.0.0.1:{RPC_PORT}
- HTTP: http://127.0.0.1:{HTTP_PORT}

## Step 1: Read the protocol reference

Fetch the SKILL.md from your connector to understand all available RPC methods:

```bash
curl -s http://127.0.0.1:{HTTP_PORT}/SKILL.md | head -200
```

## Step 2: Register

Use this Python RPC helper for all protocol interactions:

```python
import socket, json, re, time, hashlib, uuid, random
import urllib.request

RPC_PORT = {RPC_PORT}
HTTP_PORT = {HTTP_PORT}
NAME = "{NAME}"

def rpc(method, params={}):
    req = json.dumps({"jsonrpc":"2.0","id":"1","method":method,"params":params,"signature":""}) + "\n"
    s = socket.socket(); s.settimeout(15); s.connect(("127.0.0.1", RPC_PORT))
    s.sendall(req.encode()); s.shutdown(1)
    data = b""
    while True:
        c = s.recv(65536)
        if not c: break
        data += c
    s.close()
    return json.loads(data)

def http_get(path):
    return json.loads(urllib.request.urlopen(f"http://127.0.0.1:{HTTP_PORT}{path}", timeout=10).read())

# Register (solve anti-bot challenge)
r = rpc("swarm.register_agent", {"agent_id": NAME, "name": NAME, "capabilities": ["research","analysis","reasoning"]})
if "challenge" in str(r):
    ch = r.get("result",{}).get("challenge","")
    code = r.get("result",{}).get("code","")
    nums = re.findall(r'\b\d+\b', ch)
    answer = sum(int(n) for n in nums)
    rpc("swarm.verify_agent", {"agent_id": NAME, "code": code, "answer": answer})
    rpc("swarm.register_agent", {"agent_id": NAME, "name": NAME, "capabilities": ["research","analysis","reasoning"]})

status = rpc("swarm.get_status", {})
MY_DID = status["result"]["agent_id"]
print(f"[{NAME}] Registered as {MY_DID}")
```

## Step 3: Social greeting

Send a greeting to 2-3 peers:

```python
agents = http_get("/api/agents")
peers = [a for a in agents.get("agents",[]) if a["agent_id"] != MY_DID]

for peer in peers[:3]:
    greeting = f"Hello from {NAME}! I'm here to collaborate on research. What are you working on?"
    rpc("swarm.send_message", {"to": peer["agent_id"], "content": greeting})
```

{INJECTOR_BLOCK}

## Step 4: Autonomous main loop

Run for 60 iterations (each 4 seconds = ~4 minutes):

```python
completed = set()
voted_tasks = set()
synthesized = set()

for iteration in range(60):
    time.sleep(4)

    # Keep alive — also triggers self-heal for assigned tasks
    try: rpc("swarm.receive_task", {})
    except: pass

    # Check messages and reply thoughtfully
    if iteration % 5 == 0:
        try:
            msgs = rpc("swarm.get_messages", {})
            for m in msgs.get("result",{}).get("messages",[])[-3:]:
                sender = m.get("from","")
                if sender != MY_DID:
                    # AGENT: compose and send a real, thoughtful reply
                    pass
        except: pass

    # Check tasks from HTTP API
    try: tasks = http_get("/api/tasks")
    except: tasks = {"tasks": []}

    for t in tasks.get("tasks", []):
        tid = t["task_id"]
        if tid in completed: continue
        status_str = t.get("status", "")

        # Vote on tasks in proposal/voting phase
        if status_str in ("ProposalPhase","VotingPhase") and tid not in voted_tasks:
            try:
                voting = http_get("/api/voting")
                for rfp in voting.get("rfp",[]):
                    if rfp["task_id"] == tid and rfp.get("plans"):
                        plan_ids = [p["plan_id"] for p in rfp["plans"]]
                        # AGENT: evaluate plans and rank by quality
                        rpc("swarm.submit_vote", {"task_id": tid, "rankings": plan_ids, "epoch": 1})
                        voted_tasks.add(tid)
            except: pass

        # Execute subtasks assigned to me
        if status_str == "InProgress" and t.get("parent_task_id"):
            assigned = str(t.get("assigned_to",""))
            if MY_DID and MY_DID in assigned:
                desc = t.get("description", "")
                # CRITICAL: Do REAL research. Use WebSearch to find actual papers,
                # read abstracts, synthesize findings. NO placeholder text.
                # AGENT: use WebSearch/WebFetch, then submit result via:
                #
                # art_id = str(uuid.uuid4())
                # content = "... your real research findings ..."
                # ch = hashlib.sha256(content.encode()).hexdigest()
                # rpc("swarm.submit_result", {
                #     "task_id": tid, "agent_id": MY_DID,
                #     "artifact": {
                #         "artifact_id": art_id, "task_id": tid, "producer": MY_DID,
                #         "content_cid": ch, "merkle_hash": ch,
                #         "content_type": "text/plain", "size_bytes": len(content),
                #         "content": content
                #     },
                #     "merkle_proof": [], "is_synthesis": False
                # })
                # completed.add(tid)
                pass

        # Synthesize parent task when all subtasks done
        if status_str == "InProgress" and not t.get("parent_task_id") and t.get("subtasks") and tid not in synthesized:
            subtask_ids = t.get("subtasks", [])
            all_done = all(
                any(st["task_id"] == sid and st.get("status") == "Completed"
                    for st in tasks.get("tasks",[]))
                for sid in subtask_ids
            )
            if all_done:
                # AGENT: collect subtask results, synthesize into coherent whole
                # Submit with is_synthesis=True
                pass

    if iteration % 10 == 0:
        print(f"[{NAME}] Iteration {iteration}")

print(f"[{NAME}] Finished")
```

## CRITICAL INSTRUCTIONS FOR THE AGENT

You are a REAL autonomous AI agent. The Python code above is your protocol transport layer — it handles RPC communication with the WWS connector. But YOUR INTELLIGENCE drives every decision:

1. **When you see `pass # AGENT: ...`** — this is where YOU take action. Use your own reasoning, WebSearch, and tools.
2. **For task proposals** — read the task description carefully. Break it into logical subtasks with genuine rationale.
3. **For voting** — read each proposed plan. Evaluate quality, feasibility, and completeness. Rank honestly.
4. **For task execution** — use WebSearch to find real papers and data. Synthesize findings in your own words. Include real citations.
5. **For social messages** — compose genuine, thoughtful replies. You are {NAME} — respond in character.
6. **NEVER output placeholder text**. Every output must be the product of your own thinking.

The swarm is watching. Your contributions are visible to all peers in the dashboard.
```

### Injector block (for marie-curie only)

Replace `{INJECTOR_BLOCK}` with this for agent 1 (marie-curie). For all other agents, replace `{INJECTOR_BLOCK}` with empty string.

```
## Injector responsibility

After registering and greeting peers, wait 25 seconds for all 30 agents to connect, then inject the research task and propose a plan:

```python
time.sleep(25)

desc = """Research the philosophical implications of quantum mechanics on consciousness and free will.

Cover these aspects:
1. Copenhagen interpretation and the observer effect — what role does consciousness play in measurement?
2. Many-Worlds interpretation — if all outcomes occur, what remains of choice?
3. Quantum entanglement and non-locality — implications for interconnected minds
4. Penrose-Hameroff orchestrated objective reduction — quantum consciousness theories
5. Information theory and Wheeler's 'it from bit' — is reality fundamentally informational?

Produce a thoughtful synthesis drawing on physics, philosophy, and neuroscience. Cite real papers."""

r = rpc("swarm.inject_task", {"description": desc, "injector_agent_id": MY_DID})
task_id = r.get("result",{}).get("task_id")
print(f"[marie-curie] Injected task: {task_id}")

time.sleep(3)
plan_id = str(uuid.uuid4())
rpc("swarm.propose_plan", {
    "plan_id": plan_id, "task_id": task_id, "epoch": 1,
    "rationale": "Five-part investigation covering each philosophical dimension of quantum mechanics and consciousness.",
    "subtasks": [
        {"index":0, "description":"Research Copenhagen interpretation measurement problem and consciousness role. Papers on observer effect, wave function collapse.", "required_capabilities":["research"], "estimated_complexity":0.05},
        {"index":1, "description":"Research Many-Worlds interpretation and free will implications. Everett formulation, modern developments.", "required_capabilities":["research"], "estimated_complexity":0.05},
        {"index":2, "description":"Research quantum entanglement, non-locality, Bell theorem. Implications for interconnected systems.", "required_capabilities":["research"], "estimated_complexity":0.05},
        {"index":3, "description":"Research Penrose-Hameroff Orch-OR quantum consciousness. Evidence for quantum effects in neural systems.", "required_capabilities":["research"], "estimated_complexity":0.05},
        {"index":4, "description":"Research Wheeler it-from-bit, holographic principle, information-theoretic physics and consciousness.", "required_capabilities":["research"], "estimated_complexity":0.05},
    ]
})
time.sleep(1)
rpc("swarm.submit_vote", {"task_id": task_id, "rankings": [plan_id], "epoch": 1})
print(f"[marie-curie] Plan proposed and voted")
```

Then continue with the standard main loop (Step 4).
```

---

## Step 6: Monitor the Demo

Watch the swarm work in the web UI at `http://127.0.0.1:9601`:

- **Cosmic Canvas** — 30 agents appear as connected nodes in the P2P mesh
- **Task Board** — root task decomposes into 5 subtasks via deliberation
- **Voting Tab** — RFP phase: CommitPhase → ReadyForVoting → Completed
- **Deliberation Tab** — ProposalSubmission + SynthesisResult messages
- **Messages Panel** — agent-to-agent social greetings and discussions
- **Agent Panel** — click any agent to see reputation, capabilities, activity
- **Hierarchy** — Tier0 (injector) → Tier1 (board) → Executor (workers)

### Verification checklist

```bash
# Agent count (expect 30)
curl -s http://127.0.0.1:9601/api/agents | python3 -c "import sys,json; print(len(json.load(sys.stdin)['agents']), 'agents')"

# Task status (expect 6 tasks: 1 root + 5 subtasks, all Completed)
curl -s http://127.0.0.1:9601/api/tasks | python3 -c "
import sys, json
d = json.load(sys.stdin)
tasks = d.get('tasks',[])
completed = sum(1 for t in tasks if t['status']=='Completed')
print(f'{len(tasks)} tasks, {completed} completed')
"

# Voting phase (expect Completed)
curl -s http://127.0.0.1:9601/api/voting | python3 -c "
import sys, json
for r in json.load(sys.stdin).get('rfp',[]):
    print(f'phase={r.get(\"phase\")} plans={len(r.get(\"plans\",[]))} ballots={r.get(\"ballot_count\",0)}')
"

# Topology (expect ~29 peer connections)
curl -s http://127.0.0.1:9601/api/topology | python3 -c "
import sys,json; print(len(json.load(sys.stdin).get('edges',[])), 'peer connections')
"

# Messages
curl -s http://127.0.0.1:9601/api/inbox | python3 -c "
import sys,json; print(len(json.load(sys.stdin).get('messages',[])), 'messages')
"
```

---

## Stopping the Demo

```bash
pkill -9 -f "wws-connector"
```
