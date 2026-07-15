use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CEvent, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, Tabs, Wrap};
use ratatui::Terminal;
use serde::Deserialize;

use crate::auth::{auth_client, resolve_token, Config};

#[derive(Clone, Debug, Default, Deserialize)]
struct EventRow {
    id: String,
    source: String,
    status: String,
    target_url: String,
    retry_count: i32,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ListEventsResponse {
    events: Vec<EventRow>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Rule {
    name: String,
    source_pattern: String,
    target_url: String,
    active: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Metrics {
    total: i64,
    delivered: i64,
    failed: i64,
    success_rate: f64,
    queue_depth: i64,
    retry_depth: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct FwdConfig {
    default_target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tab {
    Overview,
    Events,
    Rules,
    Config,
    Tokens,
}

impl Tab {
    fn all() -> [Tab; 5] {
        [
            Tab::Overview,
            Tab::Events,
            Tab::Rules,
            Tab::Config,
            Tab::Tokens,
        ]
    }

    fn index(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Events => 1,
            Tab::Rules => 2,
            Tab::Config => 3,
            Tab::Tokens => 4,
        }
    }

    fn from_index(index: usize) -> Self {
        Self::all()[index.min(Self::all().len() - 1)]
    }
}

#[derive(Default)]
struct Data {
    events: Vec<EventRow>,
    rules: Vec<Rule>,
    metrics: Metrics,
    config: FwdConfig,
}

struct App {
    cfg: Config,
    data: Data,
    tab: Tab,
    selected: usize,
    detail: bool,
    search: String,
    editing_search: bool,
    message: String,
    last_refresh: Option<Instant>,
}

impl App {
    fn new(cfg: Config) -> Self {
        Self {
            cfg,
            data: Data::default(),
            tab: Tab::Overview,
            selected: 0,
            detail: false,
            search: String::new(),
            editing_search: false,
            message: "Press r to refresh, q to quit".to_string(),
            last_refresh: None,
        }
    }

    fn filtered_events(&self) -> Vec<EventRow> {
        let needle = self.search.trim().to_lowercase();
        if needle.is_empty() {
            return self.data.events.clone();
        }
        self.data
            .events
            .iter()
            .filter(|event| {
                event.id.to_lowercase().contains(&needle)
                    || event.source.to_lowercase().contains(&needle)
                    || event.status.to_lowercase().contains(&needle)
                    || event.target_url.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }

    async fn refresh(&mut self) {
        let client = auth_client(&self.cfg);
        let events = client
            .get(format!("{}/events?per_page=100", self.cfg.backend))
            .send()
            .await
            .and_then(|r| r.error_for_status());
        match events {
            Ok(resp) => match resp.json::<ListEventsResponse>().await {
                Ok(data) => self.data.events = data.events,
                Err(e) => self.message = format!("Events parse error: {e}"),
            },
            Err(e) => self.message = format!("Events request error: {e}"),
        }

        if let Ok(resp) = client
            .get(format!("{}/rules", self.cfg.backend))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(rules) = resp.json::<Vec<Rule>>().await {
                self.data.rules = rules;
            }
        }

        if let Ok(resp) = client
            .get(format!("{}/stats?range=24h", self.cfg.backend))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(metrics) = resp.json::<Metrics>().await {
                self.data.metrics = metrics;
            }
        }

        if let Ok(resp) = client
            .get(format!("{}/config/default-target", self.cfg.backend))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(config) = resp.json::<FwdConfig>().await {
                self.data.config = config;
            }
        }

        self.selected = self
            .selected
            .min(self.filtered_events().len().saturating_sub(1));
        self.last_refresh = Some(Instant::now());
        if self.message.starts_with("Press") || self.message.contains("request error") {
            self.message = "Refreshed".to_string();
        }
    }

    async fn retry_selected(&mut self) {
        let events = self.filtered_events();
        let Some(event) = events.get(self.selected) else {
            self.message = "No event selected".to_string();
            return;
        };
        let client = auth_client(&self.cfg);
        match client
            .post(format!("{}/events/{}/retry", self.cfg.backend, event.id))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(_) => {
                self.message = format!("Retried {}", short_id(&event.id));
                self.refresh().await;
            }
            Err(e) => self.message = format!("Retry failed: {e}"),
        }
    }
}

pub async fn run(cfg: Config) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(cfg);
    app.refresh().await;
    let result = loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(200))? {
            let CEvent::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.editing_search {
                match key.code {
                    KeyCode::Esc => app.editing_search = false,
                    KeyCode::Enter => app.editing_search = false,
                    KeyCode::Backspace => {
                        app.search.pop();
                        app.selected = 0;
                    }
                    KeyCode::Char(c) => {
                        app.search.push(c);
                        app.selected = 0;
                    }
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') => break Ok(()),
                KeyCode::Char('r') => app.refresh().await,
                KeyCode::Char('x') => app.retry_selected().await,
                KeyCode::Char('o') => {
                    open::that(&app.cfg.web).ok();
                    app.message = format!("Opened {}", app.cfg.web);
                }
                KeyCode::Char('/') => app.editing_search = true,
                KeyCode::Char('c') => {
                    app.search.clear();
                    app.selected = 0;
                }
                KeyCode::Char('b') => app.detail = false,
                KeyCode::Enter => {
                    if app.tab == Tab::Events {
                        app.detail = true;
                    }
                }
                KeyCode::Down => app.selected = app.selected.saturating_add(1),
                KeyCode::Up => app.selected = app.selected.saturating_sub(1),
                KeyCode::Char(n @ '1'..='5') => {
                    let index = n.to_digit(10).unwrap_or(1) as usize - 1;
                    app.tab = Tab::from_index(index);
                    app.detail = false;
                    app.selected = 0;
                }
                _ => {}
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let titles = ["Overview", "Events", "Rules", "Settings", "Tokens"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .block(Block::default().title("trusin").borders(Borders::ALL))
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Overview => draw_overview(frame, app, chunks[1]),
        Tab::Events => {
            if app.detail {
                draw_event_detail(frame, app, chunks[1]);
            } else {
                draw_events(frame, app, chunks[1]);
            }
        }
        Tab::Rules => draw_rules(frame, app, chunks[1]),
        Tab::Config => draw_config(frame, app, chunks[1]),
        Tab::Tokens => draw_tokens(frame, app, chunks[1]),
    }

    let help = format!(
        "1-5 tabs | r refresh | / search | c clear | enter detail | b back | x retry | o open web | q quit    {}",
        app.message
    );
    frame.render_widget(
        Paragraph::new(help).block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

fn draw_overview(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let metrics = &app.data.metrics;
    let auth = if resolve_token(&app.cfg).is_some() {
        "Bearer token"
    } else {
        "Basic fallback"
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("Total events: ", Style::default().fg(Color::Gray)),
            Span::raw(metrics.total.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Delivered: ", Style::default().fg(Color::Gray)),
            Span::raw(metrics.delivered.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Failed: ", Style::default().fg(Color::Gray)),
            Span::raw(metrics.failed.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Success rate: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{:.2}%", metrics.success_rate)),
        ]),
        Line::from(vec![
            Span::styled("Queue depth: ", Style::default().fg(Color::Gray)),
            Span::raw(metrics.queue_depth.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Retry depth: ", Style::default().fg(Color::Gray)),
            Span::raw(metrics.retry_depth.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Auth: ", Style::default().fg(Color::Gray)),
            Span::raw(auth),
        ]),
        Line::from(vec![
            Span::styled("Backend: ", Style::default().fg(Color::Gray)),
            Span::raw(app.cfg.backend.clone()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Overview").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_events(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let events = app.filtered_events();
    let rows = events.iter().enumerate().map(|(index, event)| {
        let style = if index == app.selected {
            Style::default().fg(Color::Black).bg(Color::Green)
        } else {
            Style::default()
        };
        Row::new([
            short_id(&event.id),
            event.status.clone(),
            event.source.clone(),
            event.retry_count.to_string(),
            event.target_url.clone(),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(["ID", "Status", "Source", "Retry", "Target"])
            .style(Style::default().fg(Color::Gray)),
    )
    .block(
        Block::default()
            .title(format!("Events  search: {}", app.search))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_event_detail(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let events = app.filtered_events();
    let Some(event) = events.get(app.selected) else {
        frame.render_widget(Paragraph::new("No event selected"), area);
        return;
    };
    let lines = vec![
        Line::from(format!("ID: {}", event.id)),
        Line::from(format!("Status: {}", event.status)),
        Line::from(format!("Source: {}", event.source)),
        Line::from(format!("Target: {}", event.target_url)),
        Line::from(format!("Retry count: {}", event.retry_count)),
        Line::from(""),
        Line::from("Press x to retry, b to return."),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Event Detail").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_rules(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let rows = app.data.rules.iter().map(|rule| {
        Row::new(vec![
            rule.name.clone(),
            rule.source_pattern.clone(),
            if rule.active { "active" } else { "paused" }.to_string(),
            rule.target_url.clone(),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Min(24),
        ],
    )
    .header(Row::new(["Name", "Source", "State", "Target"]).style(Style::default().fg(Color::Gray)))
    .block(Block::default().title("Rules").borders(Borders::ALL));
    frame.render_widget(table, area);
}

fn draw_config(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let lines = vec![
        Line::from(format!("Backend: {}", app.cfg.backend)),
        Line::from(format!("Web dashboard: {}", app.cfg.web)),
        Line::from(format!(
            "Default target: {}",
            if app.data.config.default_target.is_empty() {
                "-"
            } else {
                &app.data.config.default_target
            }
        )),
        Line::from(format!(
            "API token: {}",
            if resolve_token(&app.cfg).is_some() {
                "configured"
            } else {
                "not configured"
            }
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title("Config").borders(Borders::ALL)),
        area,
    );
}

fn draw_tokens(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let lines = vec![
        Line::from("API keys are created in the web dashboard: Settings > API Tokens."),
        Line::from("Paste one into this machine with: trusin set-token ts_..."),
        Line::from("Precedence: TERUSIN_TOKEN env > OS keychain > config.toml token."),
        Line::from(""),
        Line::from(format!(
            "Active auth mode: {}",
            if resolve_token(&app.cfg).is_some() {
                "Bearer token"
            } else {
                "Basic fallback"
            }
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Tokens").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
