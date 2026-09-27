## What does this change?

<!-- Describe the change and why it is needed. Link issues with "Fixes #123". -->

## How was it tested?

<!-- Tests added or run, and manual checks. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass locally.
- [ ] New or changed controls have a label directly before them, a keyboard shortcut where it makes sense, and a sensible tab order.
- [ ] New status messages are announced through `Ctx::announce` and shown in the status bar.
- [ ] Tested with a screen reader (say which), or explained why that isn't needed.
- [ ] If the change affects what data is stored or sent, `docs/privacy-policy.md` is updated.
