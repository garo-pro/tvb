//! Details dialog: description, file list, download and web actions.

use std::fmt::Write as _;
use std::rc::Rc;

use wxdragon::prelude::*;

use super::{Ctx, start_download, thing_folder};
use crate::models::{Thing, ThingDetails, ThingFile};

/// Converts the description to plain text: HTML is rendered without link
/// brackets, footnotes or other markup so a screen reader reads clean prose.
pub fn clean_text(html: Option<&str>, plain: Option<&str>) -> String {
    let rendered = html.filter(|h| !h.trim().is_empty()).and_then(|h| {
        html2text::config::with_decorator(html2text::render::TrivialDecorator::new())
            .string_from_read(h.as_bytes(), 10_000)
            .ok()
    });
    if let Some(t) = rendered {
        return tidy(&t);
    }
    plain.map(tidy).unwrap_or_default()
}

/// Trims trailing spaces and collapses runs of blank lines.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
    }
    out
}

fn summary(d: &ThingDetails) -> String {
    let t = &d.thing;
    let mut s = format!("{}\nBy {}\n", t.title(), t.creator_name());
    let mut facts = Vec::new();
    if let Some(n) = t.like_count {
        facts.push(format!("{n} likes"));
    }
    if let Some(n) = t.download_count {
        facts.push(format!("{n} downloads"));
    }
    if let Some(n) = t.make_count {
        facts.push(format!("{n} makes"));
    }
    facts.push(format!("{} files", d.files.len()));
    facts.push(format!("{} images", d.images.len()));
    s.push_str(&facts.join(", "));
    s.push('\n');
    if let Some(date) = t.added.as_deref().and_then(|a| a.get(..10)) {
        let _ = writeln!(s, "Published {date}");
    }
    if let Some(l) = &t.license {
        let _ = writeln!(s, "License: {l}");
    }

    let desc = clean_text(t.description_html.as_deref(), t.description.as_deref());
    s.push_str("\nDescription\n\n");
    s.push_str(if desc.is_empty() { "No description." } else { &desc });

    let instr = clean_text(t.instructions_html.as_deref(), t.instructions.as_deref());
    if !instr.is_empty() {
        s.push_str("\n\nInstructions\n\n");
        s.push_str(&instr);
    }
    s
}

fn image_files(d: &ThingDetails) -> Vec<ThingFile> {
    d.images
        .iter()
        .enumerate()
        .filter_map(|(i, img)| {
            let url = img.best_url()?;
            let name = img.name.clone().unwrap_or_else(|| format!("image-{}.jpg", i + 1));
            Some(ThingFile { name: Some(name), direct_url: Some(url), ..Default::default() })
        })
        .collect()
}

/// Report list of files (name, size), first row selected. Multiple selection
/// is allowed for "Download selected".
fn file_list(parent: Dialog, files: &[ThingFile]) -> ListCtrl {
    let list = ListCtrl::builder(&parent).with_style(ListCtrlStyle::Report).build();
    list.insert_column(0, "Name", ListColumnFormat::Left, 480);
    list.insert_column(1, "Size", ListColumnFormat::Right, 120);
    for (row, f) in (0i64..).zip(files) {
        list.insert_item(row, &f.display_name(), None);
        list.set_item_text_by_column(row, 1, &f.size_text());
    }
    if !files.is_empty() {
        let both = ListItemState::Selected | ListItemState::Focused;
        list.set_item_state(0, both, both);
    }
    list
}

/// Shows the details dialog modally.
pub fn show(ctx: &Rc<Ctx>, d: ThingDetails) {
    let thing: Thing = d.thing.clone();
    let title = format!("{} by {}", thing.title(), thing.creator_name());
    let dlg = Dialog::builder(&ctx.frame, &title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder | DialogStyle::MaximizeBox)
        .with_size(800, 600)
        .build();

    let desc_label = StaticText::builder(&dlg).with_label("&Description:").build();
    let desc = TextCtrl::builder(&dlg)
        .with_value(&summary(&d))
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::WordWrap)
        .build();

    let files_label = StaticText::builder(&dlg).with_label(&format!("&Files ({}):", d.files.len())).build();
    let files = file_list(dlg, &d.files);

    let dl_selected = Button::builder(&dlg).with_label("Download &selected").build();
    let dl_all = Button::builder(&dlg).with_label("Download &all").build();
    let dl_images = Button::builder(&dlg).with_label("Download &images").build();
    let web = Button::builder(&dlg).with_label("Open on &web").build();
    let close = Button::builder(&dlg).with_id(ID_CANCEL).with_label("&Close").build();
    dl_selected.enable(!d.files.is_empty());
    dl_all.enable(!d.files.is_empty());
    dl_images.enable(!d.images.is_empty());
    web.enable(thing.web_url().is_some());

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    for b in [&dl_selected, &dl_all, &dl_images, &web] {
        buttons.add(b, 0, SizerFlag::Right, 6);
    }
    buttons.add_stretch_spacer(1);
    buttons.add(&close, 0, SizerFlag::empty(), 0);

    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(&desc_label, 0, SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
    root.add(&desc, 3, SizerFlag::Expand | SizerFlag::All, 8);
    root.add(&files_label, 0, SizerFlag::Left | SizerFlag::Right, 8);
    root.add(&files, 2, SizerFlag::Expand | SizerFlag::All, 8);
    root.add_sizer(&buttons, 0, SizerFlag::Expand | SizerFlag::All, 8);
    dlg.set_sizer(root, true);
    dlg.set_escape_id(ID_CANCEL);

    let d = Rc::new(d);
    let folder = thing_folder(&ctx.download_root(), &thing);

    let selected_files = {
        let d = d.clone();
        move || -> Vec<ThingFile> {
            let mut out = Vec::new();
            let mut i = -1i64;
            loop {
                i = i64::from(files.get_next_item(i, ListNextItemFlag::All, ListItemState::Selected));
                let Some(index) = super::row_index(i) else { break };
                out.extend(d.files.get(index).cloned());
            }
            out
        }
    };
    {
        let (ctx, folder) = (ctx.clone(), folder.clone());
        dl_selected.on_click(move |_| {
            let chosen = selected_files();
            if chosen.is_empty() {
                ctx.announce("No files selected.", true);
            } else {
                start_download(&ctx, folder.clone(), chosen, "files");
            }
        });
    }
    {
        // Enter on a file downloads just that file.
        let (ctx, folder, d) = (ctx.clone(), folder.clone(), d.clone());
        files.on_item_activated(move |e| {
            if let Some(f) = super::row_index(e.get_item_index()).and_then(|i| d.files.get(i)) {
                start_download(&ctx, folder.clone(), vec![f.clone()], "file");
            }
        });
    }
    {
        let (ctx, folder, d) = (ctx.clone(), folder.clone(), d.clone());
        dl_all.on_click(move |_| start_download(&ctx, folder.clone(), d.files.clone(), "files"));
    }
    {
        let (ctx, folder, d) = (ctx.clone(), folder.clone(), d.clone());
        dl_images.on_click(move |_| start_download(&ctx, folder.join("images"), image_files(&d), "images"));
    }
    {
        let ctx = ctx.clone();
        web.on_click(move |_| {
            if let Some(url) = thing.web_url() {
                match opener::open_browser(&url) {
                    Ok(()) => ctx.status("Opened in the web browser."),
                    Err(e) => ctx.announce(&format!("Could not open the browser: {e}"), true),
                }
            }
        });
    }

    desc.set_insertion_point(0);
    desc.set_focus();
    dlg.show_modal();
    dlg.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_html_description() {
        let html = "<p>Hello <b>world</b></p><p></p><p>See <a href='https://x'>this link</a>.</p><ul><li>one</li></ul>";
        let t = clean_text(Some(html), None);
        assert!(t.starts_with("Hello world"), "{t}");
        assert!(t.contains("See this link."), "{t}");
        assert!(!t.contains("https://x"), "{t}");
        assert!(!t.contains("\n\n\n"), "{t}");
    }

    #[test]
    fn falls_back_to_plain() {
        assert_eq!(clean_text(Some("  "), Some("plain  \n\n\n\nnext")), "plain\n\nnext");
        assert_eq!(clean_text(None, None), "");
    }
}
