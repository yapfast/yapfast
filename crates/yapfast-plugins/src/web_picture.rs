//! Pictures from the web in what a plugin draws: an `image` cell whose `src`
//! is an `https://` address on one of the hosts the plugin's manifest lists,
//! for a plugin that holds `images.web`.
//!
//! The window never fetches: it asks the bridge, which makes the request
//! through the plugin's own network gate (HTTPS, the declared hosts, public
//! addresses, Discord's hosts refused), and decodes the answer on a blocking
//! thread, so neither the interface nor the bridge's task waits on a picture.
//! What comes back is pixels the window can draw as they are.

use std::io::Cursor;

/// The most a picture's file may hold: what one answer of the gate carries
/// before it spills to a file the window can't read.
pub const MAX_BYTES: u64 = 752 << 10;

/// The longest side a picture is kept at. A cell draws it at most 240 by 135,
/// so a bigger picture would only cost memory at twice the size.
pub const MAX_SIDE: u32 = 512;

/// A decoded picture, ready to draw.
#[derive(Clone, PartialEq, Eq)]
pub struct WebPicture {
    pub width: u32,
    pub height: u32,
    /// RGBA, eight bits a channel, row after row.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for WebPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WebPicture({}x{})", self.width, self.height)
    }
}

/// Decodes a picture's bytes under the same size limits `fs.thumbnail` uses,
/// and makes it at most [`MAX_SIDE`] on its longer side.
pub fn decode(bytes: &[u8]) -> Result<WebPicture, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("it isn't a picture: {error}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 << 20);
    reader.limits(limits);
    let picture = reader
        .decode()
        .map_err(|error| format!("it couldn't be read as a picture: {error}"))?;
    let picture = if picture.width() > MAX_SIDE || picture.height() > MAX_SIDE {
        picture.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        picture
    };
    let rgba = picture.to_rgba8();
    Ok(WebPicture {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// Whether `plugin`'s manifest lets it show a picture from `url`: `images.web`
/// and the host in its `network` list. The gate checks the host again when it
/// fetches; this is the reason the window can show without a request.
pub fn allowed(
    permissions: &yapfast_plugin_api::capability::Permissions,
    url: &str,
) -> Result<(), String> {
    use yapfast_plugin_api::capability::Capability;
    crate::permissions::check(permissions, Capability::ImagesWeb)
        .map_err(|denial| denial.reason().to_owned())?;
    let Some(crate::ui::Source::Web { host, .. }) = crate::ui::source(url) else {
        return Err(format!("`{url}` isn't an https address of a picture"));
    };
    let host = host.split(':').next().unwrap_or(host);
    if !permissions
        .network
        .iter()
        .any(|pattern| pattern.matches(host))
    {
        return Err(format!(
            "{host} isn't one of the hosts this plugin's manifest lists, which are the only \
             ones its pictures come from"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::capability::{Capability, HostPattern, Permissions};

    #[test]
    fn a_picture_comes_only_from_a_listed_host_with_images_web() {
        let mut permissions = Permissions {
            capabilities: vec![Capability::ImagesWeb],
            network: vec![HostPattern::parse("i.example.com").unwrap()],
            ..Permissions::default()
        };
        assert!(allowed(&permissions, "https://i.example.com/cat.png").is_ok());
        assert!(allowed(&permissions, "https://evil.example/cat.png").is_err());
        assert!(allowed(&permissions, "http://i.example.com/cat.png").is_err());
        permissions.capabilities.clear();
        let refused = allowed(&permissions, "https://i.example.com/cat.png").unwrap_err();
        assert!(refused.contains("images.web"), "{refused}");
    }

    #[test]
    fn a_big_picture_is_kept_small_and_a_non_picture_is_refused() {
        let mut png = Vec::new();
        image::RgbaImage::new(1200, 600)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode(&png).expect("a picture");
        assert_eq!((decoded.width, decoded.height), (512, 256));
        assert_eq!(decoded.rgba.len(), 512 * 256 * 4);
        assert!(decode(b"<html>not a picture</html>").is_err());
    }
}
