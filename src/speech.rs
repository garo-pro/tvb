//! Screen reader announcements through Prism (NVDA, JAWS, SAPI fallback...).
//! Lives on the UI thread only.

use std::cell::RefCell;

pub struct Speech {
    // Keep the context alive for as long as the backend is in use.
    _ctx: Option<prism::Context>,
    backend: RefCell<Option<prism::Backend>>,
}

impl Speech {
    /// Picks the best available backend. Speech failing to initialize is not
    /// fatal; the status bar still carries every message.
    pub fn new() -> Self {
        match prism::Context::new() {
            Ok(ctx) => {
                let backend = ctx.acquire_best().ok();
                Self { _ctx: Some(ctx), backend: RefCell::new(backend) }
            }
            Err(_) => Self { _ctx: None, backend: RefCell::new(None) },
        }
    }

    /// Speaks and brailles `text`. `interrupt` cuts off current speech; use it
    /// for errors, not for routine updates that follow a focus change.
    pub fn say(&self, text: &str, interrupt: bool) {
        if let Some(b) = self.backend.borrow_mut().as_mut() {
            let _ = b.output(text, interrupt);
        }
    }
}
