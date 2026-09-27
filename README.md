# TV-Blind

TV-Blind is a Windows app for blind and visually impaired people that lets you search [Thingiverse](https://www.thingiverse.com), read about 3D models and download their files, all with a screen reader.

It uses standard Windows controls, so NVDA, JAWS and Narrator read it like any other Windows program. Important events, such as how many results were found or that a download finished, are also spoken and shown in braille through your screen reader.

TV-Blind is an independent project. It is not made or endorsed by Thingiverse. Every thing's Thingiverse page is one key press away, so you can always open the original.

## Features

- Search Thingiverse and browse results in a list with name, creator and number of likes.
- Read a thing's description and instructions as plain text, without web page clutter.
- Download one file, selected files, all files, or the images of a thing. Downloads go to a folder per thing.
- Open any thing on the Thingiverse website.
- Sign in through your browser. Your token is kept in Windows Credential Manager.
- Updates itself. Choose stable releases or development builds under File, Settings. Every update is checked against a signature before it is installed.

## Download and install

1. Go to the [releases page](https://github.com/garo-pro/tvb/releases) and download `TV-Blind.zip` from the latest release.
2. Extract the zip file.
3. Run `TV-Blind.exe`. No installer is needed.

TV-Blind needs Windows 10 or later, 64-bit.

By downloading or using TV-Blind you accept the [license](docs/eula.md). How the app handles your data is described in the [privacy policy](docs/privacy-policy.md). In short, it collects nothing, and talks only to Thingiverse and, to check for updates, to GitHub.

## Getting started

1. When TV-Blind starts for the first time, it asks you to sign in. Choose "Sign in with browser". Thingiverse opens in your web browser. Log in if needed, and approve access.
2. Your browser then says you are signed in, and TV-Blind announces your name. Switch back to TV-Blind.
3. Type a search term and press Enter. TV-Blind announces how many results it found.
4. Press Tab to reach the results list, and Enter on a result to open its details.
5. In the details window, press Alt+A to download all files. TV-Blind announces when the download is complete.

If you have your own Thingiverse API token, you can use File, Paste API token instead of signing in.

## Keyboard shortcuts

Press F1 in the app for this list.

In the main window:

- Alt+T: search field (Ctrl+F also works)
- Alt+S: search
- Alt+R or Ctrl+R: results list
- Enter on a result, or Alt+O: open details
- Alt+M or Ctrl+M: load more results
- Ctrl+O: open the download folder

In the details window:

- Alt+D: description
- Alt+F: file list. Enter on a file downloads just that file.
- Alt+S: download selected files. Use Ctrl+Space or Shift with the arrow keys to select several.
- Alt+A: download all files
- Alt+I: download images
- Alt+W: open the thing on the Thingiverse website
- Escape: close

The File menu has sign-in, sign-out, settings (Ctrl+Comma) and cache options. The Help menu has this list, update checks, the privacy policy, the third-party licenses, and a link to report problems.

## Where things are stored

- Downloads: `Downloads\Thingiverse`, one folder per thing. You can change this under File, Settings.
- Sign-in token: Windows Credential Manager, under `tvb-thingiverse`.
- Settings (update channel, update checks, download folder): `%APPDATA%\tvb`.
- Temporary cache: `%LOCALAPPDATA%\tvb`. Entries are deleted after 30 days, or right away with File, Clear cache.

## License

TV-Blind is open source under the [MIT License](LICENSE).

## Help and bug reports

Please [open an issue](https://github.com/garo-pro/tvb/issues/new/choose). Include which screen reader you use, and what it said if the problem is about speech or braille. Report security problems privately as described in [SECURITY.md](SECURITY.md).

## Building from source

You need:

- Windows 10 or later
- [Rust](https://rustup.rs) (stable, with the MSVC toolchain)
- Visual Studio 2022 Build Tools with the "Desktop development with C++" workload
- [CMake](https://cmake.org) on your PATH

Then run:

```
cargo build --release
```

The app is written in Rust. It uses [wxDragon](https://crates.io/crates/wxdragon) for native Windows controls and [Prism](https://github.com/garo-pro/prism2rust) for screen reader speech. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to work on it.
