use embed_manifest::manifest::{ActiveCodePage, DpiAwareness, Setting};
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

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
