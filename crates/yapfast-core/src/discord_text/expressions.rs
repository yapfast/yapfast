//! Emoji and Stickers in a Discord server's settings: listing them,
//! uploading, renaming and deleting them as Discord's client does, and
//! reading a picture chosen for a new one. See "Emoji" and "Stickers" in
//! docs/discord.md.

use std::path::Path;

use image::{ImageFormat, imageops::FilterType};
use yapfast_discord::{
    api,
    server_settings::{BannedUser, GuildEmoji, GuildSticker},
};

use super::{
    commands::NOT_DISCORD,
    own_profile::{picture, read_shaped},
    *,
};
use crate::chat::{
    EmojiAction, PickedPicture, Picture, Pixels, RoleMember, ServerEmoji, ServerSticker,
    StickerAction, StickerForm, file_size,
};

/// An emoji as Discord's client sends one: square, at most 128 pixels, and
/// under 256 KB; its preview at the size the upload dialog shows it.
const EMOJI: (u32, u32) = (128, 128);
const EMOJI_PREVIEW: (u32, u32) = (64, 64);
const EMOJI_LIMIT: usize = 256 * 1024;
/// A sticker: within 320 pixels and under 512 KB, with a preview at the
/// size Discord's upload dialog shows it.
const STICKER: u32 = 320;
const STICKER_PREVIEW: u32 = 160;
const STICKER_LIMIT: usize = 512 * 1024;
/// The largest file Yapfast reads for a sticker before it shrinks it.
const STICKER_FILE: u64 = 10 * 1024 * 1024;

/// Discord's codes for a server out of emoji slots, static or animated, and
/// for a picture too big.
const TOO_MANY_EMOJI: [u64; 2] = [30008, 30018];
const TOO_BIG: u64 = 50045;

/// Acts on Emoji or Stickers, or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let (server, work) = match command {
        Command::Discord(DiscordCommand::Emoji { server, action }) => (server, Ok(action)),
        Command::Discord(DiscordCommand::Stickers { server, action }) => (server, Err(*action)),
        other => return Some(other),
    };
    let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
        let why = Err(discord.err().unwrap_or(NOT_DISCORD).to_owned());
        let _ = events.send(match work {
            Ok(action) => Event::EmojisChanged {
                server,
                upload: matches!(action, EmojiAction::Upload { .. }),
                result: why,
                emojis: None,
            },
            Err(action) => Event::StickersChanged {
                server,
                form: !matches!(action, StickerAction::Delete(_)),
                result: why,
                stickers: None,
            },
        });
        return None;
    };
    let (discord, guild, events) = (discord.clone(), guild.to_owned(), events.clone());
    tokio::spawn(async move {
        let event = match work {
            Ok(action) => {
                let upload = matches!(action, EmojiAction::Upload { .. });
                let result = emoji(&discord, &guild, action).await;
                Event::EmojisChanged {
                    server,
                    upload,
                    result,
                    emojis: emojis(&discord, &guild).await.ok(),
                }
            }
            Err(action) => {
                let form = !matches!(action, StickerAction::Delete(_));
                let result = sticker(&discord, &guild, action).await;
                Event::StickersChanged {
                    server,
                    form,
                    result,
                    stickers: stickers(&discord, &guild).await.ok(),
                }
            }
        };
        let _ = events.send(event);
    });
    None
}

async fn emoji(discord: &Discord, guild: &str, action: EmojiAction) -> Result<(), String> {
    let result = match action {
        EmojiAction::Reload => return Ok(()),
        EmojiAction::Upload { name, picture: new } => {
            let image = picture(&Some(new));
            let image = image.as_str().unwrap_or_default();
            discord
                .stepping_up(|| discord.create_emoji(guild, &name, image))
                .await
                .map(drop)
        }
        EmojiAction::Rename { id, name } => {
            discord
                .stepping_up(|| discord.rename_emoji(guild, &id, &name))
                .await
        }
        EmojiAction::Delete(id) => {
            discord
                .stepping_up(|| discord.delete_emoji(guild, &id))
                .await
        }
    };
    result.map_err(|error| {
        tracing::info!(error = format!("{error:#}"), "Discord refused an emoji");
        match api::code(&error) {
            Some(code) if TOO_MANY_EMOJI.contains(&code) => {
                "This server does not have any open emoji slots!".to_owned()
            }
            Some(TOO_BIG) => too_big_emoji(),
            _ => format!("Oh no! Something went wrong. {error}"),
        }
    })
}

async fn sticker(discord: &Discord, guild: &str, action: StickerAction) -> Result<(), String> {
    let result = match action {
        StickerAction::Upload { form, file } => {
            let filename = match file.mime {
                "image/gif" => "sticker.gif",
                _ => "sticker.png",
            };
            let fields = [
                ("name", form.name.as_str()),
                ("tags", form.tags.as_str()),
                ("description", form.description.as_str()),
            ];
            discord
                .stepping_up(|| {
                    discord.create_sticker(guild, fields, (filename, file.mime, &file.data))
                })
                .await
        }
        StickerAction::Edit { id, form } => {
            discord
                .stepping_up(|| {
                    discord.edit_sticker(guild, &id, &form.name, &form.tags, &form.description)
                })
                .await
        }
        StickerAction::Delete(id) => {
            discord
                .stepping_up(|| discord.delete_sticker(guild, &id))
                .await
        }
    };
    // Discord's client shows Discord's words.
    result.map_err(|error| {
        tracing::info!(error = format!("{error:#}"), "Discord refused a sticker");
        error.to_string()
    })
}

/// A server's emoji, as Emoji lists them.
pub(super) async fn emojis(discord: &Discord, guild: &str) -> anyhow::Result<Vec<ServerEmoji>> {
    let emojis = discord.guild_emojis(guild).await?;
    Ok(emojis.into_iter().map(server_emoji).collect())
}

pub(super) async fn stickers(discord: &Discord, guild: &str) -> anyhow::Result<Vec<ServerSticker>> {
    let stickers = discord.guild_stickers(guild).await?;
    Ok(stickers.into_iter().map(server_sticker).collect())
}

fn server_emoji(emoji: GuildEmoji) -> ServerEmoji {
    let picture =
        yapfast_discord::ready::custom_emoji(&emoji.id, false, crate::chat::EMOJI_PICTURE_SIZE);
    ServerEmoji {
        picture,
        id: emoji.id,
        name: emoji.name,
        animated: emoji.animated,
        available: emoji.available,
        user: emoji.user.map(uploader),
    }
}

/// Discord's sticker format that isn't a picture.
const LOTTIE: u8 = 3;

pub(super) fn server_sticker(sticker: GuildSticker) -> ServerSticker {
    let picture = (sticker.format != LOTTIE)
        .then(|| yapfast_discord::ready::sticker(&sticker.id, sticker.format.into()));
    ServerSticker {
        picture,
        id: sticker.id,
        form: StickerForm {
            name: sticker.name,
            tags: sticker.tags,
            description: sticker.description,
        },
        format: sticker.format,
        available: sticker.available,
        user_id: sticker.user_id,
        user: sticker.user.map(uploader),
    }
}

/// Who uploaded an emoji or sticker: their name, and their username under
/// it when it differs.
fn uploader(user: BannedUser) -> RoleMember {
    let (name, username) = match user.global_name {
        Some(name) if name != user.username => (name, Some(user.username)),
        _ => (user.username, None),
    };
    RoleMember {
        id: user.id,
        name,
        username,
        avatar: user.avatar,
    }
}

fn too_big_emoji() -> String {
    format!(
        "That emoji was too big! Emoji must be under {} KB.",
        EMOJI_LIMIT / 1024
    )
}

/// The picture at `path` as a new emoji: its middle, square, at most 128
/// pixels; a GIF goes as it is, so it stays animated.
pub(super) fn read_emoji(path: &Path) -> Result<PickedPicture, String> {
    let picked = read_shaped(path, EMOJI, EMOJI_PREVIEW, "an emoji")?;
    if picked.data.len() > EMOJI_LIMIT {
        return Err(too_big_emoji());
    }
    Ok(picked)
}

/// The file at `path` as a new sticker, as Discord's client prepares one:
/// a GIF or an animated PNG goes as it is; a JPEG, or a PNG over 512 KB,
/// becomes a PNG within 320 pixels, keeping its shape. Over 512 KB after
/// that, it's refused.
pub(super) fn read_sticker(path: &Path) -> Result<PickedPicture, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unreadable = || format!("Yapfast could not read {name}.");
    let size = std::fs::metadata(path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(unreadable)?
        .len();
    let too_big = || {
        format!(
            "That sticker was too big! Stickers must be under {}.",
            file_size(STICKER_LIMIT as u64)
        )
    };
    if size > STICKER_FILE {
        return Err(too_big());
    }
    let data = std::fs::read(path).map_err(|_| unreadable())?;
    let unsupported = || "Unsupported file format".to_owned();
    let format = image::guess_format(&data)
        .ok()
        .filter(|format| {
            matches!(
                format,
                ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif
            )
        })
        .ok_or_else(unsupported)?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| {
        tracing::info!(%error, "could not decode a sticker");
        "Invalid sticker file".to_owned()
    })?;
    let small = decoded
        .thumbnail(STICKER_PREVIEW, STICKER_PREVIEW)
        .into_rgba8();
    let preview = Arc::new(Picture {
        width: small.width(),
        height: small.height(),
        rgba: Pixels::new(small.width(), small.height(), small.into_raw()),
        animation: None,
    });
    let animated = format == ImageFormat::Gif || animated_png(&data);
    let mime = if format == ImageFormat::Gif {
        "image/gif"
    } else {
        "image/png"
    };
    if animated || (format == ImageFormat::Png && data.len() <= STICKER_LIMIT) {
        if data.len() > STICKER_LIMIT {
            return Err(too_big());
        }
        return Ok(PickedPicture {
            mime,
            data: data.into(),
            preview,
        });
    }
    drop(data);
    let fitted = decoded.resize(STICKER, STICKER, FilterType::Lanczos3);
    drop(decoded);
    let mut file = std::io::Cursor::new(Vec::new());
    fitted
        .write_to(&mut file, ImageFormat::Png)
        .map_err(|_| "Invalid sticker file".to_owned())?;
    let data = file.into_inner();
    if data.len() > STICKER_LIMIT {
        return Err(too_big());
    }
    Ok(PickedPicture {
        mime: "image/png",
        data: data.into(),
        preview,
    })
}

/// Whether a PNG is animated: an `acTL` chunk before its first `IDAT`, as
/// Discord's client checks.
fn animated_png(data: &[u8]) -> bool {
    let mut at = 8;
    while let Some(header) = data.get(at..at + 8) {
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        match &header[4..8] {
            b"acTL" => return true,
            b"IDAT" => return false,
            _ => at += 12 + length,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_animated_png_has_actl_before_idat() {
        let chunk = |kind: &[u8], body: &[u8]| {
            let mut chunk = (body.len() as u32).to_be_bytes().to_vec();
            chunk.extend_from_slice(kind);
            chunk.extend_from_slice(body);
            chunk.extend_from_slice(&[0; 4]);
            chunk
        };
        let png = |chunks: &[Vec<u8>]| {
            let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
            for chunk in chunks {
                png.extend_from_slice(chunk);
            }
            png
        };
        let header = chunk(b"IHDR", &[0; 13]);
        let actl = chunk(b"acTL", &[0; 8]);
        let idat = chunk(b"IDAT", &[1, 2, 3]);
        assert!(animated_png(&png(&[
            header.clone(),
            actl.clone(),
            idat.clone()
        ])));
        assert!(!animated_png(&png(&[header.clone(), idat.clone(), actl])));
        assert!(!animated_png(&png(&[header])));
    }
}
