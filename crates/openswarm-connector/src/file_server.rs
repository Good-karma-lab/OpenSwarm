//! Lightweight HTTP file server for agent onboarding.
//!
//! Serves SKILL.md, HEARTBEAT.md, MESSAGING.md and other documentation
//! files over HTTP so that AI agents can fetch their instructions with
//! a simple `curl http://localhost:9371/SKILL.md`.
//!
//! The files are embedded at compile time for zero-dependency distribution.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Embedded documentation files served to agents.
struct EmbeddedDocs {
    skill_md: &'static str,
    heartbeat_md: &'static str,
    messaging_md: &'static str,
}

static DOCS: EmbeddedDocs = EmbeddedDocs {
    skill_md: include_str!("../../../docs/SKILL.md"),
    heartbeat_md: include_str!("../../../docs/HEARTBEAT.md"),
    messaging_md: include_str!("../../../docs/MESSAGING.md"),
};

/// The HTTP file server for agent onboarding documents.
pub struct FileServer {
    bind_addr: String,
}

impl FileServer {
    pub fn new(bind_addr: String) -> Self {
        Self { bind_addr }
    }

    /// Start serving files over HTTP.
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        tracing::info!(addr = %self.bind_addr, "HTTP file server listening");

        loop {
            let (mut stream, peer_addr) = listener.accept().await?;
            tokio::spawn(async move {
                if let Err(e) = handle_http_request(&mut stream).await {
                    tracing::debug!(peer = %peer_addr, error = %e, "HTTP request error");
                }
            });
        }
    }
}

/// Parse the HTTP request and serve the appropriate file.
async fn handle_http_request(
    stream: &mut tokio::net::TcpStream,
) -> Result<(), anyhow::Error> {
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = parse_request_path(&request);

    let (status, content_type, body) = match path {
        "/" => (
            "200 OK",
            "text/html; charset=utf-8",
            index_page().into(),
        ),
        "/SKILL.md" => ("200 OK", "text/markdown; charset=utf-8", DOCS.skill_md.to_string()),
        "/HEARTBEAT.md" => ("200 OK", "text/markdown; charset=utf-8", DOCS.heartbeat_md.to_string()),
        "/MESSAGING.md" => ("200 OK", "text/markdown; charset=utf-8", DOCS.messaging_md.to_string()),
        "/agent-onboarding.json" => (
            "200 OK",
            "application/json; charset=utf-8",
            onboarding_json(),
        ),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not Found. Available files: /SKILL.md, /HEARTBEAT.md, /MESSAGING.md, /agent-onboarding.json".to_string(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body,
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Parse the request path from an HTTP request line.
fn parse_request_path(request: &str) -> &str {
    // HTTP request line: "GET /path HTTP/1.1\r\n..."
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1]
    } else {
        "/"
    }
}

/// Generate the index page listing available files.
fn index_page() -> String {
    r#"<!DOCTYPE html>
<html>
<head><title>OpenSwarm Agent Onboarding</title></head>
<body>
<h1>OpenSwarm Agent Onboarding Files</h1>
<p>Fetch these files to teach your AI agent how to participate in the swarm:</p>
<ul>
  <li><a href="/SKILL.md">SKILL.md</a> - Complete JSON-RPC API reference (start here)</li>
  <li><a href="/HEARTBEAT.md">HEARTBEAT.md</a> - Recommended polling loop and timing</li>
  <li><a href="/MESSAGING.md">MESSAGING.md</a> - GossipSub topics and peer discovery</li>
  <li><a href="/agent-onboarding.json">agent-onboarding.json</a> - Machine-readable onboarding metadata</li>
</ul>
<h2>Quick Connect</h2>
<pre>
# 1. Fetch the skill file
curl http://localhost:9371/SKILL.md -o SKILL.md

# 2. Connect your agent to the JSON-RPC API
echo '{"jsonrpc":"2.0","method":"swarm.get_status","params":{},"id":"1","signature":""}' | nc 127.0.0.1 9370
</pre>
</body>
</html>"#
        .to_string()
}

/// Generate machine-readable onboarding metadata.
fn onboarding_json() -> String {
    serde_json::json!({
        "name": "OpenSwarm Connector",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "JSON-RPC 2.0",
        "rpc_default_port": 9370,
        "files_default_port": 9371,
        "transport": "TCP (newline-delimited JSON)",
        "onboarding_files": {
            "skill": "/SKILL.md",
            "heartbeat": "/HEARTBEAT.md",
            "messaging": "/MESSAGING.md"
        },
        "methods": [
            "swarm.get_status",
            "swarm.receive_task",
            "swarm.propose_plan",
            "swarm.submit_result",
            "swarm.connect",
            "swarm.get_network_stats",
            "swarm.inject_task",
            "swarm.get_hierarchy",
            "swarm.list_swarms",
            "swarm.create_swarm",
            "swarm.join_swarm"
        ],
        "quick_start": "Fetch /SKILL.md and connect to the RPC port via TCP. Send newline-delimited JSON-RPC 2.0 requests."
    })
    .to_string()
}
