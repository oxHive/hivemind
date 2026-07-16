use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::IsTerminal as _;
use std::io::{Stdout, stdout};
use std::sync::Once;

pub mod header;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

/// True when stdout is a real terminal and the caller did not pass `--plain`.
/// `NO_COLOR` does not affect this: it only strips styling inside the TUI.
pub fn is_interactive(plain: bool) -> bool {
    !plain && stdout().is_terminal()
}

/// True when the `NO_COLOR` env var is set (any value, per the convention at
/// https://no-color.org). Widgets check this to skip foreground colors while
/// still rendering the TUI layout, borders, and text.
pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

static PANIC_HOOK_INSTALLED: Once = Once::new();

fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            default_hook(info);
        }));
    });
}

/// Owns the alt-screen/raw-mode terminal state. Restores the terminal on
/// Drop (normal exit) and via a panic hook (abnormal exit), so a bug in
/// rendering never leaves the user's shell in raw/alt-screen state.
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        if let Err(e) = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(e.into());
        }
        Ok(TerminalGuard)
    }

    pub fn terminal(&self) -> Result<Term> {
        Ok(Terminal::new(CrosstermBackend::new(stdout()))?)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_flag_forces_non_interactive() {
        assert!(!is_interactive(true));
    }
}
