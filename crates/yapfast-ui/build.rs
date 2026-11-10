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
    // The interface stays in the repository's ui/ folder, beside its fonts and icons.
    // Its `@tr` texts are translated from translations/<language>/LC_MESSAGES/yapfast-ui.po,
    // which the program carries and switches between with
    // `slint::select_bundled_translation`; see docs/i18n.md.
    println!("cargo:rerun-if-changed=../../translations");
    slint_build::compile_with_config(
        "../../ui/yapfast.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent".into())
            .with_bundled_translations("../../translations")
            // The texts are keyed by their English wording alone, as in the
            // translation files `scripts/i18n` writes.
            .with_default_translation_context(slint_build::DefaultTranslationContext::None),
    )
    .expect("could not compile Yapfast's Slint UI");
}
