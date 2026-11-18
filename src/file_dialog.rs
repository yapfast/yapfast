//! Asynchronous native file pickers and savers shared between desktop
//! (`rfd::AsyncFileDialog`) and Android Storage Access Framework (`ACTION_OPEN_DOCUMENT`
//! / `ACTION_CREATE_DOCUMENT`).

use std::path::PathBuf;

/// Lets the user select one or more files to attach or upload.
pub(crate) async fn pick_files(title: &str) -> Option<Vec<PathBuf>> {
    #[cfg(feature = "desktop")]
    {
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .pick_files()
            .await
            .map(|files| files.iter().map(|file| file.path().to_owned()).collect())
    }
    #[cfg(feature = "android")]
    {
        let _ = title;
        crate::android::pick_files(true, "*/*").await
    }
}

/// Lets the user select a single file.
pub(crate) async fn pick_file(title: &str) -> Option<PathBuf> {
    #[cfg(feature = "desktop")]
    {
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .pick_file()
            .await
            .map(|file| file.path().to_owned())
    }
    #[cfg(feature = "android")]
    {
        let _ = title;
        crate::android::pick_files(false, "*/*")
            .await
            .and_then(|mut files| files.pop())
    }
}

/// Lets the user select a single image file matching `extensions`.
pub(crate) async fn pick_image(
    title: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    #[cfg(feature = "desktop")]
    {
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .add_filter(filter_name, extensions)
            .pick_file()
            .await
            .map(|file| file.path().to_owned())
    }
    #[cfg(feature = "android")]
    {
        let _ = (title, filter_name, extensions);
        crate::android::pick_files(false, "image/*")
            .await
            .and_then(|mut files| files.pop())
    }
}

/// Prepares a writable destination path for saving a downloaded attachment.
///
/// On Android this opens `ACTION_CREATE_DOCUMENT` and returns a temporary
/// staging path whose contents are copied to the selected SAF URI once
/// [`finish_save`] runs after `Event::Saved`.
pub(crate) async fn save_file(title: &str, file_name: &str) -> Option<PathBuf> {
    #[cfg(feature = "desktop")]
    {
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_file_name(file_name)
            .save_file()
            .await
            .map(|file| file.path().to_owned())
    }
    #[cfg(feature = "android")]
    {
        crate::android::prepare_save_file(title, file_name).await
    }
}

/// Commits or cleans up a staged attachment export on Android once written.
#[allow(unused_variables)]
pub(crate) fn finish_save(name: &str, ok: bool) {
    #[cfg(feature = "android")]
    crate::android::finish_save(name, ok);
}
