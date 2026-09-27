//! Update checks through ship-shape: signed zips from GitHub Releases.

use std::sync::Arc;

use ship_shape::ui::{CheckTrigger, run_update_check};
use ship_shape::{InstallKind, UpdateChannel as ShipChannel, UpdaterConfig};

use super::Ctx;
use crate::config::{self, UpdateChannel};

/// Release asset base name: every release carries `TV-Blind.zip` and
/// `TV-Blind.zip.minisig`.
const ASSET_NAME: &str = "TV-Blind";

fn updater_config() -> Arc<UpdaterConfig> {
    Arc::new(
        UpdaterConfig::new(
            config::UPDATE_REPO,
            ASSET_NAME,
            "TV-Blind",
            config::UPDATE_PUBLIC_KEY,
            env!("CARGO_PKG_VERSION"),
        )
        .with_commit(env!("TVB_COMMIT"))
        .with_install_kind(InstallKind::Portable),
    )
}

/// Starts a check in the background. `Automatic` only shows something when an
/// update exists; `Manual` also reports "up to date" and errors.
pub fn check(ctx: &Ctx, trigger: CheckTrigger) {
    let channel = match ctx.settings.borrow().update_channel {
        UpdateChannel::Stable => ShipChannel::Stable,
        UpdateChannel::Development => ShipChannel::Dev,
    };
    if trigger == CheckTrigger::Manual {
        ctx.status("Checking for updates...");
    }
    run_update_check(updater_config(), &ctx.frame, channel, trigger);
}

/// The startup check, if enabled. Skipped in debug builds, which don't come
/// from a release zip and shouldn't be replaced by one.
pub fn check_on_startup(ctx: &Ctx) {
    if ctx.settings.borrow().check_updates_on_startup && !cfg!(debug_assertions) {
        check(ctx, CheckTrigger::Automatic);
    }
}
