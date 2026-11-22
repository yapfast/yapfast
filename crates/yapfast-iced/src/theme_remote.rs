//! Themes from GitHub and the marketplace catalog, fetched away from iced's
//! UI executor. Core owns URL parsing, archive limits, catalog parsing,
//! digest verification and install checking; this module only moves the
//! checked bytes over HTTPS and turns previews into iced-ready pixels.

use std::sync::Arc;

use iced::advanced::image::Handle;
use yapfast_core::{
    runtime::BackgroundRuntime,
    themes::{
        check,
        install::Fetched,
        remote::{self, Catalog, Listing, Repository},
    },
};

const STOPPED: &str = "Fetching the theme stopped partway; try again.";
const NO_REPOSITORY: &str =
    "GitHub has no such repository, branch or folder; a theme's repository has to be public";
const THUMBNAIL: (u32, u32) = (672, 378);

/// Decoded preview pixels and the iced handle that draws them.
#[derive(Clone, Debug)]
pub struct Preview {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
}

/// A fetched theme's previews, in manifest order.
pub type Previews = Vec<Preview>;

/// The hosts a theme's bytes come from. A redirect elsewhere is refused, so
/// what the address checks allowed is what's read.
const GITHUB_HOSTS: [&str; 3] = [
    "api.github.com",
    "codeload.github.com",
    "raw.githubusercontent.com",
];

/// Previews fetched and decoded at once; the rest wait their turn, so a long
/// catalog doesn't open a connection and decode a picture for every card
/// together.
static PREVIEWS_AT_ONCE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

/// One bounded HTTPS client for every theme fetch, made on first use: no
/// cookies, HTTPS only, redirects only between GitHub's hosts, a short
/// connection timeout, a stalled-read timeout and a limit on the whole
/// request.
fn client() -> Result<reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .ok();
            reqwest::Client::builder()
                .user_agent(concat!("Yapfast/", env!("CARGO_PKG_VERSION")))
                .https_only(true)
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    let github = attempt
                        .url()
                        .host_str()
                        .is_some_and(|host| GITHUB_HOSTS.contains(&host));
                    if attempt.previous().len() >= 5 || !github {
                        attempt.stop()
                    } else {
                        attempt.follow()
                    }
                }))
                .connect_timeout(std::time::Duration::from_secs(20))
                .read_timeout(std::time::Duration::from_secs(60))
                // A whole request, however slowly it drips, ends.
                .timeout(std::time::Duration::from_secs(180))
                .build()
                .map_err(|error| error.to_string())
        })
        .clone()
}

/// Asks for `url`, and reads its answer whole if it is at most `limit`
/// bytes. Both Content-Length and the streamed body are bounded.
async fn get(
    client: &reqwest::Client,
    url: &str,
    limit: u64,
    accept: Option<&str>,
    missing: &str,
) -> Result<Vec<u8>, String> {
    let mut request = client.get(url);
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("GitHub couldn't be reached: {}", error.without_url()))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND
        || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
    {
        return Err(missing.to_owned());
    }
    if !status.is_success() {
        return Err(format!("GitHub answered {status}; try again later"));
    }
    let too_large = || format!("{url} is larger than a theme's {limit} bytes, so it wasn't read");
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("The download stopped: {}", error.without_url()))?
    {
        if (bytes.len() + chunk.len()) as u64 > limit {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// The commit a repository's ref is at, and its archive there. A full commit
/// from a marketplace listing does not spend another API request resolving
/// itself.
async fn download(repository: Repository) -> Result<(String, Vec<u8>), String> {
    let client = client()?;
    let commit = match repository.reference.as_deref() {
        Some(reference) if remote::is_commit(reference) => reference.to_owned(),
        _ => remote::read_commit(
            &get(
                &client,
                &repository.commit_url(),
                256,
                Some("application/vnd.github.sha"),
                NO_REPOSITORY,
            )
            .await?,
        )?,
    };
    let archive = get(
        &client,
        &repository.archive_url(&commit),
        remote::MAX_ARCHIVE_BYTES,
        None,
        NO_REPOSITORY,
    )
    .await?;
    Ok((commit, archive))
}

/// Fetches a repository at one commit and checks it exactly as an install
/// would. A marketplace listing additionally pins the file digest and id.
/// Nothing is persisted until the UI explicitly installs the returned
/// [`Fetched`].
pub async fn fetch(
    runtime: Arc<BackgroundRuntime>,
    repository: Repository,
    listing: Option<Listing>,
) -> Result<(Fetched, Previews), String> {
    let (commit, archive) = runtime
        .spawn(download(repository.clone()))
        .await
        .map_err(|_| STOPPED.to_owned())??;
    runtime
        .spawn_blocking(move || {
            let files = remote::unpack(&archive, &repository.path)?;
            drop(archive);
            if let Some(listing) = &listing {
                remote::verify(&files, &listing.sha256)?;
            }
            let mut fetched = Fetched::new(
                repository.source(),
                repository.path.clone(),
                repository.reference.clone().unwrap_or_default(),
                commit,
                files,
            )?;
            if let Some(listing) = listing {
                if fetched.package().id != listing.id {
                    return Err(format!(
                        "The marketplace lists {} there, but the repository holds {}",
                        listing.id,
                        fetched.package().id
                    ));
                }
                fetched.listed = true;
            }
            let previews = fetched
                .package()
                .preview
                .iter()
                .filter_map(|path| fetched.files.files.get(path))
                .filter_map(|bytes| decode_preview(bytes))
                .collect();
            Ok((fetched, previews))
        })
        .await
        .map_err(|_| STOPPED.to_owned())?
}

/// Reads and validates the marketplace catalog.
pub async fn catalog(runtime: Arc<BackgroundRuntime>) -> Result<Catalog, String> {
    runtime
        .spawn(async {
            let client = client()?;
            let bytes = get(
                &client,
                remote::CATALOG_URL,
                remote::MAX_CATALOG_BYTES,
                None,
                "The theme catalog isn't published yet",
            )
            .await
            .map_err(|error| format!("The marketplace couldn't be loaded. {error}."))?;
            remote::read_catalog(&bytes)
        })
        .await
        .map_err(|_| "Loading the marketplace stopped partway; try again.".to_owned())?
}

/// Downloads and decodes one marketplace card preview, a few at a time. A
/// missing, invalid or oversized picture is just no preview.
pub async fn preview(runtime: Arc<BackgroundRuntime>, url: String) -> Option<Preview> {
    let _turn = PREVIEWS_AT_ONCE.acquire().await.ok()?;
    let bytes = runtime
        .spawn(async move {
            let client = client().ok()?;
            get(&client, &url, remote::MAX_PREVIEW_BYTES, None, "")
                .await
                .ok()
        })
        .await
        .ok()??;
    runtime
        .spawn_blocking(move || decode_preview(&bytes))
        .await
        .ok()?
}

/// Decodes PNG, JPEG or WebP under the core's dimension limit, then shrinks
/// it to the review/gallery size before handing its RGBA bytes to iced.
pub(crate) fn decode_preview(bytes: &[u8]) -> Option<Preview> {
    shrunk_preview(bytes).map(preview_of)
}

/// A checked PNG, JPEG or WebP preview decoded and shrunk to fit the
/// gallery's thumbnail, straight RGBA.
pub(crate) fn shrunk_preview(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(check::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(check::MAX_IMAGE_SIDE);
    reader.limits(limits);
    let decoded = reader.decode().ok()?.into_rgba8();
    let (width, height) = decoded.dimensions();
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(THUMBNAIL.0) / f64::from(width.max(1)),
            f64::from(THUMBNAIL.1) / f64::from(height.max(1)),
        ),
    );
    Some(if scale < 1.0 {
        image::imageops::thumbnail(
            &decoded,
            ((f64::from(width) * scale).round() as u32).max(1),
            ((f64::from(height) * scale).round() as u32).max(1),
        )
    } else {
        decoded
    })
}

/// Shrunk preview pixels as iced draws them.
pub(crate) fn preview_of(shrunk: image::RgbaImage) -> Preview {
    let (width, height) = shrunk.dimensions();
    let mut pixels = shrunk.into_raw();
    // The patched CPU renderer consumes premultiplied native BGRA, while
    // wgpu consumes RGBA; this is the same boundary every other iced image
    // made from decoded pixels uses.
    crate::paint::to_native(&mut pixels);
    Preview {
        handle: Handle::from_rgba(width, height, pixels),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder as _;

    #[test]
    fn previews_are_checked_and_shrunk_to_the_gallery() {
        let pixels = vec![0x80; 800 * 400 * 4];
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&pixels, 800, 400, image::ExtendedColorType::Rgba8)
            .unwrap();
        let preview = decode_preview(&png).expect("a checked PNG");
        assert_eq!((preview.width, preview.height), (672, 336));
        assert!(decode_preview(b"not a picture").is_none());
    }
}
