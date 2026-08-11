use crate::client::call;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Terminal;
use single_protocol::{AgentInfo, Request, Response, ResponseData, RuntimeStatus};
use std::path::Path;
use std::time::Duration;

struct DashboardState {
    status: Option<RuntimeStatus>,
    agents: Vec<AgentInfo>,
    error: Option<String>,
}

fn fetch(socket_path: &Path) -> DashboardState {
    let status = match call(socket_path, &Request::Status) {
        Ok(Response::Ok { data: ResponseData::Status(s) }) => Some(s),
        _ => None,
    };
    match call(socket_path, &Request::AgentList) {
        Ok(Response::Ok { data: ResponseData::Agents(agents) }) => {
            DashboardState { status, agents, error: None }
        }
        Ok(Response::Error { message }) => DashboardState { status, agents: Vec::new(), error: Some(message) },
        Ok(_) => DashboardState { status, agents: Vec::new(), error: Some("unexpected response".into()) },
        Err(e) => DashboardState { status, agents: Vec::new(), error: Some(e.to_string()) },
    }
}

/// Renders the agent dashboard (spec section 4), scoped to what Phase 1
/// tracks: per-agent detection/version/capabilities sourced live from the
/// runtime. Task graphs, MCP/LSP activity panes, and agent terminals are
/// later-phase additions to this same screen.
pub fn run(socket_path: &Path) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = fetch(socket_path);

    loop {
        terminal.draw(|frame| draw(frame, &state))?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => state = fetch(socket_path),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &DashboardState) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let header_text = match &state.status {
        Some(s) => format!(
            "SingleCLI — profile: {}  agents: {}/{} detected",
            s.active_profile, s.agents_detected, s.agents_known
        ),
        None => "SingleCLI — runtime unreachable".to_string(),
    };
    let header = Paragraph::new(Line::from(Span::styled(
        header_text,
        Style::default().add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::ALL).title("SingleCLI"));
    frame.render_widget(header, chunks[0]);

    let rows: Vec<Row> = state
        .agents
        .iter()
        .map(|a| {
            let dot = if a.detected { "●" } else { "○" };
            let color = if a.detected { Color::Green } else { Color::DarkGray };
            let caps = [
                (a.capabilities.mcp, "mcp"),
                (a.capabilities.lsp, "lsp"),
                (a.capabilities.tools, "tools"),
                (a.capabilities.sessions, "sessions"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(",");
            Row::new(vec![
                Cell::from(Span::styled(dot, Style::default().fg(color))),
                Cell::from(a.name.clone()),
                Cell::from(a.version.clone().unwrap_or_else(|| "-".into())),
                Cell::from(caps),
                Cell::from(if a.unverified { "unverified" } else { "" }),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Length(24),
            Constraint::Min(20),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(vec!["", "Agent", "Version", "Capabilities", ""]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("Agents"));
    frame.render_widget(table, chunks[1]);

    let footer_text = state.error.clone().unwrap_or_else(|| "[q] quit  [r] refresh".to_string());
    let footer_style = if state.error.is_some() { Style::default().fg(Color::Red) } else { Style::default() };
    frame.render_widget(Paragraph::new(footer_text).style(footer_style), chunks[2]);
}
