fn main() {
    // Skia's Vulkan is the archive built for Linux, so a Windows build leaves the
    // feature out; one that names it anyway says so here rather than failing in
    // Slint's own link, which names no feature at all.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some()
        && std::env::var_os("CARGO_FEATURE_SKIA_VULKAN").is_some()
    {
        panic!(
            "the skia-vulkan feature is Skia's Vulkan, which is built for Linux; a Windows build \
             leaves it out (--no-default-features --features desktop,discord)"
        );
    }
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
    // The interface stays in the repository's ui/ folder, beside its fonts and icons.
    slint_build::compile_with_config(
        "../../ui/yapfast.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .expect("could not compile Yapfast's Slint UI");
}
