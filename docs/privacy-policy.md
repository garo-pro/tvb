# TV-Blind privacy policy

Last updated: 27 September 2026

TV-Blind ("the app") is a free Thingiverse desktop client for blind and visually impaired people, made by [garo-pro](https://github.com/garo-pro). It is not made or endorsed by Thingiverse.

In short: the app collects nothing about you. Everything it stores stays on your computer, and it talks only to Thingiverse.

## What the app stores, and where

All of this is on your own computer. None of it is sent to the app's developer.

- **Your Thingiverse access token.** It is stored in Windows Credential Manager under the name `tvb-thingiverse`. The app uses it only to talk to Thingiverse on your behalf.
- **Settings.** Right now that is only your chosen download folder. They are stored in `%APPDATA%\tvb\config\settings.json`.
- **A temporary cache** of search results and thing details, so that pages you just viewed open quickly. It is stored in `%LOCALAPPDATA%\tvb\cache`. Cached thing details can include the public name of the thing's creator. Entries are reused for at most one day. They are deleted automatically when they are more than 30 days old, each time the app starts. You can delete them at any time with File, Clear cache.
- **Files you download.** They go to the folder you chose, by default `Downloads\Thingiverse`. They are yours, and the app never deletes them.

## What the app sends, and to whom

The app connects only to Thingiverse (`thingiverse.com`, `api.thingiverse.com` and Thingiverse's file servers), and always over encrypted HTTPS. It sends:

- your access token, to prove to Thingiverse that the request comes from you;
- what you search for, and which things, files and images you open or download;
- the app's name and version.

Thingiverse handles this information under [its own privacy policy](https://www.thingiverse.com/legal/privacy). Files downloaded through the app may appear in your Thingiverse download history, just as they would if you downloaded them on the website.

## Signing in

"Sign in with browser" opens Thingiverse's own sign-in and consent page in your web browser. The app never sees your Thingiverse password. While sign-in is in progress, the app listens on your own computer only (`127.0.0.1`, port 47823) to receive the token from your browser. It stops listening as soon as sign-in finishes, fails or times out after five minutes.

## What the app does not do

- It has no analytics, telemetry, crash reporting or advertising.
- It does not send anything to the developer or to anyone other than Thingiverse.
- It does not sell, share or transfer any data.
- It does not change your Thingiverse account, profile or privacy settings.

Speech and braille announcements are produced by your screen reader or by Windows' built-in speech on your own computer.

## Your data and your choices

Everything the app stores is on your computer, so you can see and delete it yourself:

- **Sign out** (File menu) removes the saved token from Credential Manager.
- **Clear cache** (File menu) deletes all cached search results and thing details.
- Deleting the `tvb` folders in `%APPDATA%` and `%LOCALAPPDATA%` removes all remaining settings and cache.

You can also revoke the app's access to your account at any time in your Thingiverse account settings.

For data that Thingiverse holds about you, contact Thingiverse as described in its privacy policy.

## Children

The app is not directed at children under 13.

## Changes to this policy

If this policy changes, the new version will be published at the same address, with a new "Last updated" date. Any change that affects what the app collects will also be noted in the release notes.

## Contact

Questions or concerns: open an issue at <https://github.com/garo-pro/tvb/issues>.
