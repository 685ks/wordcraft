//! Optional craft-fonts build input (https://github.com/685ks/craft-fonts), embedded by its
//! `craft-fonts-build` helper (see its `docs/integration.md`). With `CRAFT_FONTS_DIR=<checkout>`
//! the fonts in its `fonts/manifest.txt` are embedded as `CRAFT_FONTS`; unset, `CRAFT_FONTS` is
//! empty. Web (wasm32) builds embed only the UI face, BIZ UDPGothic Regular (the helper's
//! default), to stay within the web size budget. The helper's `legacy` output fills this crate's
//! own `CraftFont` struct (`src/lib.rs`).
//!
//! With `CRAFT_FONTS_REQUIRED` (releases), a native build also fails when the checkout has no
//! font for a CJK interface language (Japanese, Simplified Chinese), as an outdated pin once did
//! (#241). It only reads the local checkout: no network.
use std::path::PathBuf;

fn main() {
    craft_fonts_build::Builder::new().legacy(true).emit();
    check_required_scripts();
}

/// Scripts a release build (`CRAFT_FONTS_REQUIRED`, native) must embed a font for: the CJK
/// interface languages' (Japanese, Simplified Chinese). Each needs its own face: without the
/// Chinese one, Chinese menus fall back to the Japanese face and show boxes for simplified hanzi.
const REQUIRED_SCRIPTS: &[&str] = &["Jpan", "Hans"];

fn check_required_scripts() {
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    if wasm || std::env::var_os(craft_fonts_build::REQUIRED_ENV).is_none() {
        return;
    }
    // A missing or malformed checkout is already a `cargo::error` from the helper.
    let Some(dir) = std::env::var_os(craft_fonts_build::DIR_ENV).map(PathBuf::from) else { return };
    let Ok(text) = std::fs::read_to_string(dir.join("fonts/manifest.txt")) else { return };
    let Ok(entries) = craft_fonts_build::parse_manifest(&text) else { return };
    for s in REQUIRED_SCRIPTS.iter().filter(|s| !entries.iter().any(|e| e.scripts.iter().any(|c| c == *s))) {
        println!("cargo::error={}={}: no {s} font in fonts/manifest.txt (craft-fonts too old?)", craft_fonts_build::DIR_ENV, dir.display());
    }
}
