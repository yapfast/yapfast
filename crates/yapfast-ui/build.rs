fn main() {
    // With the live-preview feature, Slint generates a wrapper around its
    // interpreter instead of compiled code: the window loads ui/ at runtime
    // and reloads it on each save. The compiler reads SLINT_LIVE_PREVIEW, so
    // the feature alone sets it; the wrapper doesn't build without the
    // feature's dependencies.
    // SAFETY: nothing else in the build script runs yet.
    unsafe {
        if std::env::var_os("CARGO_FEATURE_LIVE_PREVIEW").is_some() {
            std::env::set_var("SLINT_LIVE_PREVIEW", "1");
        } else {
            std::env::remove_var("SLINT_LIVE_PREVIEW");
        }
    }
    // The element handles the accessibility tests walk (`src/ui/tests.rs`) need
    // the compiled tree to carry its element names, which the compiler leaves
    // out unless asked. A shipped build isn't asked: the names are what those
    // tests read, and nothing at runtime does. `YAPFAST_SLINT_DEBUG_INFO=1`
    // builds the interface for them.
    println!("cargo:rerun-if-env-changed=YAPFAST_SLINT_DEBUG_INFO");
    let debug_info = std::env::var_os("YAPFAST_SLINT_DEBUG_INFO").is_some();
    // The interface stays in the repository's ui/ folder, beside its fonts and icons.
    slint_build::compile_with_config(
        "../../ui/yapfast.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent".into())
            .with_debug_info(debug_info),
    )
    .expect("could not compile Yapfast's Slint UI");
}
