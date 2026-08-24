//! Interactive terminal UI.

pub mod app;
pub mod event;
pub mod ui;
pub mod visibility;

use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;
use crate::error::{Error, Result};
use crate::tui::app::{Mode, TuiApp};
use crate::tui::event::{AppEvent, EventStream};
use crate::tui::visibility::DeviceVisibility;

/// Restores the terminal when dropped, including on panic and on error paths.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        install_panic_hook();
        enable_raw_mode().map_err(io_error)?;
        let guard = TerminalGuard;
        execute!(io::stdout(), EnterAlternateScreen, crossterm::cursor::Hide).map_err(io_error)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

fn install_panic_hook() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous(info);
        }));
    });
}

fn io_error(err: io::Error) -> Error {
    Error::Other(format!("terminal error: {err}"))
}

/// Run the interactive UI until the user quits.
pub async fn run(application: App) -> Result<()> {
    let refresh_interval = application.config().refresh_interval();
    let visibility = DeviceVisibility::load()?;
    let mut tui = TuiApp::with_visibility(application, visibility);

    let _guard = TerminalGuard::enter()?;
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(io_error)?;
    terminal.clear().map_err(io_error)?;

    tui.refresh().await;

    let mut events = EventStream::new(refresh_interval);
    loop {
        terminal
            .draw(|frame| ui::draw(frame, &tui))
            .map_err(io_error)?;

        if tui.should_quit {
            break;
        }

        match events.next().await {
            Some(AppEvent::Key(key)) => tui.handle_key(key).await,
            Some(AppEvent::Tick) => {
                if tui.mode == Mode::Channels {
                    tui.refresh().await;
                }
            }
            Some(AppEvent::Resize) => {}
            None => break,
        }
    }

    Ok(())
}
