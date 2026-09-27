# Contributing to TV-Blind

Thanks for helping. TV-Blind exists to make Thingiverse usable with a screen reader, so accessibility comes before everything else, including new features.

## Before you start

For anything bigger than a small fix, please open an issue first so we can agree on the approach.

## Setting up

See "Building from source" in the [README](README.md). The first build takes a while because it compiles native libraries.

## Checks

CI runs these on every push and pull request, and all of them must pass:

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps --document-private-items
cargo deny check
```

Clippy runs with the `pedantic` group and some stricter lints; see `[lints]` in `Cargo.toml`. Fix warnings rather than silencing them. If an `#[allow]` is really needed, add a comment saying why.

Tests that need the network or a Thingiverse account are marked `#[ignore]`. Run them locally with:

```
cargo test -- --ignored --nocapture
```

`live_api` uses the token saved by the app (sign in once first), or the `THINGIVERSE_TOKEN` environment variable.

## Accessibility rules

- Use native controls only.
- Create each label (`StaticText`) directly before its control. Screen readers use the label created just before a control as its name.
- Creation order is tab order. Keep it logical: top to bottom, left to right.
- Give buttons and labels an Alt shortcut with `&`, and keep them unique within a window.
- Announce results and errors with `Ctx::announce`, which updates the status bar and speaks. Use `interrupt = true` only for errors.
- Never block the UI thread on the network or disk. Use `ui::task::background`.
- Check new controls with a screen reader. The name and role NVDA reports should make sense on their own.

## Project layout

- `src/api.rs`: Thingiverse REST client (blocking; run it on worker threads).
- `src/models.rs`: response types. Every field is optional, because the API is inconsistent.
- `src/cache.rs`: JSON file cache, pruned after 30 days.
- `src/config.rs`: settings, project URLs, and the token in Credential Manager.
- `src/oauth.rs`: browser sign-in through a one-time listener on 127.0.0.1.
- `src/speech.rs`: speech and braille through Prism.
- `src/ui/`: windows and dialogs. `task.rs` passes worker results back to the UI thread.

## Releases

1. Update `version` in `Cargo.toml` and commit.
2. Tag the commit with the same version, for example `git tag v0.2.0`, and push the tag.
3. The Release workflow builds the app, signs `TV-Blind.zip`, and creates a draft release. Review it, then publish it. Only published releases are offered to users on the stable update channel.

Every push to `main` that passes CI also rebuilds the rolling `latest` pre-release, which the development update channel follows.

### Signing

Update zips are signed with minisign by `tools/sign` (`tvb-sign`), which reads the key from the environment because the usual tools need an interactive console. The secret key and its password are secrets of the GitHub `release` environment, which only `main` and `v*` tags can use. The matching public key is `UPDATE_PUBLIC_KEY` in `src/config.rs`. If the key ever has to be replaced, every existing install must first receive an update signed with the old key that contains the new public key.
