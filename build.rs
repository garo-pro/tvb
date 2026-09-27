use embed_manifest::manifest::{ActiveCodePage, DpiAwareness, Setting};
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    // The dev update channel compares this against the rolling dev release.
    println!("cargo:rustc-env=TVB_COMMIT={}", commit_hash());

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(
            new_manifest("Tvb.Thingiverse.Client")
                .dpi_awareness(DpiAwareness::PerMonitorV2)
                .active_code_page(ActiveCodePage::Utf8)
                .long_path_aware(Setting::Enabled),
        )
        .expect("unable to embed manifest");
    }

    // A static Prism pulls screen-reader import libraries into our link; those
    // DLLs ship with the screen readers, so they must be delay-loaded.
    if let Ok(dlls) = std::env::var("DEP_PRISM_DELAYLOAD") {
        for dll in dlls.split(';').filter(|d| !d.is_empty()) {
            println!("cargo:rustc-link-arg=/delayload:{dll}");
        }
        println!("cargo:rustc-link-arg=/DELAY:unload");
        println!("cargo:rustc-link-arg=/ignore:4199");
    }
}

/// Commit being built: `git rev-parse HEAD`, else `GITHUB_SHA`, else empty.
/// Git comes first because CI jobs triggered by another workflow check out a
/// specific commit while `GITHUB_SHA` points at the branch head.
fn commit_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .unwrap_or_default()
}
