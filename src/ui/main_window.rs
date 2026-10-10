//! Main window: search field, results list, menus.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use wxdragon::prelude::*;
use wxdragon::timer::Timer;

use super::{Ctx, details, prompt_for_token, sign_out, start_sign_in, task};
use crate::api::ApiError;
use crate::config::{self, SearchSort};
use crate::models::{SearchPage, Thing, ThingDetails};

const PER_PAGE: u32 = 30;
const SEARCH_TTL: Duration = Duration::from_mins(10);
const DETAILS_TTL: Duration = Duration::from_hours(24);
/// Wait this long after the sort order changes before searching again, so
/// arrowing through the choices does not start a search for each one.
const SORT_DELAY_MS: i32 = 600;

const ID_TOKEN: i32 = ID_HIGHEST + 1;
const ID_SETTINGS: i32 = ID_HIGHEST + 2;
const ID_OPEN_DOWNLOADS: i32 = ID_HIGHEST + 3;
const ID_CLEAR_CACHE: i32 = ID_HIGHEST + 4;
const ID_EXIT: i32 = ID_HIGHEST + 5;
const ID_FOCUS_SEARCH: i32 = ID_HIGHEST + 6;
const ID_FOCUS_RESULTS: i32 = ID_HIGHEST + 7;
const ID_MORE: i32 = ID_HIGHEST + 8;
const ID_SHORTCUTS: i32 = ID_HIGHEST + 9;
const ID_ABOUT: i32 = ID_HIGHEST + 10;
const ID_SIGN_IN: i32 = ID_HIGHEST + 11;
const ID_SIGN_OUT: i32 = ID_HIGHEST + 12;
const ID_PROJECT_PAGE: i32 = ID_HIGHEST + 13;
const ID_PRIVACY: i32 = ID_HIGHEST + 14;
const ID_THIRD_PARTY: i32 = ID_HIGHEST + 15;
const ID_CHECK_UPDATES: i32 = ID_HIGHEST + 16;

#[derive(Default)]
struct State {
    term: String,
    sort: SearchSort,
    page: u32,
    has_more: bool,
    total: Option<u64>,
    things: Vec<Thing>,
    /// Bumped per new search so late results of an old one are dropped.
    generation: u64,
}

pub struct MainWindow {
    ctx: Rc<Ctx>,
    search: TextCtrl,
    sort: Choice,
    sort_timer: Timer<Frame>,
    results_label: StaticText,
    list: ListCtrl,
    more: Button,
    state: RefCell<State>,
}

const SHORTCUTS: &str = "Main window:
Alt+T or Ctrl+F: search field
Alt+S or Enter in the search field: search
Alt+Y: sort order; changing it sorts the current search again
Alt+R or Ctrl+R: results list
Enter on a result, or Alt+O: open details
Alt+M or Ctrl+M: more results
Ctrl+O: open download folder

Details window:
Alt+D: description
Alt+F: file list; Enter on a file downloads it
Alt+S: download selected files
Alt+A: download all files
Alt+I: download images
Alt+W: open on the Thingiverse website
Escape: close";

impl MainWindow {
    pub fn build(ctx: Rc<Ctx>) -> Rc<Self> {
        let frame = ctx.frame;
        let panel = Panel::builder(&frame).build();
        // Otherwise screen readers may announce the default name "panel".
        panel.set_label("");

        // Creation order is tab order, and each label directly precedes its
        // control so screen readers associate them.
        let search_label = StaticText::builder(&panel).with_label("Search &term:").build();
        let search = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
        let search_btn = Button::builder(&panel).with_label("&Search").build();

        let sort_label = StaticText::builder(&panel).with_label("Sort b&y:").build();
        let sort = Choice::builder(&panel)
            .with_choices(SearchSort::ALL.iter().map(|s| s.label().to_string()).collect())
            .build();
        let saved_sort = ctx.settings.borrow().search_sort;
        let selected = SearchSort::ALL.iter().position(|s| *s == saved_sort).unwrap_or(0);
        sort.set_selection(u32::try_from(selected).unwrap_or(0));

        let results_label = StaticText::builder(&panel).with_label("&Results:").build();
        let list = ListCtrl::builder(&panel).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build();
        list.insert_column(0, "Name", ListColumnFormat::Left, 420);
        list.insert_column(1, "Creator", ListColumnFormat::Left, 220);
        list.insert_column(2, "Likes", ListColumnFormat::Right, 90);

        let open_btn = Button::builder(&panel).with_label("&Open details").build();
        let more = Button::builder(&panel).with_label("&More results").build();
        more.enable(false);

        let top = BoxSizer::builder(Orientation::Horizontal).build();
        top.add(&search_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 6);
        top.add(&search, 1, SizerFlag::Expand | SizerFlag::Right, 6);
        top.add(&search_btn, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 12);
        top.add(&sort_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 6);
        top.add(&sort, 0, SizerFlag::AlignCenterVertical, 0);

        let buttons = BoxSizer::builder(Orientation::Horizontal).build();
        buttons.add(&open_btn, 0, SizerFlag::Right, 6);
        buttons.add(&more, 0, SizerFlag::Right, 6);

        let root = BoxSizer::builder(Orientation::Vertical).build();
        root.add_sizer(&top, 0, SizerFlag::Expand | SizerFlag::All, 8);
        root.add(&results_label, 0, SizerFlag::Left | SizerFlag::Right, 8);
        root.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 8);
        root.add_sizer(&buttons, 0, SizerFlag::All, 8);
        panel.set_sizer(root, true);

        let file_menu = Menu::builder()
            .append_item(ID_SIGN_IN, "&Sign in with browser...", "Sign in to Thingiverse in your web browser")
            .append_item(ID_TOKEN, "Paste API &token...", "Use your own Thingiverse API token")
            .append_item(ID_SIGN_OUT, "Sign o&ut", "Forget the saved token")
            .append_separator()
            .append_item(ID_SETTINGS, "S&ettings...\tCtrl+,", "Update channel, update checks and download folder")
            .append_item(ID_OPEN_DOWNLOADS, "&Open download folder\tCtrl+O", "Open the download folder in Explorer")
            .append_item(ID_CLEAR_CACHE, "C&lear cache", "Delete cached search results and details")
            .append_separator()
            .append_item(ID_EXIT, "E&xit", "Close the program")
            .build();
        let nav_menu = Menu::builder()
            .append_item(ID_FOCUS_SEARCH, "&Search field\tCtrl+F", "Move to the search field")
            .append_item(ID_FOCUS_RESULTS, "&Results list\tCtrl+R", "Move to the results list")
            .append_item(ID_MORE, "&More results\tCtrl+M", "Load the next page of results")
            .build();
        let help_menu = Menu::builder()
            .append_item(ID_SHORTCUTS, "&Keyboard shortcuts\tF1", "List keyboard shortcuts")
            .append_item(ID_PROJECT_PAGE, "&Help and bug reports", "Open the project page on GitHub")
            .append_item(ID_PRIVACY, "&Privacy policy", "Open the privacy policy in your browser")
            .append_item(ID_THIRD_PARTY, "&Third-party licenses", "Licenses of the components TV-Blind is built from")
            .append_item(ID_CHECK_UPDATES, "Check for &updates", "Look for a newer version of TV-Blind")
            .append_item(ID_ABOUT, "&About", "About this program")
            .build();
        frame.set_menu_bar(
            MenuBar::builder().append(file_menu, "&File").append(nav_menu, "&Go").append(help_menu, "&Help").build(),
        );

        let this = Rc::new(Self {
            ctx,
            search,
            sort,
            sort_timer: Timer::new(&frame),
            results_label,
            list,
            more,
            state: RefCell::new(State::default()),
        });

        let w = this.clone();
        search.on_text_enter(move |_| w.start_search());
        let w = this.clone();
        search_btn.on_click(move |_| w.start_search());
        let w = this.clone();
        list.on_item_activated(move |e| w.open_details(e.get_item_index()));
        let w = this.clone();
        open_btn.on_click(move |_| w.open_selected());
        let w = this.clone();
        more.on_click(move |_| w.load_more());
        let w = this.clone();
        sort.on_selection_changed(move |_| w.on_sort_changed());
        let w = this.clone();
        this.sort_timer.on_tick(move |_| w.resort());

        let w = this.clone();
        frame.on_menu(move |e| w.on_menu(e.get_id()));

        this
    }

    pub fn focus_search(&self) {
        self.search.set_focus();
    }

    fn on_menu(self: &Rc<Self>, id: i32) {
        let ctx = &self.ctx;
        match id {
            ID_SIGN_IN => {
                if crate::oauth::client_id().is_some() {
                    start_sign_in(ctx);
                } else {
                    ctx.announce("Browser sign-in is not set up in this build. Paste a token instead.", true);
                }
            }
            ID_TOKEN => {
                prompt_for_token(ctx);
            }
            ID_SIGN_OUT => sign_out(ctx),
            ID_SETTINGS => super::settings::show(ctx),
            ID_CHECK_UPDATES => super::updates::check(ctx, ship_shape::ui::CheckTrigger::Manual),
            ID_OPEN_DOWNLOADS => {
                let dir = ctx.download_root();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = opener::open(&dir) {
                    ctx.announce(&format!("Could not open the download folder: {e}"), true);
                }
            }
            ID_CLEAR_CACHE => match ctx.cache.clear() {
                Ok(()) => ctx.announce("Cache cleared.", false),
                Err(e) => ctx.announce(&format!("Could not clear the cache: {e}"), true),
            },
            ID_EXIT => {
                ctx.frame.close(false);
            }
            ID_FOCUS_SEARCH => self.search.set_focus(),
            ID_FOCUS_RESULTS => self.focus_results(),
            ID_MORE => self.load_more(),
            ID_SHORTCUTS => {
                MessageDialog::builder(&ctx.frame, SHORTCUTS, "Keyboard shortcuts")
                    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
                    .build()
                    .show_modal();
            }
            ID_ABOUT => {
                let msg = format!(
                    "TV-Blind {}\n\
                     A Thingiverse client for blind and visually impaired people, made by {}.\n\
                     Not made or endorsed by Thingiverse.\n\n\
                     Help and bug reports: {}\n\
                     Privacy policy: {}\n\
                     License: {}",
                    env!("CARGO_PKG_VERSION"),
                    config::AUTHOR_URL,
                    config::SUPPORT_URL,
                    config::PRIVACY_URL,
                    config::EULA_URL,
                );
                MessageDialog::builder(&ctx.frame, &msg, "About")
                    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
                    .build()
                    .show_modal();
            }
            ID_PROJECT_PAGE => {
                if let Err(e) = opener::open_browser(config::PROJECT_URL) {
                    ctx.announce(&format!("Could not open the browser: {e}"), true);
                }
            }
            ID_THIRD_PARTY => {
                // Shipped next to the exe in release zips.
                let file = std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(|dir| dir.join("THIRD-PARTY-LICENSES.html")))
                    .filter(|f| f.exists());
                match file.map(opener::open) {
                    Some(Ok(())) => {}
                    Some(Err(e)) => ctx.announce(&format!("Could not open the license list: {e}"), true),
                    None => ctx.announce(
                        "THIRD-PARTY-LICENSES.html was not found next to TV-Blind.exe. It is included in release downloads.",
                        true,
                    ),
                }
            }
            ID_PRIVACY => {
                if let Err(e) = opener::open_browser(config::PRIVACY_URL) {
                    ctx.announce(&format!("Could not open the browser: {e}"), true);
                }
            }
            _ => {}
        }
    }

    fn focus_results(&self) {
        if self.list.get_item_count() == 0 {
            self.ctx.announce("No results.", true);
            return;
        }
        if self.list.get_first_selected_item() < 0 {
            self.select_row(0);
        }
        self.list.set_focus();
    }

    fn select_row(&self, row: i64) {
        let both = ListItemState::Selected | ListItemState::Focused;
        self.list.set_item_state(row, both, both);
        self.list.ensure_visible(row);
    }

    fn selected_sort(&self) -> SearchSort {
        self.sort
            .get_selection()
            .and_then(|i| SearchSort::ALL.get(usize::try_from(i).ok()?).copied())
            .unwrap_or_default()
    }

    fn on_sort_changed(&self) {
        let sort = self.selected_sort();
        {
            let mut settings = self.ctx.settings.borrow_mut();
            if settings.search_sort != sort {
                settings.search_sort = sort;
                if let Err(e) = settings.save() {
                    drop(settings);
                    self.ctx.announce(&format!("Could not save the sort order: {e}"), true);
                }
            }
        }
        if !self.state.borrow().term.is_empty() {
            self.sort_timer.start(SORT_DELAY_MS, true);
        }
    }

    /// Runs the last search again in the newly chosen order.
    fn resort(self: &Rc<Self>) {
        let (term, sort) = {
            let s = self.state.borrow();
            (s.term.clone(), s.sort)
        };
        if !term.is_empty() && sort != self.selected_sort() {
            self.run_search(term);
        }
    }

    fn start_search(self: &Rc<Self>) {
        let term = self.search.get_value().trim().to_string();
        if term.is_empty() {
            self.ctx.announce("Type something to search for.", true);
            return;
        }
        self.sort_timer.stop();
        self.run_search(term);
    }

    fn run_search(self: &Rc<Self>, term: String) {
        {
            let mut s = self.state.borrow_mut();
            s.generation += 1;
            s.term.clone_from(&term);
            s.sort = self.selected_sort();
            s.page = 0;
            s.things.clear();
            s.has_more = false;
            s.total = None;
        }
        self.list.delete_all_items();
        self.results_label.set_label("&Results:");
        self.more.enable(false);
        let sort = self.state.borrow().sort;
        self.fetch_page(term, sort, 1);
    }

    fn load_more(self: &Rc<Self>) {
        let (term, sort, page, has_more) = {
            let s = self.state.borrow();
            (s.term.clone(), s.sort, s.page + 1, s.has_more)
        };
        if !has_more {
            self.ctx.announce("No more results.", true);
            return;
        }
        self.fetch_page(term, sort, page);
    }

    fn fetch_page(self: &Rc<Self>, term: String, sort: SearchSort, page: u32) {
        let Some(client) = self.ctx.client() else { return };
        let generation = self.state.borrow().generation;
        self.ctx.announce(
            &match (page, sort) {
                (1, SearchSort::Relevant) => format!("Searching for {term}..."),
                (1, _) => format!("Searching for {term}, sorted by {}...", sort.label().to_lowercase()),
                _ => format!("Loading page {page}..."),
            },
            false,
        );
        let cache = self.ctx.cache.clone();
        let key = format!("{}-{}-p{page}-n{PER_PAGE}", term.to_lowercase(), sort.api_value());
        let w = self.clone();
        task::background(
            move || {
                if let Some(hit) = cache.get::<SearchPage>("search", &key, SEARCH_TTL) {
                    return Ok(hit);
                }
                let r = client.search_things(&term, sort, page, PER_PAGE);
                if let Ok(p) = &r {
                    cache.put("search", &key, p);
                }
                r
            },
            move |r: Result<SearchPage, ApiError>| w.on_page(generation, r),
        );
    }

    fn on_page(&self, generation: u64, r: Result<SearchPage, ApiError>) {
        let mut s = self.state.borrow_mut();
        if generation != s.generation {
            return; // superseded by a newer search
        }
        let page = match r {
            Ok(p) => p,
            Err(e) => {
                drop(s);
                self.ctx.error("Search failed", &e);
                return;
            }
        };
        let first_new = s.things.len();
        for (row, t) in (super::list_row(first_new)..).zip(&page.hits) {
            self.list.insert_item(row, &t.title(), None);
            self.list.set_item_text_by_column(row, 1, &t.creator_name());
            let likes = t.like_count.map(|n| n.to_string()).unwrap_or_default();
            self.list.set_item_text_by_column(row, 2, &likes);
        }
        s.page = page.page;
        s.has_more = page.has_more() && !page.hits.is_empty();
        s.total = page.total.or(s.total);
        s.things.extend(page.hits);
        let shown = s.things.len();
        self.more.enable(s.has_more);

        let count = match s.total {
            Some(t) if usize::try_from(t).is_ok_and(|t| t > shown) => format!("{shown} of {t}"),
            _ => shown.to_string(),
        };
        self.results_label.set_label(&format!("&Results ({count}):"));

        let msg = if shown == 0 {
            format!("No results for {}.", s.term)
        } else if page.page == 1 {
            format!("{count} results.")
        } else if first_new == shown {
            "No more results.".to_string()
        } else {
            format!("{} more loaded, {count} results.", shown - first_new)
        };
        drop(s);
        self.ctx.announce(&msg, false);

        // After "more results", put the user on the first new row.
        if page.page > 1 && first_new < shown && self.list.has_focus() {
            self.select_row(super::list_row(first_new));
        } else if page.page == 1 && shown > 0 {
            self.select_row(0);
        }
    }

    fn open_selected(self: &Rc<Self>) {
        let row = self.list.get_first_selected_item();
        if row < 0 {
            self.ctx.announce("Select a result first.", true);
            return;
        }
        self.open_details(row);
    }

    fn open_details(self: &Rc<Self>, row: i32) {
        let Some(thing) = super::row_index(row).and_then(|i| self.state.borrow().things.get(i).cloned()) else {
            return;
        };
        let Some(id) = thing.id else {
            self.ctx.announce("This result has no id; cannot load details.", true);
            return;
        };
        let Some(client) = self.ctx.client() else { return };
        self.ctx.announce(&format!("Loading {}...", thing.title()), false);
        let cache = self.ctx.cache.clone();
        let w = self.clone();
        task::background(
            move || {
                let key = id.to_string();
                if let Some(hit) = cache.get::<ThingDetails>("things", &key, DETAILS_TTL) {
                    return Ok(hit);
                }
                let r = client.details(id);
                if let Ok(d) = &r {
                    cache.put("things", &key, d);
                }
                r
            },
            move |r: Result<ThingDetails, ApiError>| match r {
                Ok(d) => {
                    w.ctx.status(&format!("{} loaded.", d.thing.title()));
                    details::show(&w.ctx, d);
                    w.list.set_focus();
                }
                Err(e) => w.ctx.error("Could not load details", &e),
            },
        );
    }
}
