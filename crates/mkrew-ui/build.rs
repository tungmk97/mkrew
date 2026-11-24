//! Build steps of `mkrew.exe`, Windows only (no-ops on other targets):
//! - Embeds the app icon (`resources/windows/mkrew.ico`, from `icon/mkrew-1-lanes/tile-dark.svg`).
//! - Links the bin with an 8 MiB main-thread stack (MSVC only).
//!
//! Regenerate the .ico after the SVG changes (16, 20, 24, 32, 40, 48, 64, 256 px), e.g. with ImageMagick:
//! `magick -background none -density 384 icon/mkrew-1-lanes/tile-dark.svg -define icon:auto-resize=256,64,48,40,32,24,20,16 crates/mkrew-ui/resources/windows/mkrew.ico`

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/mkrew.rc");
    println!("cargo:rerun-if-changed=resources/windows/mkrew.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "resources/windows/mkrew.rc",
            ["mkrew"],
            embed_resource::NONE,
        )
        .manifest_optional()
        .unwrap_or_else(|e| panic!("embedding the app icon failed: {e}"));
        // GPUI builds and lays out element trees recursively on the main thread, and a deep
        // TSP document (an omp transcript) needs more than the 1 MiB MSVC default; macOS and
        // Linux give the main thread 8 MiB.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bin=mkrew=/STACK:8388608");
        }
    }
}
