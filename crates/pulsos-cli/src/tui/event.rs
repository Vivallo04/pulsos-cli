//! TUI event types — the messages flowing through the event loop.

use crate::tui::actions::ActionResult;
use crossterm::event::KeyEvent;

/// Events processed by the main TUI loop.
pub enum AppEvent {
    /// A keyboard event from the terminal.
    Key(KeyEvent),
    /// Periodic tick for UI refresh (driven by `TuiConfig::fps`).
    Tick,
    /// Terminal was resized.
    Resize(u16, u16),
    /// Asynchronous Settings/Auth action result.
    ActionResult(Box<ActionResult>),
}
