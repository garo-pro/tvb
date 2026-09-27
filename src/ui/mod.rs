//! User interface. Everything here runs on the UI thread; network and disk
//! work goes through [`task`] so the window never blocks.

mod details;
mod main_window;
mod settings;
mod task;
mod updates;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use wxdragon::prelude::*;

use crate::api::{self, Client};
use crate::cache::Cache;
use crate::config::{self, Settings};
use crate::models::{Thing, ThingFile};
use crate::oauth::{self, SignInError, SignedIn};
use crate::speech::Speech;

/// State shared by every window.
pub struct Ctx {
    pub frame: Frame,
    pub status: StatusBar,
    pub speech: Speech,
    pub cache: Cache,
    pub client: RefCell<Option<Client>>,
    pub settings: RefCell<Settings>,
    /// Set to cancel a browser sign-in that is waiting for its reply.
    pub sign_in_cancel: RefCell<Option<Arc<AtomicBool>>>,
}

impl Ctx {
    /// Shows `msg` in the status bar and sends it to the screen reader.
    pub fn announce(&self, msg: &str, interrupt: bool) {
        self.status.set_status_text(msg, 0);
        self.speech.say(msg, interrupt);
    }

    /// Status bar only, for frequent updates such as download progress.
    pub fn status(&self, msg: &str) {
        self.status.set_status_text(msg, 0);
    }

    pub fn error(&self, what: &str, err: &api::ApiError) {
        self.announce(&format!("{what}: {err}"), true);
    }

    /// A clone of the API client. If nobody is signed in, offers to sign in
    /// and returns `None` unless a token was pasted on the spot.
    pub fn client(self: &Rc<Self>) -> Option<Client> {
        if self.client.borrow().is_none() {
            offer_sign_in(self);
        }
        self.client.borrow().clone()
    }

    pub fn download_root(&self) -> PathBuf {
        self.settings.borrow().download_dir()
    }
}

fn store_token(ctx: &Ctx, token: String) {
    if let Err(e) = config::save_token(&token) {
        // Still usable for this session.
        ctx.announce(&format!("Could not save the token to Credential Manager: {e}"), true);
    }
    *ctx.client.borrow_mut() = Some(Client::new(token));
}

/// Explains that sign-in is needed and lets the user pick browser sign-in or
/// token pasting. Falls straight through to pasting when this build has no
/// client ID.
pub fn offer_sign_in(ctx: &Rc<Ctx>) {
    if oauth::client_id().is_none() {
        prompt_for_token(ctx);
        return;
    }
    let dlg = MessageDialog::builder(
        &ctx.frame,
        "Sign in to Thingiverse to search and download.\n\n\
         Sign in with browser opens Thingiverse in your web browser, where you approve access. \
         Paste a token is for people who have their own API token.",
        "Sign in",
    )
    .with_style(MessageDialogStyle::YesNo | MessageDialogStyle::Cancel | MessageDialogStyle::IconInformation)
    .build();
    dlg.set_yes_no_labels("&Sign in with browser", "&Paste a token");
    match dlg.show_modal() {
        ID_YES => start_sign_in(ctx),
        ID_NO => {
            prompt_for_token(ctx);
        }
        _ => ctx.announce("Not signed in. Use the File menu to sign in.", false),
    }
}

/// Browser sign-in on a worker thread; waits up to five minutes for approval.
pub fn start_sign_in(ctx: &Rc<Ctx>) {
    // Only one attempt at a time: a new one cancels the previous.
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = ctx.sign_in_cancel.borrow_mut().replace(cancel.clone()) {
        old.store(true, Ordering::SeqCst);
    }
    ctx.announce("Opening Thingiverse in your browser. Approve access there, then come back to this window.", false);
    let ctx2 = ctx.clone();
    task::background(
        move || oauth::sign_in(&cancel, Duration::from_mins(5)),
        move |r: Result<SignedIn, SignInError>| match r {
            Ok(signed_in) => {
                *ctx2.sign_in_cancel.borrow_mut() = None;
                store_token(&ctx2, signed_in.token);
                ctx2.frame.raise();
                let who = signed_in.user_name.map(|n| format!(" as {n}")).unwrap_or_default();
                ctx2.announce(&format!("Signed in{who}. You can search now."), true);
            }
            Err(SignInError::Canceled) => {}
            Err(e) => {
                *ctx2.sign_in_cancel.borrow_mut() = None;
                ctx2.frame.raise();
                ctx2.announce(&format!("Sign-in failed: {e}"), true);
            }
        },
    );
}

pub fn sign_out(ctx: &Ctx) {
    if let Some(c) = ctx.sign_in_cancel.borrow_mut().take() {
        c.store(true, Ordering::SeqCst);
    }
    *ctx.client.borrow_mut() = None;
    match config::delete_token() {
        Ok(()) => ctx.announce("Signed out. The token was removed from Credential Manager.", false),
        Err(e) => ctx.announce(&format!("Signed out, but the saved token could not be removed: {e}"), true),
    }
}

/// Asks for an API token and stores it. Returns whether one was saved.
pub fn prompt_for_token(ctx: &Ctx) -> bool {
    // The last line doubles as the edit field's accessible name.
    let message = "Paste an API token from one of your apps at thingiverse.com/apps. \
        It is stored in Windows Credential Manager.\n\nAPI token:";
    let dlg = TextEntryDialog::builder(&ctx.frame, message, "Thingiverse API token").build();
    if dlg.show_modal() != ID_OK {
        return false;
    }
    let token = dlg.get_value().unwrap_or_default().trim().to_string();
    if token.is_empty() {
        ctx.announce("No token entered.", true);
        return false;
    }
    store_token(ctx, token);
    ctx.announce("API token saved.", false);
    true
}

/// List control row for a vector index (lists never get near `i64::MAX`).
pub fn list_row(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

/// Vector index for a list control row; `None` for "no row" (-1).
pub fn row_index(row: impl TryInto<usize>) -> Option<usize> {
    row.try_into().ok()
}

/// Folder for a thing's downloads: `<root>/<name> (<id>)`.
pub fn thing_folder(root: &Path, thing: &Thing) -> PathBuf {
    let name = match thing.id {
        Some(id) => format!("{} ({id})", thing.title()),
        None => thing.title(),
    };
    root.join(api::safe_file_name(&name))
}

enum DownloadMsg {
    Started { index: usize, count: usize, name: String },
    Progress { index: usize, count: usize, name: String, percent: u64 },
}

/// Downloads `files` into `folder` on a worker thread, reporting progress in
/// the status bar and announcing the outcome.
pub fn start_download(ctx: &Rc<Ctx>, folder: PathBuf, files: Vec<ThingFile>, what: &str) {
    if files.is_empty() {
        ctx.announce("Nothing to download.", true);
        return;
    }
    let Some(client) = ctx.client() else { return };
    let count = files.len();
    ctx.announce(&format!("Downloading {count} {what}."), false);

    let progress_ctx = ctx.clone();
    let tx = task::ui_channel(move |m: DownloadMsg| match m {
        DownloadMsg::Started { index, count, name } => {
            progress_ctx.status(&format!("Downloading {} of {count}: {name}", index + 1));
        }
        DownloadMsg::Progress { index, count, name, percent } => {
            progress_ctx.status(&format!("Downloading {} of {count}: {name}, {percent}%", index + 1));
        }
    });

    let done_ctx = ctx.clone();
    let folder_for_msg = folder.clone();
    task::background(
        move || {
            let mut ok = 0usize;
            let mut failures = Vec::new();
            for (index, file) in files.iter().enumerate() {
                let name = file.display_name();
                tx.send(DownloadMsg::Started { index, count, name: name.clone() });
                let mut last = 0u64;
                let r = client.download_file(file, &folder, |done, total| {
                    if let Some(total) = total.filter(|t| *t > 0) {
                        let percent = (done * 100 / total).min(100);
                        // Throttle: one update per 10%.
                        if percent >= last + 10 {
                            last = percent;
                            tx.send(DownloadMsg::Progress { index, count, name: name.clone(), percent });
                        }
                    }
                });
                match r {
                    Ok(_) => ok += 1,
                    Err(e) => failures.push(format!("{name}: {e}")),
                }
            }
            drop(tx);
            (ok, failures)
        },
        move |(ok, failures): (usize, Vec<String>)| {
            let folder_name = folder_for_msg
                .file_name()
                .map_or_else(|| folder_for_msg.display().to_string(), |n| n.to_string_lossy().into_owned());
            if failures.is_empty() {
                let noun = if ok == 1 { "file" } else { "files" };
                done_ctx.announce(&format!("Download complete: {ok} {noun} saved to {folder_name}."), false);
            } else {
                done_ctx.announce(
                    &format!("{ok} of {} downloaded. Failed: {}", ok + failures.len(), failures.join("; ")),
                    true,
                );
            }
        },
    );
}

pub fn run() {
    let _ = main(|_| {
        let frame = Frame::builder().with_title("TV-Blind").with_size(Size::new(900, 650)).build();
        let status = frame.create_status_bar(1, 0, Id::try_from(ID_ANY).unwrap_or(-1), "statusBar");

        let token = config::load_token();
        let ctx = Rc::new(Ctx {
            frame,
            status,
            speech: Speech::new(),
            cache: Cache::open(),
            client: RefCell::new(token.map(Client::new)),
            settings: RefCell::new(Settings::load()),
            sign_in_cancel: RefCell::new(None),
        });

        let cache = ctx.cache.clone();
        task::background(move || cache.prune(config::CACHE_MAX_AGE), |_| {});

        let reporter = ctx.clone();
        task::set_panic_reporter(move |msg| reporter.announce(&format!("Internal error: {msg}"), true));

        let main = main_window::MainWindow::build(ctx.clone());
        frame.centre();
        frame.show(true);
        main.focus_search();

        if ctx.client.borrow().is_none() {
            offer_sign_in(&ctx);
        } else {
            ctx.status("Ready. Type a search term and press Enter.");
        }
        updates::check_on_startup(&ctx);
    });
}
