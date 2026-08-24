//! Terminal event plumbing for the TUI.

use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use tokio::sync::mpsc;

/// Re-exported so front-ends and tests can build key events without depending
/// on crossterm directly.
pub use crossterm::event::{KeyCode, KeyEvent as Key, KeyModifiers};

/// Events consumed by the TUI loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize,
    /// Periodic refresh tick.
    Tick,
}

/// Merges terminal input and refresh ticks into one async stream.
pub struct EventStream {
    receiver: mpsc::Receiver<AppEvent>,
    _input: std::thread::JoinHandle<()>,
    ticker: tokio::time::Interval,
}

impl EventStream {
    /// Start reading terminal events and emitting ticks every `tick_rate`.
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel(32);

        let input_sender = sender.clone();
        let input = std::thread::spawn(move || {
            loop {
                match event::poll(Duration::from_millis(150)) {
                    Ok(true) => match event::read() {
                        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                            if input_sender.blocking_send(AppEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Resize(_, _)) => {
                            if input_sender.blocking_send(AppEvent::Resize).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    },
                    Ok(false) => {
                        if input_sender.is_closed() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        drop(sender);
        let mut ticker = tokio::time::interval(tick_rate);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.reset();

        EventStream {
            receiver,
            _input: input,
            ticker,
        }
    }

    /// Await the next event.
    pub async fn next(&mut self) -> Option<AppEvent> {
        tokio::select! {
            biased;
            event = self.receiver.recv() => event,
            _ = self.ticker.tick() => Some(AppEvent::Tick),
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.receiver.close();
    }
}
