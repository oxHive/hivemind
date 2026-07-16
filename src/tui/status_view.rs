use crate::cli::{StatusData, build_status_data};
use crate::store::SqliteStore;
use crate::tui::{TerminalGuard, header::render_header};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
use std::path::Path;
use std::time::Duration;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const DIM: Color = Color::Rgb(0x8a, 0x8a, 0x9a);

/// Runs the interactive `hivemind status` view: header + a key-value panel
/// that auto-refreshes every 5s, with `r` for an immediate manual refresh.
/// `q` / Ctrl+C exits. Returns once the user quits.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    cwd: &Path,
    global_path: &Path,
    store: &SqliteStore,
    db_path: &str,
    registered_clients: &[&str],
    settings: &crate::config::ServerSettings,
    server_up: bool,
) -> Result<()> {
    let guard = TerminalGuard::enter()?;
    let mut terminal = guard.terminal()?;

    let mut data = build_status_data(
        cwd,
        global_path,
        store,
        db_path,
        registered_clients,
        settings,
        server_up,
    )
    .await?;

    let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
    ticker.tick().await; // first tick fires immediately; consume it, we already fetched above
    let no_color = crate::tui::no_color();

    loop {
        terminal.draw(|frame| draw(&data, no_color, frame))?;

        tokio::select! {
            _ = ticker.tick() => {
                data = build_status_data(
                    cwd, global_path, store, db_path, registered_clients, settings, server_up,
                )
                .await
                .unwrap_or(data);
            }
            key = poll_key_event() => {
                if let Some(key) = key {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char('r') => {
                            data = build_status_data(
                                cwd, global_path, store, db_path, registered_clients, settings, server_up,
                            )
                            .await
                            .unwrap_or(data);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

/// Polls for a key-press event on a blocking thread (crossterm's `poll`/`read`
/// are synchronous) without blocking the tokio runtime. Returns `None` on a
/// 100ms timeout with no event, or a non-press event (e.g. key release).
async fn poll_key_event() -> Option<event::KeyEvent> {
    tokio::task::spawn_blocking(|| {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    return Some(key);
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None)
}

fn draw(data: &StatusData, no_color: bool, frame: &mut ratatui::Frame) {
    let area = frame.area();
    let layout = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(data, no_color, layout[0], frame.buffer_mut());

    let body = Block::default().borders(Borders::ALL).title(" Overview ");
    let inner = body.inner(layout[1]);
    frame.render_widget(body, layout[1]);

    let mut lines = vec![
        Line::from(format!(
            "Server     {}",
            if data.server_up {
                format!(
                    "running at http://{}:{}",
                    data.server_host, data.server_port
                )
            } else {
                "not running".to_string()
            }
        )),
        Line::from(format!("Storage    {}", data.db_path)),
        Line::from(format!(
            "Sync       {}",
            if data.sync_enabled {
                format!("enabled -> {}", data.sync_remote_url)
            } else {
                "disabled (local only)".to_string()
            }
        )),
        Line::from(format!(
            "AI clients {}",
            if data.registered_clients.is_empty() {
                "none registered".to_string()
            } else {
                data.registered_clients.join(", ")
            }
        )),
    ];
    if data.project.is_none() {
        lines.push(Line::from(""));
        lines.push(Line::from(
            "No .hivemind.toml found in this directory tree.",
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    let footer_style = if no_color {
        Style::default()
    } else {
        Style::default().fg(DIM)
    };
    frame.render_widget(
        Paragraph::new(Line::from("q quit   r refresh").style(footer_style)),
        layout[2],
    );
}
