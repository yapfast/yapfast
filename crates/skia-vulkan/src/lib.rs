//! Skia's Vulkan, for the builds that draw Yapfast's window on it.
//!
//! There is no code here: the crate is its manifest. Cargo decides features for
//! every target at once, and the archive with Vulkan in it is Linux's, so
//! Yapfast's `skia-vulkan` feature turns this crate on and this crate asks for
//! Vulkan on Linux alone — see its `Cargo.toml`. A Windows build of the same tree
//! gets nothing from it: the OpenGL archive, and Direct3D 12 through wgpu where
//! the GPU setting asks for a device.
