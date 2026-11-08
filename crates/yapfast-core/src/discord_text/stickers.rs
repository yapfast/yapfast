//! The sticker picker's commands: Discord's default packs, the pictures the
//! picker shows, and a sticker clicked.

use super::*;
use crate::chat::{Sticker, StickerCollection};

/// The largest sticker picture the picker downloads: a sticker is a PNG,
/// APNG or GIF under 512 KiB, or a Lottie animation of JSON under 2 MiB.
const MAX_STICKER_BYTES: usize = 4 * 1024 * 1024;

/// Carries out a sticker picker command, or gives back any other.
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
            DiscordCommand::StickerPacks
            | DiscordCommand::StickerPicture { .. }
            | DiscordCommand::SendSticker { .. },
            Ok(discord),
        ) => discord.clone(),
        (DiscordCommand::StickerPacks, Err(why)) => {
            let _ = events.send(Event::StickerPacks(Err(why.into())));
            return None;
        }
        (DiscordCommand::SendSticker { room, .. }, Err(why)) => {
            let _ = events.send(Event::StickerSent {
                room: room.clone(),
                result: Err(why.into()),
            });
            return None;
        }
        (DiscordCommand::StickerPicture { .. }, Err(_)) => return None,
        _ => return Some(Command::Discord(command)),
    };
    let events = events.clone();
    match command {
        DiscordCommand::StickerPacks => {
            tokio::spawn(async move {
                // What Yapfast kept shows at once, whether or not it is a
                // day old: Discord is asked only once it is.
                let mut ask = true;
                if let Some(kept) = discord.kept_sticker_packs().await {
                    ask = !yapfast_discord::cache::sticker_packs_are_fresh(kept.fetched);
                    let _ = events.send(Event::KeptStickerPacks(
                        kept.packs.into_iter().map(collection).collect(),
                    ));
                }
                if !ask {
                    return;
                }
                let result = discord
                    .sticker_packs()
                    .await
                    .map(|packs| packs.into_iter().map(collection).collect())
                    .map_err(|error| format!("Couldn't load the stickers: {error}"));
                let _ = events.send(Event::StickerPacks(result));
            });
        }
        DiscordCommand::StickerPicture { url } => {
            tokio::spawn(async move {
                let picture = sticker_picture(&discord, &url).await;
                let _ = events.send(Event::StickerPicture {
                    url,
                    picture: picture.map(Arc::new),
                });
            });
        }
        DiscordCommand::SendSticker { room, id } => {
            tokio::spawn(async move {
                let result = match discord::parse_room(&room) {
                    Some((guild, channel)) => discord
                        .send_sticker(guild, channel, &id)
                        .await
                        .map_err(|error| format!("Couldn't send the sticker: {error}")),
                    None => Err("Stickers aren't on Matrix.".into()),
                };
                let _ = events.send(Event::StickerSent { room, result });
            });
        }
        _ => {}
    }
    None
}

/// A sticker as the picker sends and names it.
fn sticker(sticker: yapfast_discord::stickers::Sticker) -> Sticker {
    Sticker {
        picture: sticker.picture(),
        id: sticker.id,
        name: sticker.name,
        format: sticker.format,
        usable: true,
    }
}

/// A pack as the picker lists it.
fn collection(collection: yapfast_discord::stickers::Collection) -> StickerCollection {
    StickerCollection {
        id: collection.id,
        name: collection.name,
        stickers: collection.stickers.into_iter().map(sticker).collect(),
    }
}

/// The sticker picture at `url`, at the size the picker's tiles show it: a
/// Lottie sticker drawn as it animates, and any other as a picture. The
/// download finds it kept on disk when the start fetched it ahead.
async fn sticker_picture(discord: &Discord, url: &str) -> Option<Picture> {
    let data = match discord.download(url, MAX_STICKER_BYTES).await {
        Ok(data) => data,
        Err(error) => {
            tracing::debug!(%error, %url, "could not download a sticker");
            return None;
        }
    };
    let lottie = yapfast_discord::ready::lottie_sticker(url);
    let size = yapfast_discord::stickers::TILE_SIZE;
    tokio::task::spawn_blocking(move || {
        // Only a Lottie sticker's address reaches ThorVG's parser.
        if lottie {
            crate::chat::decode_lottie(&data, size, size)
        } else {
            crate::chat::decode_picture(&data, size, size)
        }
    })
    .await
    .ok()
    .flatten()
}
