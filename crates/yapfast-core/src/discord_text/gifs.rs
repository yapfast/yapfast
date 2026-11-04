//! The GIF picker's commands: trending terms, searches, suggestions, the
//! account's favourites, and the pictures the picker shows.

use super::*;
use crate::chat::{FavoriteGif, Gif, GifCategory};

/// The largest GIF picture the picker downloads. A favourite from another
/// client may be a full-size `.gif`.
const MAX_GIF_BYTES: usize = 8 * 1024 * 1024;

/// Carries out a GIF picker command, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    let discord = match (&command, discord) {
        (
            DiscordCommand::GifCategories
            | DiscordCommand::Gifs(_)
            | DiscordCommand::GifSuggestions(_)
            | DiscordCommand::GifPicked { .. }
            | DiscordCommand::FavoriteGifs
            | DiscordCommand::SetFavoriteGifs(_)
            | DiscordCommand::GifPicture { .. }
            | DiscordCommand::PrefetchFavorites(_),
            Ok(discord),
        ) => discord.clone(),
        (DiscordCommand::GifCategories, Err(why)) => {
            let _ = events.send(Event::GifCategories(Err(why.into())));
            return None;
        }
        (DiscordCommand::Gifs(query), Err(why)) => {
            let _ = events.send(Event::Gifs {
                query: query.clone(),
                result: Err(why.into()),
            });
            return None;
        }
        (DiscordCommand::FavoriteGifs, Err(why)) => {
            let _ = events.send(Event::FavoriteGifs(Err(why.into())));
            return None;
        }
        (
            DiscordCommand::GifSuggestions(_)
            | DiscordCommand::GifPicked { .. }
            | DiscordCommand::SetFavoriteGifs(_)
            | DiscordCommand::GifPicture { .. }
            | DiscordCommand::PrefetchFavorites(_),
            Err(_),
        ) => return None,
        _ => return Some(Command::Discord(command)),
    };
    let events = events.clone();
    match command {
        DiscordCommand::GifCategories => {
            tokio::spawn(async move {
                let result = discord
                    .gifs_trending()
                    .await
                    .map(|trending| {
                        let categories = trending
                            .categories
                            .into_iter()
                            .map(|category| GifCategory {
                                name: category.name,
                                src: category.src,
                            })
                            .collect();
                        (categories, trending.preview.map(gif))
                    })
                    .map_err(|error| format!("Couldn't load GIFs: {error}"));
                let _ = events.send(Event::GifCategories(result));
            });
        }
        DiscordCommand::Gifs(query) => {
            tokio::spawn(async move {
                let found = match &query {
                    Some(query) => discord.gifs_search(query).await,
                    None => discord.gifs_trending_gifs().await,
                };
                let result = found
                    .map(|gifs| gifs.into_iter().map(gif).collect())
                    .map_err(|error| format!("Couldn't load GIFs: {error}"));
                let _ = events.send(Event::Gifs { query, result });
            });
        }
        DiscordCommand::GifSuggestions(query) => {
            tokio::spawn(async move {
                // As Discord's client asks: five, which show under the results.
                match discord.gifs_suggest(&query, 5).await {
                    Ok(terms) => {
                        let _ = events.send(Event::GifSuggestions { query, terms });
                    }
                    Err(error) => tracing::debug!(%error, "could not suggest GIF searches"),
                }
            });
        }
        DiscordCommand::GifPicked { id, query } => {
            tokio::spawn(async move {
                if let Err(error) = discord.gif_select(&id, &query).await {
                    tracing::debug!(%error, "could not report the GIF picked");
                }
            });
        }
        DiscordCommand::FavoriteGifs => {
            tokio::spawn(async move {
                if let Some(kept) = discord.kept_favorite_gifs().await {
                    let kept = kept.into_iter().map(favorite).collect();
                    let _ = events.send(Event::KeptFavoriteGifs(kept));
                }
                let result = discord
                    .favorite_gifs()
                    .await
                    .map(|favorites| favorites.into_iter().map(favorite).collect())
                    .map_err(|error| format!("Couldn't load your favourite GIFs: {error}"));
                let _ = events.send(Event::FavoriteGifs(result));
            });
        }
        DiscordCommand::SetFavoriteGifs(favorites) => {
            tokio::spawn(async move {
                let kept: Vec<yapfast_discord::gifs::Favorite> = favorites
                    .into_iter()
                    .map(|favorite| yapfast_discord::gifs::Favorite {
                        url: favorite.url,
                        src: favorite.src,
                        video: favorite.video,
                        width: favorite.width,
                        height: favorite.height,
                        order: favorite.order,
                    })
                    .collect();
                if let Err(error) = discord.set_favorite_gifs(&kept).await {
                    let _ = events.send(Event::Error(format!(
                        "Couldn't save your favourite GIFs: {error}"
                    )));
                }
            });
        }
        DiscordCommand::PrefetchFavorites(thumbnails) => {
            tokio::spawn(async move {
                // The favourites are exactly these, so the cache keeps theirs
                // and lets the others go.
                let urls: Vec<String> = thumbnails.iter().map(|(url, ..)| url.clone()).collect();
                let _ = tokio::task::spawn_blocking(move || {
                    yapfast_discord::cache::retain_gif_thumbnails(urls)
                })
                .await;
                // Four at a time: a thumbnail is a few hundred KB, and nobody
                // is waiting on it.
                let turns = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
                for (url, width, _height) in thumbnails {
                    let Ok(turn) = turns.clone().acquire_owned().await else {
                        return;
                    };
                    let discord = discord.clone();
                    tokio::spawn(async move {
                        let _turn = turn;
                        let known = url.clone();
                        let kept = tokio::task::spawn_blocking(move || {
                            yapfast_discord::cache::kept_gif(&known)
                        })
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                        if kept {
                            return;
                        }
                        if make_thumbnail(&discord, &url, width).await.is_some() {
                            tracing::debug!(%url, "prefetched a favourite GIF's thumbnail");
                        }
                    });
                }
            });
        }
        DiscordCommand::GifPicture {
            url,
            width,
            height,
            favorite,
        } => {
            tokio::spawn(async move {
                let picture = if favorite {
                    favorite_picture(&discord, &url, width, height).await
                } else {
                    picture(&discord, &url, width, height).await
                };
                let _ = events.send(Event::GifPicture {
                    url,
                    picture: picture.map(Arc::new),
                });
            });
        }
        _ => {}
    }
    None
}

/// The GIF picture at `url`, fit to `width` by `height`.
async fn picture(discord: &Discord, url: &str, width: u32, height: u32) -> Option<Picture> {
    match discord.download(url, MAX_GIF_BYTES).await {
        Ok(data) => {
            tokio::task::spawn_blocking(move || crate::chat::decode_picture(&data, width, height))
                .await
                .ok()
                .flatten()
        }
        Err(error) => {
            tracing::debug!(%error, %url, "could not download a GIF");
            None
        }
    }
}

/// A favourite GIF's picture, from the thumbnail kept of it, which this
/// makes the first time; its frames read from the file as they show.
async fn favorite_picture(
    discord: &Discord,
    url: &str,
    width: u32,
    height: u32,
) -> Option<Picture> {
    let lookup = url.to_owned();
    let kept = tokio::task::spawn_blocking(move || {
        yapfast_discord::cache::kept_gif(&lookup)
            .map(|kept| crate::chat::decode_picture_at(&kept, width, height))
    })
    .await
    .ok()?;
    if let Some(picture) = kept {
        return picture;
    }
    let made = make_thumbnail(discord, url, width).await?;
    tokio::task::spawn_blocking(move || crate::chat::decode_picture_at(&made, width, height))
        .await
        .ok()?
}

/// The most a favourite's picture downloads to make its thumbnail from:
/// Tenor's full-size GIFs took up to 24 MB.
const MAX_FAVORITE_BYTES: u64 = 40 * 1024 * 1024;
/// A favourite's picture this small, or a video, is its own thumbnail. A
/// small GIF keeps its transparency, which VP9 can't.
const KEPT_AS_IS: u64 = 64 * 1024;
/// A picture no thumbnail could be made of is kept if it's no larger.
const KEPT_UNMADE: u64 = 8 * 1024 * 1024;

/// Makes the thumbnail of the favourite whose picture is at `url`, for tiles
/// `width` wide, from Tenor's small MP4 when it's Tenor's, or from the
/// picture itself, which downloads to disk as it arrives.
async fn make_thumbnail(discord: &Discord, url: &str, width: u32) -> Option<PathBuf> {
    use std::io::Seek;
    let target = url.to_owned();
    let (download, mut file) =
        tokio::task::spawn_blocking(move || yapfast_discord::cache::gif_download(&target))
            .await
            .ok()??;
    let mut downloaded = false;
    for source in yapfast_discord::gifs::small_video(url)
        .into_iter()
        .chain([url.to_owned()])
    {
        if file.set_len(0).and_then(|()| file.rewind()).is_err() {
            break;
        }
        match discord
            .download_into(&source, &mut file, MAX_FAVORITE_BYTES)
            .await
        {
            Ok(_) => {
                downloaded = true;
                break;
            }
            Err(error) => tracing::debug!(%error, %source, "could not download a favourite GIF"),
        }
    }
    drop(file);
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || {
        let made = downloaded
            .then(|| thumbnail(&url, &download, width))
            .flatten();
        let _ = std::fs::remove_file(&download);
        made
    })
    .await
    .ok()?
}

/// Keeps the picture downloaded to `download` as the favourite's thumbnail:
/// itself when it's a video or small, and otherwise a VP9 WebM of it at
/// `width`, a tenth of a WebP's size.
fn thumbnail(url: &str, download: &std::path::Path, width: u32) -> Option<PathBuf> {
    use yapfast_discord::cache::{KeptGif, keep_gif};
    let size = std::fs::metadata(download).ok()?.len();
    let mut start = [0; 8];
    let video = std::io::Read::read_exact(&mut std::fs::File::open(download).ok()?, &mut start)
        .is_ok()
        && crate::video::is_video(&start);
    if video || size <= KEPT_AS_IS {
        return keep_gif(url, KeptGif::File(download));
    }
    let made = crate::chat::decode_picture_at(download, width, width.saturating_mul(4))
        .and_then(|picture| crate::clip::thumbnail(&picture));
    match made {
        Some(webm) => keep_gif(url, KeptGif::Bytes(&webm)),
        None if size <= KEPT_UNMADE => keep_gif(url, KeptGif::File(download)),
        None => None,
    }
}

/// A message's picture as a GIF to keep among the favourites, as Discord's
/// client offers it on any picture that moves: by the address it was linked
/// by, a GIF's page or the picture's own, or an attached file's address.
pub(crate) fn favorite_gif(attachment: &yapfast_discord::message::Attachment) -> Option<Gif> {
    // Messages kept before Yapfast knew what moves have the name to go by.
    let moves = attachment.animated
        || attachment.link_url.is_some()
        || attachment.name.to_lowercase().ends_with(".gif");
    if !attachment.image || !moves {
        return None;
    }
    let (width, height) = attachment.dimensions.unwrap_or_default();
    let url = attachment.link_url.as_deref().unwrap_or(&attachment.url);
    Some(Gif {
        id: None,
        url: favorite_key(url),
        src: favorite_key(&attachment.url),
        gif_src: String::new(),
        width,
        height,
    })
}

/// `url` as Discord's client keys a favourite by: an attached file's address
/// on Discord's CDN or media proxy without `ex`, `is` and `hm`, which sign it
/// for a while, and any other address as it is.
fn favorite_key(url: &str) -> String {
    let attached = url
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .is_some_and(|(host, path)| {
            ["cdn.discordapp.com", "media.discordapp.net"]
                .iter()
                .any(|known| discord_host(host, known))
                && ["attachments/", "ephemeral-attachments/"]
                    .iter()
                    .any(|kept| path.starts_with(kept))
        });
    let Some((path, query)) = url.split_once('?').filter(|_| attached) else {
        return url.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !matches!(pair.split('=').next(), Some("" | "ex" | "is" | "hm")))
        .collect();
    if kept.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

/// Whether `host` is `known` as Discord's client matches it: itself, a name
/// under it, or a numbered sibling such as `cdn-2.discordapp.com`.
fn discord_host(host: &str, known: &str) -> bool {
    if host == known
        || host
            .strip_suffix(known)
            .is_some_and(|start| start.ends_with('.'))
    {
        return true;
    }
    let (Some((label, domain)), Some((known_label, known_domain))) =
        (host.split_once('.'), known.split_once('.'))
    else {
        return false;
    };
    domain == known_domain
        && label
            .strip_prefix(known_label)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|rest| !rest.is_empty())
}

/// The favourites another client changed, as Yapfast shows them.
pub(super) fn favorites(favorites: &[yapfast_discord::gifs::Favorite]) -> Vec<FavoriteGif> {
    favorites.iter().cloned().map(favorite).collect()
}

fn gif(gif: yapfast_discord::gifs::Gif) -> Gif {
    Gif {
        id: gif.id,
        url: gif.url,
        src: gif.src,
        gif_src: gif.gif_src,
        width: gif.width,
        height: gif.height,
    }
}

fn favorite(favorite: yapfast_discord::gifs::Favorite) -> FavoriteGif {
    FavoriteGif {
        url: favorite.url,
        src: favorite.src,
        video: favorite.video,
        width: favorite.width,
        height: favorite.height,
        order: favorite.order,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GIF of noise, which compresses badly, so `frames` frames make a
    /// large file.
    fn noisy_gif(frames: u32) -> Vec<u8> {
        let mut seed = 7u32;
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            for _ in 0..frames {
                let picture = image::RgbaImage::from_fn(64, 48, |_, _| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let [r, g, b, _] = seed.to_le_bytes();
                    image::Rgba([r, g, b, 255])
                });
                encoder
                    .encode_frame(image::Frame::from_parts(
                        picture,
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(80, 1),
                    ))
                    .unwrap();
            }
        }
        data
    }

    #[test]
    fn stars_moving_pictures_by_the_address_discords_client_keeps_them_by() {
        // A favourite saved under a direct link to the file, sent from the
        // favourites: Discord unfurls it into an image embed of the link.
        let link = "https://static.klipy.com/ii/wave.webp";
        let message = yapfast_discord::message::parse(&serde_json::json!({
            "id": "1", "channel_id": "2", "content": link,
            "author": {"id": "3", "username": "alice"},
            "timestamp": "2026-10-15T10:00:00+00:00",
            "attachments": [
                {"id": "4", "filename": "spin.webp", "size": 1000, "width": 220, "height": 220,
                 "url": "https://cdn.discordapp.com/attachments/2/4/spin.webp?ex=1&is=2&hm=3&",
                 "proxy_url": "https://media.discordapp.net/attachments/2/4/spin.webp?ex=1&is=2&hm=3&",
                 "content_type": "image/webp", "flags": 32},
                {"id": "5", "filename": "still.png", "size": 1000, "width": 64, "height": 64,
                 "url": "https://cdn.discordapp.com/attachments/2/5/still.png",
                 "proxy_url": "https://media.discordapp.net/attachments/2/5/still.png",
                 "content_type": "image/png"},
            ],
            "embeds": [{
                "type": "image", "url": link,
                "thumbnail": {
                    "url": link,
                    "proxy_url": "https://images-ext-1.discordapp.net/external/x/https/static.klipy.com/ii/wave.webp",
                    "width": 220, "height": 165, "content_type": "image/webp",
                    "placeholder": "", "placeholder_version": 1, "flags": 32,
                },
            }],
        }))
        .unwrap();
        let stars: Vec<_> = message
            .attachments
            .iter()
            .map(|attachment| favorite_gif(attachment).map(|gif| gif.url))
            .collect();
        assert_eq!(
            stars,
            [
                // An animated WebP, by its address without the signature.
                Some("https://cdn.discordapp.com/attachments/2/4/spin.webp".to_owned()),
                // A still picture has no star.
                None,
                Some(link.to_owned()),
            ]
        );
        let linked = favorite_gif(&message.attachments[2]).unwrap();
        assert!(
            linked
                .src
                .starts_with("https://images-ext-1.discordapp.net/external/")
        );
        assert_eq!((linked.width, linked.height), (220, 165));

        // Only Discord's own files lose their query.
        assert_eq!(
            favorite_key(
                "https://media.discordapp.net/ephemeral-attachments/1/2/a.gif?ex=1&width=40"
            ),
            "https://media.discordapp.net/ephemeral-attachments/1/2/a.gif?width=40"
        );
        assert_eq!(
            favorite_key("https://cdn-2.discordapp.com/attachments/1/2/a.gif?hm=3"),
            "https://cdn-2.discordapp.com/attachments/1/2/a.gif"
        );
        for kept in [
            "https://example.com/attachments/a.gif?ex=1",
            "https://cdn.discordapp.com/emojis/1.gif?ex=1",
            "https://notcdn.discordapp.com/attachments/1/2/a.gif?ex=1",
        ] {
            assert_eq!(favorite_key(kept), kept);
        }
    }

    #[test]
    fn keeps_a_favourites_thumbnail_as_vp9_unless_small() {
        let directory = tempfile::tempdir().unwrap();
        yapfast_discord::cache::keep_in(directory.path().join("discord-account.json"));
        let download = |url: &str, data: &[u8]| {
            let (path, mut file) = yapfast_discord::cache::gif_download(url).unwrap();
            std::io::Write::write_all(&mut file, data).unwrap();
            path
        };

        // A large GIF becomes a WebM at the tiles' width, its frames timed
        // as they were.
        let large = noisy_gif(30);
        assert!(large.len() as u64 > KEPT_AS_IS);
        let url = "https://media.tenor.com/abc/large.gif";
        let kept = thumbnail(url, &download(url, &large), 32).unwrap();
        assert_eq!(yapfast_discord::cache::kept_gif(url), Some(kept.clone()));
        let webm = std::fs::read(&kept).unwrap();
        assert!(crate::video::is_video(&webm) && webm.len() < large.len() / 4);
        let picture = crate::chat::decode_picture_at(&kept, 32, 128).unwrap();
        assert_eq!((picture.width, picture.height), (32, 24));
        assert_eq!(picture.animation.as_ref().unwrap().delays, [80; 30]);
        assert_eq!(picture.frames().count(), 30);

        // A small one stays as it is.
        let small = noisy_gif(2);
        assert!(small.len() as u64 <= KEPT_AS_IS);
        let url = "https://media.tenor.com/abc/small.gif";
        let kept = thumbnail(url, &download(url, &small), 32).unwrap();
        assert_eq!(std::fs::read(kept).unwrap(), small);
    }
}
