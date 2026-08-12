use crate::app::{App, InstallFlow, ProviderAddFlow, Tab, TaskAddFlow};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const BAD: Color = Color::Red;
const MUTED: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Length(3), // tab bar
        Constraint::Min(0),    // content
        Constraint::Length(1), // footer
    ])
    .split(area);

    draw_header(frame, chunks[0], app);
    draw_tabs(frame, chunks[1], app);
    draw_content(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    if !matches!(app.install, InstallFlow::Idle) {
        draw_install_modal(frame, area, app);
    }
    if !matches!(app.provider_add, ProviderAddFlow::Idle) {
        draw_provider_add_modal(frame, area, app);
    }
    if !matches!(app.task_add, TaskAddFlow::Idle) {
        draw_task_add_modal(frame, area, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let text = match &app.status {
        Some(s) => format!(
            "SingleCLI  ·  profile: {}  ·  agents: {}/{} detected  ·  v{}",
            s.active_profile, s.agents_detected, s.agents_known, s.version
        ),
        None => "SingleCLI  ·  runtime unreachable".to_string(),
    };
    let style = if app.status.is_some() { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(BAD) };
    let header = Paragraph::new(Line::from(Span::styled(text, style)))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)));
    frame.render_widget(header, area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Span> = Tab::ALL
        .iter()
        .map(|t| {
            let label = format!(" {} ", t.title());
            if *t == app.tab {
                Span::styled(label, Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(label, Style::default().fg(Color::White))
            }
        })
        .collect();
    let mut spans = Vec::new();
    for (i, span) in titles.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(span);
    }
    let tabs = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));
    frame.render_widget(tabs, area);
}

fn draw_content(frame: &mut Frame, area: Rect, app: &App) {
    match app.tab {
        Tab::Agents => draw_agents(frame, area, app),
        Tab::Tasks => draw_tasks(frame, area, app),
        Tab::Mcp => draw_mcp(frame, area, app),
        Tab::Providers => draw_providers(frame, area, app),
        Tab::Accounts => draw_accounts(frame, area, app),
        Tab::Memory => draw_memory(frame, area, app),
        Tab::Help => draw_help(frame, area),
    }
}

fn selected_style() -> Style {
    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
}

fn draw_agents(frame: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (dot, color) = if a.detected { ("●", OK) } else { ("○", MUTED) };
            let caps = [
                (a.capabilities.mcp, "mcp"),
                (a.capabilities.lsp, "lsp"),
                (a.capabilities.tools, "tools"),
                (a.capabilities.sessions, "sessions"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join(",");
            let flag = if a.unverified { "unverified" } else { "" };
            let style = if i == app.selected && app.tab == Tab::Agents { selected_style() } else { Style::default() };
            Row::new(vec![
                Cell::from(Span::styled(dot, Style::default().fg(color))),
                Cell::from(a.name.clone()),
                Cell::from(a.version.clone().unwrap_or_else(|| "-".into())),
                Cell::from(caps),
                Cell::from(flag),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Length(2), Constraint::Length(12), Constraint::Length(26), Constraint::Min(20), Constraint::Length(12)],
    )
    .header(Row::new(vec!["", "Agent", "Version", "Capabilities", ""]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title(" Agents — [i] install selected, [enter] inspect "));
    frame.render_widget(table, area);
}

fn draw_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (label, color) = match t.status {
                single_protocol::TaskStatus::Completed => ("completed", OK),
                single_protocol::TaskStatus::Failed => ("failed", BAD),
                single_protocol::TaskStatus::Running => ("running", WARN),
                single_protocol::TaskStatus::Created => ("created", MUTED),
            };
            let style = if i == app.selected && app.tab == Tab::Tasks { selected_style() } else { Style::default() };
            Row::new(vec![
                Cell::from(format!("#{}", t.id)),
                Cell::from(Span::styled(label, Style::default().fg(color))),
                Cell::from(t.agent.clone()),
                Cell::from(t.description.clone()),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(rows, [Constraint::Length(6), Constraint::Length(12), Constraint::Length(12), Constraint::Min(20)])
        .header(Row::new(vec!["ID", "Status", "Agent", "Description"]).style(Style::default().add_modifier(Modifier::BOLD)))
        .block(Block::default().borders(Borders::ALL).title(" Tasks — [n] new task "));
    frame.render_widget(table, area);
}

fn draw_mcp(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .mcp_servers
        .iter()
        .map(|s| {
            let (flag, color) = if s.enabled { ("enabled", OK) } else { ("disabled", MUTED) };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<14}", s.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<40} ", s.command)),
                Span::styled(format!("[{flag}]"), Style::default().fg(color)),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" MCP servers "));
    frame.render_widget(list, area);
}

fn draw_providers(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .providers
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<16}", p.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<22} ", p.env_var_name)),
                Span::styled(p.base_url.clone().unwrap_or_else(|| "-".into()), Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Providers — [a] add "));
    frame.render_widget(list, area);
}

fn draw_accounts(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .accounts
        .iter()
        .map(|a| {
            let flag = if a.unverified_complete { " (best-effort)" } else { "" };
            let (status_label, status_color) = match a.status {
                single_protocol::AccountStatus::Available => ("available", OK),
                single_protocol::AccountStatus::RateLimited => ("rate_limited", WARN),
                single_protocol::AccountStatus::NeedsTopup => ("needs_topup", BAD),
                single_protocol::AccountStatus::Unknown => ("unknown", MUTED),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<12}", a.agent), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<16} ", a.name)),
                Span::raw(format!("{:<24} ", a.label.as_deref().unwrap_or("-"))),
                Span::styled(format!("[{status_label}] "), Style::default().fg(status_color)),
                Span::styled(format!("{}{flag}", a.captured_at), Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Accounts — single account capture/use <agent> <name> "));
    frame.render_widget(list, area);
}

fn draw_memory(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![
        Line::from(Span::styled("Knowledge graph (SQLite)", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(format!("  entities: {}", app.kg_entity_count.map(|n| n.to_string()).unwrap_or_else(|| "-".into()))),
        Line::from(""),
        Line::from(Span::styled("Redis working memory", Style::default().add_modifier(Modifier::BOLD))),
    ];
    lines.push(status_line(app.cache_configured, app.cache_reachable, "SINGLE_REDIS_URL"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Qdrant vector store", Style::default().add_modifier(Modifier::BOLD))));
    lines.push(status_line(app.vector_configured, app.vector_reachable, "SINGLE_QDRANT_URL"));

    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Memory "));
    frame.render_widget(p, area);
}

fn status_line(configured: bool, reachable: bool, env_var: &str) -> Line<'static> {
    if !configured {
        Line::from(vec![Span::styled(format!("  not configured (set {env_var})"), Style::default().fg(MUTED))])
    } else if reachable {
        Line::from(vec![Span::styled("  ● reachable", Style::default().fg(OK))])
    } else {
        Line::from(vec![Span::styled("  ○ configured but unreachable", Style::default().fg(BAD))])
    }
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  tab / shift+tab      switch tabs"),
        Line::from("  ↑/↓ or k/j           move selection"),
        Line::from("  r                    refresh"),
        Line::from("  q / esc              quit"),
        Line::from(""),
        Line::from(Span::styled("Agents tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  i                    install the selected agent (confirms before running anything)"),
        Line::from(""),
        Line::from(Span::styled("Providers tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  a                    add a provider (OpenAI/Anthropic/OpenCode Zen/NVIDIA presets), key goes straight to your OS keychain"),
        Line::from(""),
        Line::from(Span::styled("Tasks tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  n                    new task: description, workspace path, then pick one or more agents (space to toggle)"),
        Line::from(""),
        Line::from(Span::styled("Everything else", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  Use the `single` CLI for actions not yet in the TUI: mcp add, provider add/sync,"),
        Line::from("  account capture/use, task run, memory graph/cache/vector — see `single --help`."),
    ];
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Help "));
    frame.render_widget(p, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let text = if let Some(err) = &app.error {
        Line::from(Span::styled(format!("error: {err}"), Style::default().fg(BAD)))
    } else {
        Line::from(Span::styled("[tab] switch  [↑↓] move  [i] install agent  [a] add provider  [n] new task  [r] refresh  [q] quit", Style::default().fg(MUTED)))
    };
    frame.render_widget(Paragraph::new(text), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1]);
    horizontal[1]
}

fn draw_provider_add_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, modal_area);

    let (title, lines): (&str, Vec<Line>) = match &app.provider_add {
        ProviderAddFlow::PickingPreset { presets, selected } => {
            let mut lines = vec![
                Line::from(Span::styled("Add a provider", Style::default().add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];
            for (i, p) in presets.iter().enumerate() {
                let style = if i == *selected { selected_style() } else { Style::default() };
                lines.push(Line::from(Span::styled(format!("  {:<14} {:<20} {}", p.name, p.env_var_name, p.base_url), style)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("[↑↓] choose  [enter] select  [esc] cancel"));
            (" Add provider ", lines)
        }
        ProviderAddFlow::EnteringKey { preset, input } => (
            " Enter API key ",
            vec![
                Line::from(Span::styled(format!("{} ({})", preset.name, preset.env_var_name), Style::default().add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(preset.base_url.clone(), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from(format!("API key: {}", "*".repeat(input.chars().count()))),
                Line::from(""),
                Line::from(Span::styled("stored in your OS keychain, never written to a config file", Style::default().fg(MUTED))),
                Line::from(""),
                Line::from("[enter] save  [esc] cancel"),
            ],
        ),
        ProviderAddFlow::Submitting { preset_name, .. } => (
            " Saving… ",
            vec![Line::from(format!("Registering {preset_name} and storing the key in your OS keychain..."))],
        ),
        ProviderAddFlow::Done { preset_name } => (
            " Provider added ",
            vec![
                Line::from(Span::styled(format!("{preset_name} added"), Style::default().fg(OK))),
                Line::from(""),
                Line::from("Run `single provider sync <name> --agents <agent> --yes` to write it into an agent's config,"),
                Line::from("or do it from the CLI — sync isn't wired into the TUI yet."),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        ProviderAddFlow::Failed { preset_name, error } => (
            " Failed ",
            vec![
                Line::from(Span::styled(format!("Could not add {preset_name}"), Style::default().fg(BAD))),
                Line::from(""),
                Line::from(error.clone()),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        ProviderAddFlow::Idle => (" ", vec![]),
    };

    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT));
    frame.render_widget(Paragraph::new(lines).block(block), modal_area);
}

fn draw_task_add_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(65, 55, area);
    frame.render_widget(Clear, modal_area);

    let (title, lines): (&str, Vec<Line>) = match &app.task_add {
        TaskAddFlow::EnteringDescription { input } => (
            " New task — description ",
            vec![
                Line::from(format!("Description: {input}")),
                Line::from(""),
                Line::from("[enter] next  [esc] cancel"),
            ],
        ),
        TaskAddFlow::EnteringCwd { description, input } => (
            " New task — workspace path ",
            vec![
                Line::from(Span::styled(description.clone(), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from(format!("Workspace path: {input}")),
                Line::from(Span::styled("  (defaults to \".\" if left empty)", Style::default().fg(MUTED))),
                Line::from(""),
                Line::from("[enter] next  [esc] cancel"),
            ],
        ),
        TaskAddFlow::PickingAgents { description, cwd, agent_names, chosen, cursor } => {
            let mut lines = vec![
                Line::from(Span::styled(description.clone(), Style::default().fg(MUTED))),
                Line::from(Span::styled(format!("cwd: {cwd}"), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from("Agents (space to toggle; picking >1 orchestrates them together):"),
            ];
            if agent_names.is_empty() {
                lines.push(Line::from(Span::styled("  (no detected agents)", Style::default().fg(BAD))));
            }
            for (i, name) in agent_names.iter().enumerate() {
                let mark = if chosen[i] { "[x]" } else { "[ ]" };
                let style = if i == *cursor { selected_style() } else { Style::default() };
                lines.push(Line::from(Span::styled(format!("  {mark} {name}"), style)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("[↑↓] move  [space] toggle  [enter] run  [esc] cancel"));
            (" New task — agent(s) ", lines)
        }
        TaskAddFlow::Submitting { .. } => (" Running… ", vec![Line::from("Submitting to the runtime...")]),
        TaskAddFlow::Done { count } => (
            " Task submitted ",
            vec![
                Line::from(Span::styled(format!("Started against {count} agent(s)"), Style::default().fg(OK))),
                Line::from(""),
                Line::from("Check the Tasks tab for progress."),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        TaskAddFlow::Failed { error } => (
            " Failed ",
            vec![
                Line::from(Span::styled("Could not start task", Style::default().fg(BAD))),
                Line::from(""),
                Line::from(error.clone()),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        TaskAddFlow::Idle => (" ", vec![]),
    };

    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT));
    frame.render_widget(Paragraph::new(lines).block(block), modal_area);
}

fn draw_install_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(60, 40, area);
    frame.render_widget(Clear, modal_area);

    let (title, lines): (&str, Vec<Line>) = match &app.install {
        InstallFlow::Confirming { agent, command, source } => (
            " Install agent ",
            vec![
                Line::from(Span::styled(format!("Install {agent}?"), Style::default().add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from("This will run:"),
                Line::from(Span::styled(format!("  {command}"), Style::default().fg(WARN))),
                Line::from(""),
                Line::from(Span::styled(format!("source: {source}"), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from("[y] confirm    [n/esc] cancel"),
            ],
        ),
        InstallFlow::Running { agent, started_at, .. } => (
            " Installing… ",
            vec![
                Line::from(format!("Installing {agent}...")),
                Line::from(""),
                Line::from(Span::styled(format!("elapsed: {}s", started_at.elapsed().as_secs()), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from(Span::styled("this may take a while (network install script)", Style::default().fg(MUTED))),
            ],
        ),
        InstallFlow::Done { agent, action } => (
            " Install finished ",
            vec![
                Line::from(Span::styled(format!("{agent}: {}", if action.executed { "installed" } else { "not installed" }), Style::default().fg(if action.executed { OK } else { BAD }))),
                Line::from(""),
                Line::from(action.detail.clone()),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        InstallFlow::Failed { agent, error } => (
            " Install failed ",
            vec![
                Line::from(Span::styled(format!("{agent} install failed"), Style::default().fg(BAD))),
                Line::from(""),
                Line::from(error.clone()),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        InstallFlow::Idle => (" ", vec![]),
    };

    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal_area);
}
