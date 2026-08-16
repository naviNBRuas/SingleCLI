use crate::app::{App, BackupFlow, BackupMode, BackupOutcome, InstallFlow, ProviderAddFlow, QuickAddFlow, Tab, TaskAddFlow, TaskDetailFlow};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap};
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
    if !matches!(app.quick_add, QuickAddFlow::Idle) {
        draw_quick_add_modal(frame, area, app);
    }
    if !matches!(app.task_detail, TaskDetailFlow::Idle) {
        draw_task_detail_modal(frame, area, app);
    }
    if !matches!(app.backup, BackupFlow::Idle) {
        draw_backup_modal(frame, area, app);
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
        Tab::Lsp => draw_lsp(frame, area, app),
        Tab::Plugins => draw_plugins(frame, area, app),
        Tab::Tools => draw_tools(frame, area, app),
        Tab::Providers => draw_providers(frame, area, app),
        Tab::Accounts => draw_accounts(frame, area, app),
        Tab::Usage => draw_usage(frame, area, app),
        Tab::Backup => draw_backup(frame, area, app),
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
            let (auth_label, auth_color) = match a.authenticated {
                single_protocol::AuthState::Authenticated => ("auth", OK),
                single_protocol::AuthState::NotAuthenticated => ("no auth", MUTED),
                single_protocol::AuthState::Unsupported => ("-", MUTED),
            };
            let flag = if a.unverified { "unverified" } else { "" };
            let style = if i == app.selected && app.tab == Tab::Agents { selected_style() } else { Style::default() };
            Row::new(vec![
                Cell::from(Span::styled(dot, Style::default().fg(color))),
                Cell::from(a.name.clone()),
                Cell::from(a.version.clone().unwrap_or_else(|| "-".into())),
                Cell::from(Span::styled(auth_label, Style::default().fg(auth_color))),
                Cell::from(caps),
                Cell::from(flag),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Length(9),
            Constraint::Min(20),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(vec!["", "Agent", "Version", "Auth", "Capabilities", ""]).style(Style::default().add_modifier(Modifier::BOLD)))
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
        .block(Block::default().borders(Borders::ALL).title(" Tasks — [n] new task  [enter] view output "));
    frame.render_widget(table, area);
}

fn draw_mcp(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .mcp_servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (flag, color) = if s.enabled { ("enabled", OK) } else { ("disabled", MUTED) };
            let style = if i == app.selected && app.tab == Tab::Mcp { selected_style() } else { Style::default() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<14}", s.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<40} ", s.command)),
                Span::styled(format!("[{flag}]"), Style::default().fg(color)),
            ]))
            .style(style)
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" MCP servers — [a] add  [e] enable/disable  [d] remove "));
    frame.render_widget(list, area);
}

fn draw_lsp(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .lsp_servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (flag, color) = if s.enabled { ("enabled", OK) } else { ("disabled", MUTED) };
            let style = if i == app.selected && app.tab == Tab::Lsp { selected_style() } else { Style::default() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<14}", s.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<26} ", s.command)),
                Span::raw(format!("{:<20} ", s.extensions.join(","))),
                Span::styled(format!("[{flag}]"), Style::default().fg(color)),
            ]))
            .style(style)
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" LSP servers — [a] add  [d] remove "));
    frame.render_widget(list, area);
}

fn draw_plugins(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .plugins
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == app.selected && app.tab == Tab::Plugins { selected_style() } else { Style::default() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<16}", p.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<28} ", p.target)),
                Span::styled(p.opencode_module.clone().unwrap_or_else(|| "-".into()), Style::default().fg(MUTED)),
            ]))
            .style(style)
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Plugins — [a] add  [s] sync to all agents  [d] remove "));
    frame.render_widget(list, area);
}

fn draw_tools(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .tools
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (flag, color) = if t.enabled { ("enabled", OK) } else { ("disabled", MUTED) };
            let (risk_label, risk_color) = match t.risk_level {
                single_protocol::RiskLevel::Low => ("low", OK),
                single_protocol::RiskLevel::Medium => ("medium", WARN),
                single_protocol::RiskLevel::High => ("high", BAD),
            };
            let style = if i == app.selected && app.tab == Tab::Tools { selected_style() } else { Style::default() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<14}", t.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:<34} ", t.description)),
                Span::styled(format!("{risk_label:<8}"), Style::default().fg(risk_color)),
                Span::styled(format!("[{flag}]"), Style::default().fg(color)),
            ]))
            .style(style)
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Tools — [a] add  [e] enable/disable "));
    frame.render_widget(list, area);
}

fn draw_providers(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = if app.providers.is_empty() {
        vec![ListItem::new(Span::styled(
            "(no providers configured yet — press [a] to add one from the full preset catalog)",
            Style::default().fg(MUTED),
        ))]
    } else {
        app.providers
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<16}", p.name), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{:<22} ", p.env_var_name)),
                    Span::styled(p.base_url.clone().unwrap_or_else(|| "-".into()), Style::default().fg(MUTED)),
                ]))
            })
            .collect()
    };
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Providers (configured) — [a] add "));
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

fn draw_usage(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);

    let Some(usage) = &app.usage else {
        let text = if app.usage_loading { "Loading usage…" } else { "No usage data yet — press 'r' to fetch" };
        let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Usage "));
        frame.render_widget(p, area);
        return;
    };

    let rows: Vec<Row> = usage
        .provider_usage
        .iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.provider.clone()),
                Cell::from(r.key_label.clone().unwrap_or_else(|| "-".into())),
                Cell::from(r.agent.clone().unwrap_or_else(|| "-".into())),
                Cell::from(format!("${:.4}", r.cost_usd)),
                Cell::from(format!("{} .. {}", r.period_start, r.period_end)),
            ])
        })
        .collect();
    let widths = [Constraint::Length(12), Constraint::Length(10), Constraint::Length(12), Constraint::Length(12), Constraint::Min(20)];
    let title = format!(
        " Provider spend — total: ${:.4}  (as of {}) ",
        usage.total_usd,
        usage.last_refreshed.as_deref().unwrap_or("never")
    );
    let table = Table::new(rows, widths)
        .header(Row::new(vec!["provider", "key", "agent", "cost", "period"]).style(Style::default().add_modifier(Modifier::BOLD)))
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, chunks[0]);

    let items: Vec<ListItem> = usage
        .agent_local_stats
        .iter()
        .map(|a| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<14}", a.agent), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("runs: {:<5} ", a.run_count)),
                Span::raw(format!("avg: {}ms  ", a.avg_duration_ms)),
                Span::styled(format!("last: {}", a.last_run_at.as_deref().unwrap_or("never")), Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Connected agents — local stats only, no billing API "));
    frame.render_widget(list, chunks[1]);
}

fn draw_backup(frame: &mut Frame, area: Rect, _app: &App) {
    let lines = vec![
        Line::from(Span::styled("Full-setup backup/restore", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  Captures: config, every agent's real credentials, keychain secrets"),
        Line::from("  (provider keys, billing keys, anything set via `single secret set`),"),
        Line::from("  and task/memory/knowledge-graph history — encrypted with a passphrase."),
        Line::from(""),
        Line::from(vec![Span::styled("  [x]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::raw(" export to a new archive")]),
        Line::from(vec![
            Span::styled("  [i]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(" preview an import from an existing archive (dry run only — apply for real with `single backup import <path> --yes`)"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  If single-runtimed is running, stop it first (`single daemon stop`) so state/single.db isn't captured mid-write.",
            Style::default().fg(MUTED),
        )),
    ];
    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(" Backup "));
    frame.render_widget(p, area);
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
        Line::from("  enter                view a task's output — live-tailed while it's running, full output once it finishes."),
        Line::from("                       orchestrate runs create one row per agent per step, so select any row to see just that agent."),
        Line::from(""),
        Line::from(Span::styled("MCP / LSP / Plugins / Tools tabs", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  a                    quick add (one line, pipe-separated fields — shown in the modal)"),
        Line::from("  d                    remove the selected entry (MCP/LSP/Plugins)"),
        Line::from("  e                    toggle enabled/disabled (MCP/Tools)"),
        Line::from("  s                    sync the selected plugin into every registered agent (Plugins tab)"),
        Line::from(""),
        Line::from(Span::styled("Usage tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  refetches on tab entry / r    real $ spend needs a billing admin key: `single provider set-billing-key <name> <key>`"),
        Line::from("                                agents with no billing API (claude, codex, cursor, ...) show local run stats only."),
        Line::from(""),
        Line::from(Span::styled("Backup tab", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  x                    export your entire setup (config, credentials, keychain secrets) to a password-encrypted archive"),
        Line::from("  i                    preview restoring from an archive (dry run only — apply for real with `single backup import <path> --yes`)"),
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
        Line::from(Span::styled("[tab] switch  [↑↓] move  [a] add  [d] remove  [e] toggle  [s] sync  [i] install/import  [x] backup export  [n] new task  [r] refresh  [q] quit", Style::default().fg(MUTED)))
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

fn draw_backup_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(70, 60, area);
    frame.render_widget(Clear, modal_area);

    let (title, lines): (&str, Vec<Line>) = match &app.backup {
        BackupFlow::EnteringPath { mode, input } => {
            let verb = if *mode == BackupMode::Export { "Export to" } else { "Import from" };
            (
                " Backup path ",
                vec![
                    Line::from(format!("{verb}: {input}")),
                    Line::from(""),
                    Line::from("[enter] continue  [esc] cancel"),
                ],
            )
        }
        BackupFlow::EnteringPassphrase { input, .. } => (
            " Passphrase ",
            vec![
                Line::from(format!("Passphrase: {}", "*".repeat(input.chars().count()))),
                Line::from(""),
                Line::from(Span::styled("never sent over the daemon socket, never logged", Style::default().fg(MUTED))),
                Line::from(""),
                Line::from("[enter] continue  [esc] cancel"),
            ],
        ),
        BackupFlow::ConfirmingPassphrase { input, .. } => (
            " Confirm passphrase ",
            vec![
                Line::from(format!("Confirm: {}", "*".repeat(input.chars().count()))),
                Line::from(""),
                Line::from("[enter] continue  [esc] cancel"),
            ],
        ),
        BackupFlow::Submitting { mode, .. } => {
            let verb = if *mode == BackupMode::Export { "Encrypting and writing archive" } else { "Decrypting and previewing archive" };
            (" Working… ", vec![Line::from(format!("{verb}..."))])
        }
        BackupFlow::Done { mode: BackupMode::Export, outcome: BackupOutcome::Exported { path, warnings } } => {
            let mut lines = vec![Line::from(Span::styled(format!("Backup written to {path}"), Style::default().fg(OK))), Line::from("")];
            for w in warnings {
                lines.push(Line::from(Span::styled(format!("warning: {w}"), Style::default().fg(WARN))));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("[enter/esc] close"));
            (" Backup complete ", lines)
        }
        BackupFlow::Done { mode: BackupMode::Import, outcome: BackupOutcome::Imported { report } } => {
            let ok = report.files.iter().chain(&report.secrets).filter(|i| i.success).count();
            let total = report.files.len() + report.secrets.len();
            let mut lines = vec![
                Line::from(Span::styled(format!("Preview: {ok}/{total} items would restore cleanly"), Style::default().add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled("This was a preview only — nothing was written.", Style::default().fg(MUTED))),
                Line::from(Span::styled("Run `single backup import <path> --yes` to actually apply it.", Style::default().fg(MUTED))),
                Line::from(""),
            ];
            for item in report.files.iter().chain(&report.secrets).filter(|i| !i.success).take(8) {
                lines.push(Line::from(Span::styled(format!("✗ {}: {}", item.path, item.detail), Style::default().fg(BAD))));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("[enter/esc] close"));
            (" Import preview ", lines)
        }
        BackupFlow::Done { .. } => (" ", vec![]), // unreachable: mode/outcome always match
        BackupFlow::Failed { error, .. } => (
            " Failed ",
            vec![Line::from(Span::styled(error.clone(), Style::default().fg(BAD))), Line::from(""), Line::from("[enter/esc] close")],
        ),
        BackupFlow::Idle => (" ", vec![]),
    };

    let block = Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), modal_area);
}

fn draw_task_detail_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(90, 85, area);
    frame.render_widget(Clear, modal_area);

    let TaskDetailFlow::Viewing { task, output, .. } = &app.task_detail else { return };

    let (status_label, status_color) = match task.status {
        single_protocol::TaskStatus::Completed => ("completed", OK),
        single_protocol::TaskStatus::Failed => ("failed", BAD),
        single_protocol::TaskStatus::Running => ("running…", WARN),
        single_protocol::TaskStatus::Created => ("created", MUTED),
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Task #{} — {} ", task.id, task.agent))
        .border_style(Style::default().fg(ACCENT));
    let inner = outer.inner(modal_area);
    frame.render_widget(outer, modal_area);

    let sections = Layout::vertical([Constraint::Length(4), Constraint::Min(0), Constraint::Length(1)]).split(inner);

    let mut header_lines = vec![
        Line::from(vec![
            Span::styled("status: ", Style::default().fg(MUTED)),
            Span::styled(status_label, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled("exit: ", Style::default().fg(MUTED)),
            Span::raw(task.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".into())),
            Span::raw(if task.timed_out { "   (timed out)" } else { "" }),
        ]),
        Line::from(Span::styled(task.description.clone(), Style::default())),
    ];
    if let Some(summary) = &task.summary {
        header_lines.push(Line::from(Span::styled(format!("summary: {summary}"), Style::default().fg(MUTED))));
    }
    frame.render_widget(Paragraph::new(header_lines).wrap(Wrap { trim: false }), sections[0]);

    let output_block = Block::default().borders(Borders::ALL).title(if task.status == single_protocol::TaskStatus::Running {
        " live output (auto-refreshing) "
    } else {
        " output "
    });
    let output_paragraph = Paragraph::new(output.as_str()).wrap(Wrap { trim: false }).block(output_block);
    frame.render_widget(output_paragraph, sections[1]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("[enter/esc/q] close", Style::default().fg(MUTED)))),
        sections[2],
    );
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
        TaskAddFlow::PickingAgents { description, cwd, agent_names, chosen, cursor, real_home } => {
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
            let (real_home_mark, real_home_color) = if *real_home { ("[x]", WARN) } else { ("[ ]", MUTED) };
            lines.push(Line::from(Span::styled(
                format!("  {real_home_mark} [g] real $HOME (touch your actual system, not the isolated sandbox)"),
                Style::default().fg(real_home_color),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from("[↑↓] move  [space] toggle agent  [g] toggle real $HOME  [enter] run  [esc] cancel"));
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

fn draw_quick_add_modal(frame: &mut Frame, area: Rect, app: &App) {
    let modal_area = centered_rect(65, 40, area);
    frame.render_widget(Clear, modal_area);

    let (title, lines): (String, Vec<Line>) = match &app.quick_add {
        QuickAddFlow::EnteringLine { kind, input } => (
            format!(" Add {} ", kind.label()),
            vec![
                Line::from(Span::styled(kind.format_hint(), Style::default().fg(MUTED))),
                Line::from(""),
                Line::from(format!("> {input}")),
                Line::from(""),
                Line::from("[enter] add  [esc] cancel"),
            ],
        ),
        QuickAddFlow::Submitting { kind, .. } => (format!(" Adding {}… ", kind.label()), vec![Line::from("Submitting...")]),
        QuickAddFlow::Done { kind } => (
            " Added ".to_string(),
            vec![Line::from(Span::styled(format!("{} added", kind.label()), Style::default().fg(OK))), Line::from(""), Line::from("[enter/esc] close")],
        ),
        QuickAddFlow::Failed { kind, error } => (
            " Failed ".to_string(),
            vec![
                Line::from(Span::styled(format!("Could not add {}", kind.label()), Style::default().fg(BAD))),
                Line::from(""),
                Line::from(error.clone()),
                Line::from(""),
                Line::from("[enter/esc] close"),
            ],
        ),
        QuickAddFlow::Idle => (" ".to_string(), vec![]),
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
