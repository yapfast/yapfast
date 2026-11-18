# Android build and port

Yapfast runs natively on `aarch64-linux-android` (Android 9 / API 28 and newer) by sharing `yapfast-core`, `yapfast-discord`, and the Slint UI (`yapfast-ui`) without bundling a web view or external native shared libraries beyond Android NDK system libraries.

## Architecture

- **Frontend and JNI host** (`crates/yapfast-android`, `android/`): Builds `libyapfast.so` (`cdylib`), exporting `android_main` for Slint's `backend-android-activity-06` (`NativeActivity`) and JNI callbacks for `io.github.yapfast.YapfastActivity`. System TLS certificates are verified through `rustls-platform-verifier` (`rustls:rustls-platform-verifier:0.1.1`).
- **Audio** (`crates/yapfast-core/src/audio_io/aaudio.rs`): Uses Android's C NDK `AAudio` API directly for low-latency call capture, playback, and device-change reopening.
- **Media decoding** (`crates/yapfast-core/native/decode_android.c`, `video_android.c`): Decodes watched VP8 streams with `libvpx`, watched H264 streams with `AMediaCodec`, and attached video/audio files with `AMediaExtractor` + `AMediaCodec` + `libyuv` (`libmediandk`). `crates/yapfast-core/src/stream_worker.rs` runs video playback in an in-process worker thread over `std::io::pipe()` rather than spawning a child process (`yapfast --stream-worker`), and excludes desktop screen capture.
- **Credentials and cache**: Stores the Discord token and cache key under Android's app-private files directory in `0o600` files opened with `O_NOFOLLOW`, alongside the SQLite state databases under `app.internal_data_path()`.
- **File pickers and saves** (`src/file_dialog.rs`, `src/android.rs`, `YapfastActivity.kt`): Maps attachment picking and downloads to Android Storage Access Framework (`ACTION_OPEN_DOCUMENT` and `ACTION_CREATE_DOCUMENT`), staging picked/saved files in cache storage before flushing to SAF URIs.
- **Notifications, deep links, and calls**: Creates notification channels for messages, incoming calls, and active voice calls (`YapfastCallService` runs as a `microphone` foreground service with a Disconnect action), and handles `yapfast://` and `discord://` intents.
- **Logging**: Mirrors Yapfast's filtered operational logs to Android Logcat under the `Yapfast` tag (`crates/yapfast-core/src/logging.rs`).

## Mobile layout

When `mobile` (`cfg!(target_os = "android")`) or `width < 720px` is active in `ui/yapfast.slint`:

- The workspace switches between three full-width panes (`mobile-pane`: `0` server rail and channel list, `1` active conversation with header back arrow, `2` members/profile/thread/search panel) while keeping desktop `>= 800×560` geometry unchanged.
- `YapfastWindow` applies `safe-area-insets` and `virtual-keyboard-size.height` so system bars, display cutouts, and the soft keyboard do not cover the header or composer.
- The composer shows an explicit Send button, and tapping and holding a message opens its context menu after 450 ms alongside the hardware/gesture Back key (`navigate-back-mobile`).

## Prerequisites

Building the APK requires:

- Rust `1.97.1` with target `aarch64-linux-android` (`rustup target add aarch64-linux-android`)
- JDK 17 (`JAVA_HOME`)
- Android SDK (`ANDROID_HOME`, platform API 35, build-tools 35.0.0)
- Android NDK r27+ (`ANDROID_NDK_HOME`, API 28 C/C++ compiler wrappers and `llvm-ar`)
- Gradle 8.10+ (or `android/gradlew`) and internet access on the first build to fetch Gradle/AGP artifacts and WebRTC/Skia archives

## Building the APK

[`scripts/build-android`](../scripts/build-android) cross-compiles `crates/yapfast-android` and packages the APK into `target/dist/`:

```sh
scripts/build-android discord          # Discord-only release build -> target/dist/yapfast-android-discord.apk
scripts/build-android full --fast      # Discord + Matrix with the fast profile -> target/dist/yapfast-android-full.apk
```

By default, Gradle produces a debug-signed APK (`assembleDebug`). Set `ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD` to run `assembleRelease` with release signing.

## Permissions

The manifest declares `INTERNET`, `ACCESS_NETWORK_STATE`, `RECORD_AUDIO`, `MODIFY_AUDIO_SETTINGS`, `CAMERA`, `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE`, and `WAKE_LOCK`. Microphone and camera features are marked `android:required="false"`, and `src/ui.rs` requests `RECORD_AUDIO` and `CAMERA` at runtime when joining voice or enabling video.
