//! Terminal event handling

use crossterm::event::{self, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Events that can occur in the TUI
#[derive(Debug, Clone)]
pub enum Event {
    /// Key press
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for animations/updates
    Tick,
    /// Tuple space event
    TupleEvent(String),
    /// Command output ready
    OutputReady(String),
    /// Command completed
    CommandComplete(i32),
}

/// Event handler that polls for terminal events
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Spawn event polling task
        tokio::spawn(async move {
            loop {
                // Poll for terminal events
                if event::poll(tick_rate).unwrap_or(false) {
                    let ev = match event::read() {
                        Ok(event::Event::Key(key)) => Event::Key(key),
                        Ok(event::Event::Mouse(mouse)) => Event::Mouse(mouse),
                        Ok(event::Event::Resize(w, h)) => Event::Resize(w, h),
                        _ => continue,
                    };
                    if event_tx.send(ev).is_err() {
                        break;
                    }
                } else if event_tx.send(Event::Tick).is_err() {
                    // Tick when there's no input
                    break;
                }
            }
        });

        Self { rx, _tx: tx }
    }

    /// Get the sender for external events (tuple space, output, etc)
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self._tx.clone()
    }

    /// Receive the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}
