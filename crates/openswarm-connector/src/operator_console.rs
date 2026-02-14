//! Human-Operator Console for the OpenSwarm Connector.
//!
//! An interactive TUI that allows a human operator (or script piping stdin)
//! to inject tasks into the swarm, view the agent hierarchy tree, monitor
//! active tasks, and observe the event log.
//!
//! Launch with `openswarm-connector --console`.

use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame, Terminal,
};
use tokio::sync::RwLock;

use crate::connector::{ConnectorState, ConnectorStatus};
use crate::tui::{LogCategory, LogEntry};
use openswarm_protocol::{Task, Tier};

/// A node in the hierarchy tree for display.
#[derive(Debug, Clone)]
pub struct HierarchyNode {
    pub agent_id: String,
    pub display_name: String,
    pub tier: String,
    pub is_self: bool,
    pub children: Vec<HierarchyNode>,
    pub task_count: usize,
}

/// Snapshot of operator console state for rendering.
#[allow(dead_code)]
struct ConsoleSnapshot {
    agent_id: String,
    tier: String,
    epoch: u64,
    status: String,
    status_color: Color,
    peer_count: usize,
    swarm_size: u64,
    active_tasks: Vec<String>,
    hierarchy: Vec<HierarchyNode>,
    event_log: Vec<LogEntry>,
    current_swarm_name: String,
}

/// The operator console TUI state.
struct OperatorConsole {
    state: Arc<RwLock<ConnectorState>>,
    /// Current text in the input field.
    input: String,
    /// Cursor position within the input field.
    cursor_pos: usize,
    /// Command history for up/down arrow navigation.
    history: Vec<String>,
    /// Current position in history (-1 = current input).
    history_pos: Option<usize>,
    /// Scroll offset for the event log panel (reserved for future use).
    #[allow(dead_code)]
    log_scroll: u16,
    /// Scroll offset for the hierarchy panel.
    hierarchy_scroll: u16,
    /// Messages displayed in the console output area.
    console_messages: Vec<(chrono::DateTime<chrono::Utc>, String, Color)>,
}

impl OperatorConsole {
    fn new(state: Arc<RwLock<ConnectorState>>) -> Self {
        let mut console_messages = Vec::new();
        console_messages.push((
            chrono::Utc::now(),
            "OpenSwarm Operator Console ready. Type a task description and press Enter to inject it.".to_string(),
            Color::Cyan,
        ));
        console_messages.push((
            chrono::Utc::now(),
            "Commands: /help, /status, /hierarchy, /peers, /quit".to_string(),
            Color::DarkGray,
        ));

        Self {
            state,
            input: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_pos: None,
            log_scroll: 0,
            hierarchy_scroll: 0,
            console_messages,
        }
    }

    /// Take a snapshot of connector state for rendering.
    async fn snapshot(&self) -> ConsoleSnapshot {
        let state = self.state.read().await;

        let current_swarm_name = state
            .known_swarms
            .get(state.current_swarm_id.as_str())
            .map(|r| r.name.clone())
            .unwrap_or_else(|| state.current_swarm_id.as_str().to_string());

        // Build hierarchy from known agents.
        let hierarchy = build_hierarchy_tree(&state);

        ConsoleSnapshot {
            agent_id: state.agent_id.to_string(),
            tier: format_tier(&state.my_tier),
            epoch: state.epoch_manager.current_epoch(),
            status: format_status(&state.status),
            status_color: status_color(&state.status),
            peer_count: state.agent_set.len(),
            swarm_size: state.network_stats.total_agents,
            active_tasks: state.task_set.elements(),
            hierarchy,
            event_log: state.event_log.clone(),
            current_swarm_name,
        }
    }

    /// Process a command or task input from the operator.
    async fn process_input(&mut self) {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return;
        }

        // Save to history.
        self.history.push(input.clone());
        self.history_pos = None;

        if input.starts_with('/') {
            self.process_command(&input).await;
        } else {
            // Treat as a task description to inject.
            self.inject_task(&input).await;
        }

        self.input.clear();
        self.cursor_pos = 0;
    }

    /// Process a slash command.
    async fn process_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let _args = parts.get(1).copied().unwrap_or("");

        match command {
            "/help" => {
                self.add_message(
                    "Available commands:",
                    Color::Cyan,
                );
                self.add_message(
                    "  <text>       - Inject a task with the given description",
                    Color::White,
                );
                self.add_message(
                    "  /status      - Show detailed connector status",
                    Color::White,
                );
                self.add_message(
                    "  /hierarchy   - Show agent hierarchy tree",
                    Color::White,
                );
                self.add_message(
                    "  /peers       - List connected peers",
                    Color::White,
                );
                self.add_message(
                    "  /tasks       - List active tasks",
                    Color::White,
                );
                self.add_message(
                    "  /help        - Show this help message",
                    Color::White,
                );
                self.add_message(
                    "  /quit        - Exit the operator console",
                    Color::White,
                );
            }
            "/status" => {
                let (agent_id, tier, epoch, status, peers, tasks, content) = {
                    let state = self.state.read().await;
                    (
                        state.agent_id.to_string(),
                        format!("{:?}", state.my_tier),
                        state.epoch_manager.current_epoch(),
                        format!("{:?}", state.status),
                        state.agent_set.len(),
                        state.task_set.len(),
                        state.content_store.item_count(),
                    )
                };
                self.add_message(
                    &format!("Agent ID: {}", agent_id),
                    Color::Green,
                );
                self.add_message(
                    &format!("Tier: {} | Epoch: {} | Status: {}", tier, epoch, status),
                    Color::Green,
                );
                self.add_message(
                    &format!("Peers: {} | Tasks: {} | Content: {}", peers, tasks, content),
                    Color::Green,
                );
            }
            "/peers" => {
                let peers = {
                    let state = self.state.read().await;
                    state.agent_set.elements()
                };
                if peers.is_empty() {
                    self.add_message("No connected peers.", Color::Yellow);
                } else {
                    self.add_message(
                        &format!("Connected peers ({}):", peers.len()),
                        Color::Cyan,
                    );
                    for peer in &peers {
                        self.add_message(&format!("  {}", peer), Color::White);
                    }
                }
            }
            "/tasks" => {
                let tasks = {
                    let state = self.state.read().await;
                    state.task_set.elements()
                };
                if tasks.is_empty() {
                    self.add_message("No active tasks.", Color::Yellow);
                } else {
                    self.add_message(
                        &format!("Active tasks ({}):", tasks.len()),
                        Color::Cyan,
                    );
                    for task in &tasks {
                        self.add_message(&format!("  {}", task), Color::White);
                    }
                }
            }
            "/hierarchy" => {
                let hierarchy = {
                    let state = self.state.read().await;
                    build_hierarchy_tree(&state)
                };
                if hierarchy.is_empty() {
                    self.add_message("No hierarchy data available yet.", Color::Yellow);
                } else {
                    self.add_message("Agent Hierarchy:", Color::Cyan);
                    for node in &hierarchy {
                        print_hierarchy_node(node, "", true, &mut self.console_messages);
                    }
                }
            }
            "/quit" | "/exit" | "/q" => {
                // Handled in the event loop.
            }
            _ => {
                self.add_message(
                    &format!("Unknown command: {}. Type /help for available commands.", command),
                    Color::Red,
                );
            }
        }
    }

    /// Inject a task into the swarm.
    async fn inject_task(&mut self, description: &str) {
        let mut state = self.state.write().await;
        let epoch = state.epoch_manager.current_epoch();
        let task = Task::new(description.to_string(), 1, epoch);
        let task_id = task.task_id.clone();

        // Add task to the local task set.
        state.task_set.add(task_id.clone());

        // Log the injection.
        state.push_log(
            LogCategory::Task,
            format!("Operator injected task: {} ({})", task_id, description),
        );

        drop(state);

        self.add_message(
            &format!("Task injected: {}", task_id),
            Color::Green,
        );
        self.add_message(
            &format!("  Description: {}", description),
            Color::White,
        );
    }

    fn add_message(&mut self, msg: &str, color: Color) {
        self.console_messages.push((
            chrono::Utc::now(),
            msg.to_string(),
            color,
        ));
        // Cap at 500 messages.
        if self.console_messages.len() > 500 {
            self.console_messages.remove(0);
        }
    }

    /// Render the full operator console layout.
    fn render(&self, frame: &mut Frame, snapshot: &ConsoleSnapshot) {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Status bar
                Constraint::Min(8),    // Main area (hierarchy + tasks + log)
                Constraint::Length(5), // Input area
            ])
            .split(frame.area());

        self.render_status_bar(frame, outer[0], snapshot);
        self.render_main_area(frame, outer[1], snapshot);
        self.render_input(frame, outer[2]);
    }

    /// Render the top status bar.
    fn render_status_bar(&self, frame: &mut Frame, area: Rect, snap: &ConsoleSnapshot) {
        let block = Block::default()
            .title(" OpenSwarm Operator Console ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let agent_short = if snap.agent_id.len() > 24 {
            format!("{}...", &snap.agent_id[..24])
        } else {
            snap.agent_id.clone()
        };

        let status_line = Line::from(vec![
            Span::styled("  Agent: ", Style::default().fg(Color::Gray)),
            Span::styled(&agent_short, Style::default().fg(Color::White)),
            Span::styled("  |  Tier: ", Style::default().fg(Color::Gray)),
            Span::styled(&snap.tier, Style::default().fg(Color::Cyan)),
            Span::styled("  |  Epoch: ", Style::default().fg(Color::Gray)),
            Span::styled(snap.epoch.to_string(), Style::default().fg(Color::Magenta)),
            Span::styled("  |  Status: ", Style::default().fg(Color::Gray)),
            Span::styled(&snap.status, Style::default().fg(snap.status_color)),
            Span::styled("  |  Peers: ", Style::default().fg(Color::Gray)),
            Span::styled(snap.peer_count.to_string(), Style::default().fg(Color::Green)),
            Span::styled("  |  Swarm: ", Style::default().fg(Color::Gray)),
            Span::styled(&snap.current_swarm_name, Style::default().fg(Color::LightCyan)),
        ]);

        let paragraph = Paragraph::new(status_line).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render the main area with hierarchy, tasks, and event log.
    fn render_main_area(&self, frame: &mut Frame, area: Rect, snap: &ConsoleSnapshot) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Hierarchy
                Constraint::Percentage(60), // Tasks + Log
            ])
            .split(area);

        self.render_hierarchy(frame, columns[0], snap);

        let right_column = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Tasks
                Constraint::Min(4),    // Console output / Event log
            ])
            .split(columns[1]);

        self.render_tasks(frame, right_column[0], snap);
        self.render_console_output(frame, right_column[1]);
    }

    /// Render the agent hierarchy tree panel.
    fn render_hierarchy(&self, frame: &mut Frame, area: Rect, snap: &ConsoleSnapshot) {
        let block = Block::default()
            .title(" Agent Hierarchy ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        if snap.hierarchy.is_empty() {
            let text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Waiting for hierarchy data...",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Hierarchy forms as agents join",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "  and elections are held.",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let paragraph = Paragraph::new(text).block(block);
            frame.render_widget(paragraph, area);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        for node in &snap.hierarchy {
            render_hierarchy_lines(node, "", true, &mut lines);
        }

        // Apply scroll offset.
        let visible_height = area.height.saturating_sub(2) as usize;
        let scroll = self.hierarchy_scroll as usize;
        let visible: Vec<Line> = lines
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .collect();

        let paragraph = Paragraph::new(visible).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render the active tasks panel.
    fn render_tasks(&self, frame: &mut Frame, area: Rect, snap: &ConsoleSnapshot) {
        let block = Block::default()
            .title(format!(" Active Tasks ({}) ", snap.active_tasks.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        if snap.active_tasks.is_empty() {
            let text = Paragraph::new(Line::from(Span::styled(
                "  No active tasks. Type a task description below to inject one.",
                Style::default().fg(Color::DarkGray),
            )))
            .block(block);
            frame.render_widget(text, area);
            return;
        }

        let rows: Vec<Row> = snap
            .active_tasks
            .iter()
            .map(|task_id| {
                let short_id = if task_id.len() > 20 {
                    format!("{}...", &task_id[..20])
                } else {
                    task_id.clone()
                };
                Row::new(vec![
                    ratatui::widgets::Cell::from(Span::styled(
                        format!("  {}", short_id),
                        Style::default().fg(Color::White),
                    )),
                    ratatui::widgets::Cell::from(Span::styled(
                        "Active",
                        Style::default().fg(Color::Yellow),
                    )),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [Constraint::Percentage(65), Constraint::Percentage(35)],
        )
        .block(block)
        .header(
            Row::new(vec!["  Task ID", "Status"])
                .style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
        );

        frame.render_widget(table, area);
    }

    /// Render the console output area (mixed console messages + recent events).
    fn render_console_output(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Console Output ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        let inner_height = area.height.saturating_sub(2) as usize;

        if self.console_messages.is_empty() {
            let text = Paragraph::new(Line::from(Span::styled(
                "  Waiting for events...",
                Style::default().fg(Color::DarkGray),
            )))
            .block(block);
            frame.render_widget(text, area);
            return;
        }

        // Show the most recent messages that fit.
        let start = self.console_messages.len().saturating_sub(inner_height);
        let visible = &self.console_messages[start..];

        let lines: Vec<Line> = visible
            .iter()
            .map(|(ts, msg, color)| {
                let time_str = ts.format("%H:%M:%S").to_string();
                Line::from(vec![
                    Span::styled(
                        format!("  [{}] ", time_str),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(msg.as_str(), Style::default().fg(*color)),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render the input area at the bottom.
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Command Input (Enter = inject task, /help = commands, /quit = exit) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        // Show the input with cursor.
        let input_display = if self.input.is_empty() {
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(Color::Green)),
                Span::styled(
                    "Type a task description or /command...",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(Color::Green)),
                Span::styled(&self.input, Style::default().fg(Color::White)),
            ])
        };

        let hint_line = Line::from(vec![
            Span::styled(
                "  Ctrl+C or /quit to exit  |  Up/Down for history  |  Enter to submit",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let paragraph = Paragraph::new(vec![
            Line::from(""),
            input_display,
            hint_line,
        ])
        .block(block);
        frame.render_widget(paragraph, area);

        // Position cursor.
        let cursor_x = area.x + 4 + self.cursor_pos as u16;
        let cursor_y = area.y + 2;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    /// Handle keyboard input. Returns `true` if the console should exit.
    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match (code, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
            (KeyCode::Char(c), _) => {
                self.input.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
            }
            (KeyCode::Backspace, _) => {
                if self.cursor_pos > 0 {
                    self.input.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
            }
            (KeyCode::Delete, _) => {
                if self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                }
            }
            (KeyCode::Left, _) => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            (KeyCode::Right, _) => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
            }
            (KeyCode::Home, _) => {
                self.cursor_pos = 0;
            }
            (KeyCode::End, _) => {
                self.cursor_pos = self.input.len();
            }
            (KeyCode::Up, _) => {
                if !self.history.is_empty() {
                    let pos = match self.history_pos {
                        Some(p) if p > 0 => p - 1,
                        Some(p) => p,
                        None => self.history.len() - 1,
                    };
                    self.history_pos = Some(pos);
                    self.input = self.history[pos].clone();
                    self.cursor_pos = self.input.len();
                }
            }
            (KeyCode::Down, _) => {
                if let Some(pos) = self.history_pos {
                    if pos + 1 < self.history.len() {
                        let new_pos = pos + 1;
                        self.history_pos = Some(new_pos);
                        self.input = self.history[new_pos].clone();
                        self.cursor_pos = self.input.len();
                    } else {
                        self.history_pos = None;
                        self.input.clear();
                        self.cursor_pos = 0;
                    }
                }
            }
            (KeyCode::Enter, _) => {
                // Handled by caller (needs async).
            }
            (KeyCode::PageUp, _) => {
                self.hierarchy_scroll = self.hierarchy_scroll.saturating_sub(5);
            }
            (KeyCode::PageDown, _) => {
                self.hierarchy_scroll += 5;
            }
            _ => {}
        }
        false
    }
}

/// Build a hierarchy tree from the connector state.
fn build_hierarchy_tree(state: &ConnectorState) -> Vec<HierarchyNode> {
    let self_id = state.agent_id.to_string();
    let peers = state.agent_set.elements();

    // Self node is always the root in the display.
    let mut self_node = HierarchyNode {
        agent_id: self_id.clone(),
        display_name: truncate_agent_id(&self_id),
        tier: format_tier(&state.my_tier),
        is_self: true,
        children: Vec::new(),
        task_count: state.task_set.len(),
    };

    // Add peers as children (we don't have full hierarchy info,
    // so show them as subordinates for now).
    for peer in &peers {
        let child = HierarchyNode {
            agent_id: peer.clone(),
            display_name: truncate_agent_id(peer),
            tier: "Peer".to_string(),
            is_self: false,
            children: Vec::new(),
            task_count: 0,
        };
        self_node.children.push(child);
    }

    if self_node.agent_id.is_empty() {
        Vec::new()
    } else {
        vec![self_node]
    }
}

/// Render hierarchy tree into display lines.
fn render_hierarchy_lines(
    node: &HierarchyNode,
    prefix: &str,
    is_last: bool,
    lines: &mut Vec<Line<'static>>,
) {
    let branch = if prefix.is_empty() {
        "  ".to_string()
    } else if is_last {
        "  └── ".to_string()
    } else {
        "  ├── ".to_string()
    };

    let tier_color = match node.tier.as_str() {
        "Tier1" => Color::Red,
        "Tier2" => Color::Yellow,
        "Peer" => Color::Cyan,
        _ => Color::White,
    };

    let self_marker = if node.is_self { " (you)" } else { "" };
    let task_info = if node.task_count > 0 {
        format!(" [{}t]", node.task_count)
    } else {
        String::new()
    };

    lines.push(Line::from(vec![
        Span::styled(branch, Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("[{}]", node.tier),
            Style::default().fg(tier_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", node.display_name),
            Style::default().fg(if node.is_self {
                Color::White
            } else {
                Color::Gray
            }),
        ),
        Span::styled(
            self_marker.to_string(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(task_info, Style::default().fg(Color::Yellow)),
    ]));

    let child_prefix = if prefix.is_empty() {
        "  ".to_string()
    } else if is_last {
        format!("{}     ", prefix)
    } else {
        format!("{}  │  ", prefix)
    };

    for (i, child) in node.children.iter().enumerate() {
        let is_child_last = i == node.children.len() - 1;
        render_hierarchy_lines(child, &child_prefix, is_child_last, lines);
    }
}

/// Print hierarchy node to console messages (for /hierarchy command).
fn print_hierarchy_node(
    node: &HierarchyNode,
    prefix: &str,
    is_last: bool,
    messages: &mut Vec<(chrono::DateTime<chrono::Utc>, String, Color)>,
) {
    let branch = if prefix.is_empty() {
        "".to_string()
    } else if is_last {
        format!("{}└── ", prefix)
    } else {
        format!("{}├── ", prefix)
    };

    let self_marker = if node.is_self { " (you)" } else { "" };
    messages.push((
        chrono::Utc::now(),
        format!("  {}[{}] {}{}", branch, node.tier, node.display_name, self_marker),
        if node.is_self { Color::Green } else { Color::White },
    ));

    let child_prefix = if prefix.is_empty() {
        "".to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    for (i, child) in node.children.iter().enumerate() {
        let is_child_last = i == node.children.len() - 1;
        print_hierarchy_node(child, &child_prefix, is_child_last, messages);
    }
}

fn truncate_agent_id(id: &str) -> String {
    if id.len() > 28 {
        format!("{}...{}", &id[..16], &id[id.len() - 8..])
    } else {
        id.to_string()
    }
}

fn format_tier(tier: &Tier) -> String {
    match tier {
        Tier::Tier1 => "Tier1".to_string(),
        Tier::Tier2 => "Tier2".to_string(),
        Tier::TierN(n) => format!("Tier{}", n),
        Tier::Executor => "Executor".to_string(),
    }
}

fn format_status(status: &ConnectorStatus) -> String {
    match status {
        ConnectorStatus::Initializing => "Initializing".to_string(),
        ConnectorStatus::Running => "Running".to_string(),
        ConnectorStatus::InElection => "In Election".to_string(),
        ConnectorStatus::ShuttingDown => "Shutting Down".to_string(),
    }
}

fn status_color(status: &ConnectorStatus) -> Color {
    match status {
        ConnectorStatus::Initializing => Color::Yellow,
        ConnectorStatus::Running => Color::Green,
        ConnectorStatus::InElection => Color::Magenta,
        ConnectorStatus::ShuttingDown => Color::Red,
    }
}

/// Set up the terminal for TUI rendering.
fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to its original state.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Run the operator console event loop.
///
/// This is the main entry point for the operator console. It provides an
/// interactive TUI where a human operator can:
/// - Type task descriptions and press Enter to inject them into the swarm
/// - View the agent hierarchy tree
/// - Monitor active tasks and events
/// - Use slash commands for additional operations
pub async fn run_operator_console(
    state: Arc<RwLock<ConnectorState>>,
) -> Result<(), anyhow::Error> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(anyhow::anyhow!(
            "Operator console requires a terminal (TTY)."
        ));
    }

    // Set up panic hook to restore terminal.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let mut terminal = setup_terminal()?;
    let mut console = OperatorConsole::new(state);

    let tick_rate = Duration::from_millis(100); // ~10fps

    loop {
        // Take a snapshot.
        let snapshot = console.snapshot().await;

        // Render.
        terminal.draw(|frame| {
            console.render(frame, &snapshot);
        })?;

        // Poll for keyboard events.
        if event::poll(tick_rate)? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    // Check for quit commands first.
                    if key_event.code == KeyCode::Enter {
                        // Check if input is a quit command.
                        let trimmed = console.input.trim().to_string();
                        if trimmed == "/quit" || trimmed == "/exit" || trimmed == "/q" {
                            break;
                        }
                        console.process_input().await;
                    } else if console.handle_key(key_event.code, key_event.modifiers) {
                        break; // Ctrl+C
                    }
                }
            }
        }
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}
