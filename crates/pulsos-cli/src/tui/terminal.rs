//! Terminal setup, teardown, and panic recovery.
//!
//! Entering the TUI takes over the terminal (raw mode + alternate screen).
//! These functions ensure the terminal is always restored, even on panic.

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enter raw mode, switch to the alternate screen, and return a ready Terminal.
pub fn setup() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e.into());
    }
    let backend = CrosstermBackend::new(stdout);
    match Terminal::new(backend) {
        Ok(terminal) => Ok(terminal),
        Err(e) => {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
            Err(e.into())
        }
    }
}

/// Leave the alternate screen and disable raw mode.
pub fn teardown(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Install a panic hook that restores the terminal before printing the panic.
///
/// Without this, a panic inside the TUI loop would leave the terminal in raw
/// mode with the alternate screen active, making the error message invisible.
///
/// If a `TuiActiveFlag` is provided, the flag is cleared in the panic hook
/// so that the panic message writes to stderr normally.
pub fn install_panic_hook(tui_active: Option<super::log_buffer::TuiActiveFlag>) {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Re-enable stderr output before restoring terminal.
        if let Some(ref flag) = tui_active {
            flag.set_active(false);
        }
        // Best-effort terminal restoration — ignore errors.
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}
