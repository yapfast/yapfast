// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use cfg_aliases::cfg_aliases;

fn main() {
    // Setup cfg aliases
    cfg_aliases! {
       skia_backend_opengl: { any(feature = "opengl", not(any(target_vendor = "apple", target_family = "windows", target_arch = "wasm32"))) },
       skia_backend_wgpu: { any(feature = "wgpu-29", feature = "wgpu-30") },
       skia_backend_software: { not(target_os = "android") },
       skia_backend_softbuffer: { all(skia_backend_software, feature = "softbuffer") },
       // Targets where wgpu and Skia both have their Vulkan backend compiled in:
       // wgpu on, and Skia's own Vulkan with it. Yapfast: rust-skia gates `gpu::vk`
       // behind the "vulkan" feature since 0.157, and the prebuilt Skia Yapfast draws
       // with on Linux holds its own FreeType and no Vulkan, so on Linux this is off;
       // Yapfast asks for Skia's OpenGL there and never for wgpu.
       skia_wgpu_vulkan: { all(skia_backend_wgpu, feature = "vulkan") },
       // Yapfast: wgpu's surface is built but Skia has no Vulkan for it to wrap, so
       // it draws nowhere. The winit backend still names those renderers.
       skia_wgpu_without_vulkan: { all(skia_backend_wgpu, not(skia_wgpu_vulkan)) },
       skia_windowed: { any(skia_backend_wgpu, skia_backend_opengl, skia_backend_softbuffer) },
    }

    println!("cargo:rustc-check-cfg=cfg(slint_nightly_test)");
}
