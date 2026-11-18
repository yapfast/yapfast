fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // Align ELF LOAD segments to 16 KiB for Android 15+ devices when loading
        // libyapfast.so directly from the APK (`useLegacyPackaging = false`).
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
        // Prebuilt Android Skia bundles libjpeg-turbo alongside libwebrtc.a's copy
        // of jpeg_natural_order; see build.rs in the workspace root.
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
        // Force archive extraction and dynamic export of libwebrtc JNI symbols and
        // JNI_OnLoad in libyapfast.so for libwebrtc.jar classes. rustc passes its own
        // version script with `local: *;` for cdylib targets, which overrides
        // `--export-dynamic-symbol`; merging an auxiliary version script with global
        // entries in ld.lld preserves these entry points past `--gc-sections` and
        // exports them in `.dynsym`.
        let out_dir = std::path::PathBuf::from(
            std::env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo"),
        );
        let version_script = out_dir.join("webrtc_jni.ver");
        std::fs::write(
            &version_script,
            "{\n  global:\n    JNI_OnLoad;\n    JNI_OnUnLoad;\n    Java_livekit_*;\n};\n",
        )
        .expect("write WebRTC JNI version script");

        println!("cargo:rustc-link-arg=-Wl,-u,JNI_OnLoad");
        println!("cargo:rustc-link-arg=-Wl,-u,JNI_OnUnLoad");
        println!("cargo:rustc-link-arg=-Wl,--undefined-glob=Java_livekit_*");
        println!(
            "cargo:rustc-link-arg=-Wl,--version-script={}",
            version_script.display()
        );
    }
}
