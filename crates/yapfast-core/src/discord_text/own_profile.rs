//! This account's own profile, in a Discord server or its main one:
//! fetching it for its editor, reading a picture chosen for it as Discord's
//! client sends one, and saving it. See "Editing one's profile" in
//! docs/discord/people.md.

use std::path::Path;

use base64::Engine;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use serde_json::{Map, Value};
use yapfast_discord::profile::PICTURE_LIMIT;

use super::{commands::NOT_DISCORD, *};
use crate::chat::{OwnProfile, PickedPicture, Pixels, ProfileChange, ProfilePicture, file_size};

/// The size Discord's client crops a picture to before it sends it: a
/// square avatar, and a banner 17 by 6. One smaller goes at its own size.
const AVATAR: (u32, u32) = (1024, 1024);
const BANNER: (u32, u32) = (2400, 848);
/// The preview's size: as the editor's card gets everyone's pictures, a
/// 128px avatar for its 120px one, and a banner 600 wide.
const AVATAR_PREVIEW: (u32, u32) = (128, 128);
const BANNER_PREVIEW: (u32, u32) = (600, 212);

/// Carries out a command about this account's own profile, or gives back
/// any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    let events = events.clone();
    match command {
        DiscordCommand::OwnProfile { server } => {
            let guild = guild(server.as_deref());
            let (Ok(discord), Some(guild)) = (discord, guild) else {
                let why = discord.err().unwrap_or(NOT_DISCORD);
                let _ = events.send(Event::OwnProfile {
                    server,
                    result: Err(why.into()),
                });
                return None;
            };
            let discord = discord.clone();
            tokio::spawn(async move {
                let result = match discord.own_profile(guild.as_deref()).await {
                    Ok(profile) => Ok(own_profile(&discord, guild.as_deref(), profile)),
                    Err(error) => {
                        tracing::warn!(
                            error = format!("{error:#}"),
                            "could not fetch this account's profile"
                        );
                        Err(format!("Discord didn't send your profile: {error}"))
                    }
                };
                let _ = events.send(Event::OwnProfile { server, result });
            });
        }
        DiscordCommand::ProfilePicture { kind, path } => {
            tokio::task::spawn_blocking(move || {
                let result = read_picture(&path, kind);
                let _ = events.send(Event::ProfilePicture { kind, path, result });
            });
        }
        DiscordCommand::SaveProfile { server, change } => {
            let guild = guild(server.as_deref());
            let (Ok(discord), Some(guild)) = (discord, guild) else {
                let why = discord.err().unwrap_or(NOT_DISCORD);
                let _ = events.send(Event::ProfileSaved {
                    server,
                    result: Err(why.into()),
                });
                return None;
            };
            let discord = discord.clone();
            tokio::spawn(async move {
                let result = match &guild {
                    Some(guild) => {
                        let (member, profile) = server_bodies(&change);
                        drop(change);
                        discord.save_server_profile(guild, &member, &profile).await
                    }
                    None => {
                        let (user, profile) = main_bodies(&change);
                        drop(change);
                        discord.save_main_profile(&user, &profile).await
                    }
                };
                let result = result.map_err(|error| {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not save this account's profile"
                    );
                    match guild {
                        Some(_) => {
                            format!("There was a problem updating your server profile: {error}")
                        }
                        None => format!("There was a problem updating your profile: {error}"),
                    }
                });
                let _ = events.send(Event::ProfileSaved { server, result });
            });
        }
        other => return Some(Command::Discord(other)),
    }
    None
}

/// The Discord server a profile is in, by its server id: `Some(None)` for
/// the main profile, and none for an id that isn't a server's.
fn guild(server: Option<&str>) -> Option<Option<String>> {
    match server {
        Some(server) => discord::parse_server(server).map(|guild| Some(guild.to_owned())),
        None => Some(None),
    }
}

/// The profile as Yapfast shows it, its fallback's bio's mentions written
/// as names.
fn own_profile(
    discord: &Discord,
    guild: Option<&str>,
    profile: yapfast_discord::profile::OwnProfile,
) -> OwnProfile {
    OwnProfile {
        name: profile.name,
        pronouns: profile.pronouns,
        bio: profile.bio,
        avatar: profile.avatar,
        banner: profile.banner,
        accent: profile.accent,
        fallback: render::chat_profile(discord, guild.unwrap_or_default(), profile.fallback),
    }
}

/// A picture as Discord's client sends it: a data URI, or null to go by
/// the fallback's.
pub(super) fn picture(picture: &Option<PickedPicture>) -> Value {
    match picture {
        Some(picture) => Value::String(format!(
            "data:{};base64,{}",
            picture.mime,
            base64::engine::general_purpose::STANDARD.encode(&picture.data)
        )),
        None => Value::Null,
    }
}

/// What Discord's client sends for a change in a server: the member's
/// fields, its nickname and avatar, for `PATCH /guilds/{id}/members/@me`,
/// and the profile's, its banner, bio and pronouns, for `PATCH
/// /guilds/{id}/profile/@me`. An empty text goes by the main profile's.
fn server_bodies(change: &ProfileChange) -> (Value, Value) {
    let (mut member, mut profile) = (Map::new(), Map::new());
    if let Some(nick) = &change.name {
        member.insert("nick".into(), nick.as_str().into());
    }
    if let Some(avatar) = &change.avatar {
        member.insert("avatar".into(), picture(avatar));
    }
    if let Some(banner) = &change.banner {
        profile.insert("banner".into(), picture(banner));
    }
    if let Some(bio) = &change.bio {
        profile.insert("bio".into(), bio.as_str().into());
    }
    if let Some(pronouns) = &change.pronouns {
        profile.insert("pronouns".into(), pronouns.as_str().into());
    }
    (Value::Object(member), Value::Object(profile))
}

/// What Discord's client sends for a change to the main profile: the
/// user's fields, its display name and avatar, for `PATCH /users/@me`, and
/// the profile's, its bio, pronouns, banner and banner colour, for `PATCH
/// /users/@me/profile`. An empty display name goes by the username.
fn main_bodies(change: &ProfileChange) -> (Value, Value) {
    let (mut user, mut profile) = (Map::new(), Map::new());
    if let Some(name) = &change.name {
        user.insert("global_name".into(), name.as_str().into());
    }
    if let Some(avatar) = &change.avatar {
        user.insert("avatar".into(), picture(avatar));
    }
    if let Some(bio) = &change.bio {
        profile.insert("bio".into(), bio.as_str().into());
    }
    if let Some(pronouns) = &change.pronouns {
        profile.insert("pronouns".into(), pronouns.as_str().into());
    }
    if let Some(banner) = &change.banner {
        profile.insert("banner".into(), picture(banner));
    }
    if let Some(accent) = change.accent {
        profile.insert("accent_color".into(), accent.into());
    }
    (Value::Object(user), Value::Object(profile))
}

/// The picture at `path` as Discord's client sends it for a profile: cropped
/// from its middle to a square avatar or a 17 by 6 banner, at most
/// [`AVATAR`] or [`BANNER`], with a preview at the editor's size. Only the
/// file to send and the preview stay; the decoded picture goes. An animated
/// GIF goes as it is, so it stays animated.
pub(super) fn read_picture(path: &Path, kind: ProfilePicture) -> Result<PickedPicture, String> {
    let (largest, preview) = match kind {
        ProfilePicture::Avatar => (AVATAR, AVATAR_PREVIEW),
        ProfilePicture::Banner => (BANNER, BANNER_PREVIEW),
    };
    read_shaped(path, largest, preview, "a profile picture")
}

/// The picture at `path` as Discord's client sends it: cropped from its
/// middle to the shape of `largest`, and at most that, with a preview of
/// `preview`'s size. `what` names it in an error, such as "a profile
/// picture".
pub(super) fn read_shaped(
    path: &Path,
    largest: (u32, u32),
    preview: (u32, u32),
    what: &str,
) -> Result<PickedPicture, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = std::fs::metadata(path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(|| format!("Yapfast could not read {name}."))?
        .len();
    if size > PICTURE_LIMIT {
        return Err(format!(
            "{name} is {}; {what} can be up to {}.",
            file_size(size),
            file_size(PICTURE_LIMIT)
        ));
    }
    let data = std::fs::read(path).map_err(|_| format!("Yapfast could not read {name}."))?;
    let not_a_picture = || format!("{name} isn't a PNG, JPG, GIF or WebP picture.");
    let format = image::guess_format(&data)
        .ok()
        .filter(|format| {
            matches!(
                format,
                ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
            )
        })
        .ok_or_else(not_a_picture)?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| {
        tracing::info!(%error, "could not decode {what}");
        not_a_picture()
    })?;
    let cropped = crop(&decoded, largest);
    drop(decoded);
    let small = cropped.thumbnail_exact(preview.0, preview.1).into_rgba8();
    let preview = Arc::new(Picture {
        width: small.width(),
        height: small.height(),
        rgba: Pixels::new(small.width(), small.height(), small.into_raw()),
        animation: None,
    });
    if format == ImageFormat::Gif {
        return Ok(PickedPicture {
            mime: "image/gif",
            data: data.into(),
            preview,
        });
    }
    drop(data);
    // A picture with transparency stays a PNG; a photo goes as a JPEG,
    // a fraction of the size.
    let mut file = std::io::Cursor::new(Vec::new());
    let mime = if cropped.color().has_alpha() {
        cropped
            .write_to(&mut file, ImageFormat::Png)
            .map_err(|_| not_a_picture())?;
        "image/png"
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 90)
            .encode_image(&cropped.into_rgb8())
            .map_err(|_| not_a_picture())?;
        "image/jpeg"
    };
    Ok(PickedPicture {
        mime,
        data: file.into_inner().into(),
        preview,
    })
}

/// The middle of `picture` in the shape of `largest`, shrunk to it if it's
/// larger.
fn crop(picture: &DynamicImage, largest: (u32, u32)) -> DynamicImage {
    let (width, height) = picture.dimensions();
    // The widest part of the shape that fits, as whole pixels.
    let (shape_width, shape_height) = largest;
    let (crop_width, crop_height) = if u64::from(width) * u64::from(shape_height)
        > u64::from(height) * u64::from(shape_width)
    {
        let crop_width =
            (u64::from(height) * u64::from(shape_width) / u64::from(shape_height)) as u32;
        (crop_width.max(1), height)
    } else {
        let crop_height =
            (u64::from(width) * u64::from(shape_height) / u64::from(shape_width)) as u32;
        (width, crop_height.max(1))
    };
    let cropped = picture.crop_imm(
        (width - crop_width) / 2,
        (height - crop_height) / 2,
        crop_width,
        crop_height,
    );
    if crop_width > shape_width {
        cropped.resize_exact(shape_width, shape_height, FilterType::Lanczos3)
    } else {
        cropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` by `height` picture saved as `format` in a new directory.
    fn saved(width: u32, height: u32, alpha: bool, format: ImageFormat) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let picture = if alpha {
            DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                width,
                height,
                image::Rgba([200, 100, 50, 128]),
            ))
        } else {
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                width,
                height,
                image::Rgb([200, 100, 50]),
            ))
        };
        picture
            .save_with_format(dir.path().join("picture"), format)
            .unwrap();
        dir
    }

    #[test]
    fn a_picture_is_cropped_and_sized_as_discords_client_sends_it() {
        let dir = saved(3000, 1000, false, ImageFormat::Jpeg);
        let avatar = read_picture(&dir.path().join("picture"), ProfilePicture::Avatar).unwrap();
        assert_eq!(avatar.mime, "image/jpeg");
        let sent = image::load_from_memory(&avatar.data).unwrap();
        assert_eq!(
            sent.dimensions(),
            (1000, 1000),
            "the middle square, not enlarged"
        );
        assert_eq!(
            (avatar.preview.width, avatar.preview.height),
            AVATAR_PREVIEW
        );

        let banner = read_picture(&dir.path().join("picture"), ProfilePicture::Banner).unwrap();
        let sent = image::load_from_memory(&banner.data).unwrap();
        assert_eq!(
            sent.dimensions(),
            BANNER,
            "17 by 6, shrunk to Discord's largest"
        );
        assert_eq!(
            (banner.preview.width, banner.preview.height),
            BANNER_PREVIEW
        );

        let dir = saved(64, 64, true, ImageFormat::Png);
        let avatar = read_picture(&dir.path().join("picture"), ProfilePicture::Avatar).unwrap();
        assert_eq!(avatar.mime, "image/png", "transparency stays");
    }

    #[test]
    fn what_isnt_a_picture_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "hello").unwrap();
        assert_eq!(
            read_picture(&path, ProfilePicture::Avatar).unwrap_err(),
            "notes.txt isn't a PNG, JPG, GIF or WebP picture."
        );
        assert!(read_picture(&dir.path().join("gone.png"), ProfilePicture::Avatar).is_err());
    }

    #[test]
    fn a_change_goes_where_discords_client_sends_it() {
        let picture = PickedPicture {
            mime: "image/png",
            data: Arc::from(&b"png"[..]),
            preview: Arc::default(),
        };
        let change = ProfileChange {
            name: Some(String::new()),
            pronouns: Some("they/them".into()),
            avatar: Some(Some(picture)),
            banner: Some(None),
            ..Default::default()
        };
        let (member, profile) = server_bodies(&change);
        assert_eq!(
            member,
            serde_json::json!({"nick": "", "avatar": "data:image/png;base64,cG5n"})
        );
        assert_eq!(
            profile,
            serde_json::json!({"banner": null, "pronouns": "they/them"})
        );

        let change = ProfileChange {
            accent: Some(Some(0x5865f2)),
            bio: Some("Hi".into()),
            ..change
        };
        let (user, profile) = main_bodies(&change);
        assert_eq!(
            user,
            serde_json::json!({"global_name": "", "avatar": "data:image/png;base64,cG5n"})
        );
        assert_eq!(
            profile,
            serde_json::json!({"bio": "Hi", "pronouns": "they/them", "banner": null, "accent_color": 5_793_266})
        );
        let (_, profile) = main_bodies(&ProfileChange {
            accent: Some(None),
            ..Default::default()
        });
        assert_eq!(profile, serde_json::json!({"accent_color": null}));
    }
}
