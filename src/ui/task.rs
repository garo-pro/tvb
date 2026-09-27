//! Runs blocking work on worker threads and hands results back to the UI thread.
//!
//! wxDragon widgets are not `Send`, so completion handlers can't travel to the
//! worker. Instead each handler waits in a UI-thread-local table under an id;
//! the worker only sends the id plus its (`Send`) result through `call_after`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::panic::{self, AssertUnwindSafe};
use std::thread;

type Payload = Box<dyn Any + Send>;
type Handler = Box<dyn FnMut(Payload) -> bool>;
type Reporter = Box<dyn Fn(&str)>;

thread_local! {
    static HANDLERS: RefCell<HashMap<u64, Handler>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static PANIC_REPORTER: RefCell<Option<Reporter>> = const { RefCell::new(None) };
}

/// Sets what happens when a background task panics (on the UI thread).
pub fn set_panic_reporter(f: impl Fn(&str) + 'static) {
    PANIC_REPORTER.with(|r| *r.borrow_mut() = Some(Box::new(f)));
}

/// Drops the task's handler and reports the panic instead of leaving the UI
/// waiting for a result that will never come.
fn post_panic(id: u64, msg: String) {
    wxdragon::call_after(Box::new(move || {
        HANDLERS.with(|h| h.borrow_mut().remove(&id));
        PANIC_REPORTER.with(|r| {
            if let Some(report) = r.borrow().as_ref() {
                report(&msg);
            }
        });
    }));
    wxdragon::wake_up_idle();
}

fn panic_message(e: &(dyn Any + Send)) -> String {
    e.downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| e.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".to_string())
}

fn register(handler: Handler) -> u64 {
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    HANDLERS.with(|h| h.borrow_mut().insert(id, handler));
    id
}

/// Queues `payload` for handler `id` on the UI thread. Safe from any thread.
fn post(id: u64, payload: Payload) {
    wxdragon::call_after(Box::new(move || {
        // Take the handler out while it runs so it may itself start new tasks.
        let Some(mut handler) = HANDLERS.with(|h| h.borrow_mut().remove(&id)) else { return };
        let keep = handler(payload);
        if keep {
            HANDLERS.with(|h| h.borrow_mut().insert(id, handler));
        }
    }));
    wxdragon::wake_up_idle();
}

/// Runs `work` on a new thread and calls `done` with its result on the UI thread.
pub fn background<T, W, D>(work: W, done: D)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let mut done = Some(done);
    let id = register(Box::new(move |p| {
        if let (Some(done), Ok(v)) = (done.take(), p.downcast::<T>()) {
            done(*v);
        }
        false
    }));
    thread::spawn(move || match panic::catch_unwind(AssertUnwindSafe(work)) {
        Ok(v) => post(id, Box::new(v)),
        Err(e) => post_panic(id, panic_message(e.as_ref())),
    });
}

/// A `Send` handle a worker uses to deliver repeated updates (e.g. progress)
/// to a UI-thread callback. The callback is released when the sender drops.
pub struct UiSender<T> {
    id: u64,
    _t: PhantomData<fn(T)>,
}

pub fn ui_channel<T: Send + 'static>(mut on_msg: impl FnMut(T) + 'static) -> UiSender<T> {
    let id = register(Box::new(move |p| {
        if let Ok(msg) = p.downcast::<T>() {
            on_msg(*msg);
        }
        true
    }));
    UiSender { id, _t: PhantomData }
}

impl<T: Send + 'static> UiSender<T> {
    pub fn send(&self, msg: T) {
        post(self.id, Box::new(msg));
    }
}

impl<T> Drop for UiSender<T> {
    fn drop(&mut self) {
        let id = self.id;
        // `call_after` is FIFO, so this runs after every message already sent.
        wxdragon::call_after(Box::new(move || {
            HANDLERS.with(|h| h.borrow_mut().remove(&id));
        }));
        wxdragon::wake_up_idle();
    }
}
