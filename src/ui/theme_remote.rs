//! Themes from GitHub and the marketplace's catalog, fetched on the
//! background runtime so the window never waits on the network.
//!
//! What's fetched is read by `yapfast_core::themes::remote`: an archive into
//! a theme's files, under its limits, and the catalog into listings. The
//! files are then checked as a local folder's are, and nothing is kept until
//! the person installs the theme from its review.

use std::sync::Arc;

use yapfast_core::runtime::BackgroundRuntime;
use yapfast_core::themes::install::Fetched;
use yapfast_core::themes::remote::{self, Catalog, Listing, Repository};

const STOPPED: &str = "Fetching the theme stopped partway; try again.";

/// Asks for `url`, and reads its answer whole if it's at most `limit` bytes.
async fn get(
    client: &reqwest::Client,
    url: &str,
    limit: u64,
    accept: Option<&str>,
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
        return Err(
            "GitHub has no such repository, branch or folder; a theme's repository has to be public"
                .into(),
        );
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

/// The commit a repository's ref is at, and its archive there.
async fn download(repository: Repository) -> Result<(String, Vec<u8>), String> {
    let client = crate::update::client()?;
    let commit = match repository.reference.as_deref() {
        Some(reference) if remote::is_commit(reference) => reference.to_owned(),
        _ => remote::read_commit(
            &get(
                &client,
                &repository.commit_url(),
                256,
                Some("application/vnd.github.sha"),
            )
            .await?,
        )?,
    };
    let archive = get(
        &client,
        &repository.archive_url(&commit),
        remote::MAX_ARCHIVE_BYTES,
        None,
    )
    .await?;
    Ok((commit, archive))
}

/// Fetches the theme in a repository at the commit its ref is at, and checks
/// it as an install would. With a listing, the commit is the one it lists
/// and its files have to be the ones it pins.
pub async fn fetch(
    runtime: Arc<BackgroundRuntime>,
    repository: Repository,
    listing: Option<Listing>,
) -> Result<Fetched, String> {
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
            Ok(fetched)
        })
        .await
        .map_err(|_| STOPPED.to_owned())?
}

/// Reads the marketplace's catalog.
pub async fn catalog(runtime: Arc<BackgroundRuntime>) -> Result<Catalog, String> {
    runtime
        .spawn(async {
            let client = crate::update::client()?;
            let bytes = get(
                &client,
                remote::CATALOG_URL,
                remote::MAX_CATALOG_BYTES,
                None,
            )
            .await
            .map_err(|error| format!("The marketplace couldn't be loaded. {error}"))?;
            remote::read_catalog(&bytes)
        })
        .await
        .map_err(|_| "Loading the marketplace stopped partway; try again.".to_owned())?
}

/// A listed theme's preview, decoded at most 960 pixels wide off the UI
/// thread; `None` if it couldn't be had.
pub async fn preview(
    runtime: Arc<BackgroundRuntime>,
    url: String,
) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let bytes = runtime
        .spawn(async move {
            let client = crate::update::client().ok()?;
            get(&client, &url, remote::MAX_PREVIEW_BYTES, None)
                .await
                .ok()
        })
        .await
        .ok()??;
    runtime
        .spawn_blocking(move || super::theme_look::decode_preview(&bytes))
        .await
        .ok()?
}
