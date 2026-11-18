//! Shared Yapfast client library supporting desktop (`winit`) and Android
//! (`android-activity`) frontends alongside the headless CLI binary.

#![recursion_limit = "256"]

pub mod build_info;
#[cfg(feature = "client")]
pub mod version;

#[cfg(feature = "android")]
pub mod android;
#[cfg(feature = "android")]
pub use android_activity;
#[cfg(feature = "android")]
pub use jni;

#[cfg(feature = "desktop")]
mod autostart;
#[cfg(feature = "client")]
mod copy;
#[cfg(feature = "client")]
mod custom_emoji;
#[cfg(all(feature = "desktop", target_os = "linux"))]
mod desktop_entry;
#[cfg(feature = "client")]
mod discord_emoji_names;
#[cfg(feature = "desktop")]
mod drop;
#[cfg(feature = "client")]
mod emoji;
#[cfg(feature = "client")]
mod emoji_font;
#[cfg(feature = "client")]
mod emoji_font_tables;
#[cfg(feature = "client")]
mod emoji_text;
#[cfg(feature = "client")]
mod file_dialog;
#[cfg(feature = "desktop")]
pub mod graphics;
#[cfg(feature = "desktop")]
mod icon;
#[cfg(feature = "desktop")]
mod instance;
#[cfg(feature = "desktop")]
mod keybinds;
#[cfg(feature = "client")]
mod notify;
#[cfg(feature = "client")]
mod paste;
#[cfg(feature = "desktop")]
pub mod protocol;
#[cfg(feature = "desktop")]
mod tray;
#[cfg(feature = "client")]
mod twemoji;
#[cfg(feature = "desktop")]
pub mod ui;
#[cfg(all(feature = "client", not(feature = "desktop")))]
mod ui;
#[cfg(feature = "desktop")]
pub mod update;
#[cfg(feature = "desktop")]
mod zoom;

#[cfg(feature = "android")]
pub fn android_main(app: android_activity::AndroidApp) {
    android::main(app);
}
