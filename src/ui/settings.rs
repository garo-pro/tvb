//! Settings dialog: update channel, startup update check, download folder.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use wxdragon::prelude::*;

use super::Ctx;
use crate::config::UpdateChannel;

/// Shows the dialog modally and saves the settings if the user presses OK.
pub fn show(ctx: &Rc<Ctx>) {
    let current = ctx.settings.borrow().clone();
    let dlg = Dialog::builder(&ctx.frame, "Settings").build();

    // Each label is created directly before its control so screen readers
    // pick it up as the control's name; creation order is tab order.
    let channel_label = StaticText::builder(&dlg).with_label("&Update channel:").build();
    let channel =
        Choice::builder(&dlg).with_choices(UpdateChannel::ALL.iter().map(|c| c.label().to_string()).collect()).build();
    let selected = UpdateChannel::ALL.iter().position(|c| *c == current.update_channel).unwrap_or(0);
    channel.set_selection(u32::try_from(selected).unwrap_or(0));

    let startup = CheckBox::builder(&dlg)
        .with_label("&Check for updates when TV-Blind starts")
        .with_value(current.check_updates_on_startup)
        .build();

    // Editable on purpose: wxMSW skips read-only single-line fields when
    // tabbing, so screen reader users could not reach or review the path.
    let folder_label = StaticText::builder(&dlg).with_label("&Download folder:").build();
    let original_folder = current.download_dir().to_string_lossy().into_owned();
    let folder = TextCtrl::builder(&dlg).with_value(&original_folder).build();
    let browse = Button::builder(&dlg).with_label("&Browse...").build();

    let ok = Button::builder(&dlg).with_id(ID_OK).with_label("OK").build();
    let cancel = Button::builder(&dlg).with_id(ID_CANCEL).with_label("Cancel").build();
    ok.set_default();
    dlg.set_affirmative_id(ID_OK);
    dlg.set_escape_id(ID_CANCEL);

    browse.on_click(move |_| {
        let picker = DirDialog::builder(&dlg, "Choose the download folder", &folder.get_value()).build();
        if picker.show_modal() == ID_OK
            && let Some(path) = picker.get_path()
        {
            folder.set_value(&path);
        }
        folder.set_focus();
    });

    // Keep the dialog open until the typed folder is usable.
    ok.on_click(move |_| {
        if parse_folder(&folder.get_value()).is_some() {
            dlg.end_modal(ID_OK);
        } else {
            MessageDialog::builder(
                &dlg,
                "Type a full folder path, such as C:\\Users\\Name\\Downloads, or choose one with Browse.",
                "Download folder",
            )
            .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
            .build()
            .show_modal();
            folder.set_focus();
            folder.select_all();
        }
    });

    let folder_row = BoxSizer::builder(Orientation::Horizontal).build();
    folder_row.add(&folder, 1, SizerFlag::Expand | SizerFlag::Right, 6);
    folder_row.add(&browse, 0, SizerFlag::empty(), 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add_stretch_spacer(1);
    buttons.add(&ok, 0, SizerFlag::Right, 6);
    buttons.add(&cancel, 0, SizerFlag::empty(), 0);

    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(&channel_label, 0, SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 10);
    root.add(&channel, 0, SizerFlag::Expand | SizerFlag::All, 10);
    root.add(&startup, 0, SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 10);
    root.add(&folder_label, 0, SizerFlag::Left | SizerFlag::Right, 10);
    root.add_sizer(&folder_row, 0, SizerFlag::Expand | SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand | SizerFlag::All, 10);
    dlg.set_sizer_and_fit(root, true);
    dlg.set_min_size(Size::new(520, -1));
    dlg.centre();

    channel.set_focus();
    let accepted = dlg.show_modal() == ID_OK;
    let new_channel = channel
        .get_selection()
        .and_then(|i| UpdateChannel::ALL.get(usize::try_from(i).ok()?).copied())
        .unwrap_or(current.update_channel);
    let new_startup = startup.is_checked();
    let folder_text = folder.get_value();
    dlg.destroy();
    if !accepted {
        return;
    }

    let mut settings = ctx.settings.borrow_mut();
    settings.update_channel = new_channel;
    settings.check_updates_on_startup = new_startup;
    // Only store a folder the user changed, so the default keeps following
    // the Windows Downloads folder.
    if let Some(dir) = parse_folder(&folder_text)
        && dir != Path::new(&original_folder)
    {
        settings.download_dir = Some(dir);
    }
    match settings.save() {
        Ok(()) => ctx.announce("Settings saved.", false),
        Err(e) => ctx.announce(&format!("Could not save settings: {e}"), true),
    }
}

/// Returns the folder typed in the dialog if it is an absolute path.
/// Surrounding quotes, as left by Explorer's "Copy as path", are removed.
fn parse_folder(text: &str) -> Option<PathBuf> {
    let text = text.trim().trim_matches('"').trim();
    let path = PathBuf::from(text);
    (!text.is_empty() && path.is_absolute()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_absolute_and_quoted_paths() {
        assert_eq!(parse_folder(r" C:\Downloads\Things "), Some(PathBuf::from(r"C:\Downloads\Things")));
        assert_eq!(parse_folder(r#""D:\3D prints""#), Some(PathBuf::from(r"D:\3D prints")));
        assert_eq!(parse_folder(r"\\server\share\stl"), Some(PathBuf::from(r"\\server\share\stl")));
    }

    #[test]
    fn rejects_empty_and_relative_paths() {
        assert_eq!(parse_folder("   "), None);
        assert_eq!(parse_folder(r#""""#), None);
        assert_eq!(parse_folder(r"Downloads\Things"), None);
        assert_eq!(parse_folder(r"\Downloads"), None);
    }
}
