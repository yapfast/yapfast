//! Extras: one use of each ability in the second batch.
//!
//! - `link_open` (`links.offer`): a Spotify or Steam store link offers to open
//!   in its app, beside the leaving dialog's Visit site.
//! - `switcher_rank` (`switcher.rank`): direct messages come first in the
//!   quick switcher, each group in Yapfast's own order.
//! - `startup_timing` (`app.timing`) and `window.set_title` (`window.title`):
//!   the title says how long this start took to be ready.
//! - `relationship_*` (`relationships.read`): a toast when a friend is added,
//!   goes, or a request arrives, which is what RelationshipNotifier does.
//! - `timeline.intro` replaced (`ui.modify`) with a picture from the web
//!   (`images.web`, from the host in `network`), and two channel-menu rows
//!   renamed and moved, with the message gap and the channel rows' height,
//!   all from the manifest.

use yapfast_pdk::json::json;
use yapfast_pdk::prelude::*;

#[derive(Default)]
struct Extras;

/// The app address a link opens in, for the two stores this knows.
fn app_link(url: &str) -> Option<(&'static str, String)> {
    let path = url.split(['?', '#']).next()?;
    if let Some(rest) = path.strip_prefix("https://open.spotify.com/") {
        let mut parts = rest.split('/').filter(|part| !part.is_empty());
        let mut kind = parts.next()?;
        // Spotify puts a locale first in some links: `intl-de/track/…`.
        if kind.starts_with("intl-") {
            kind = parts.next()?;
        }
        let id = parts.next()?;
        let known = ["track", "album", "playlist", "artist", "episode", "show"];
        if !known.contains(&kind) || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return None;
        }
        return Some(("Open in Spotify", format!("spotify:{kind}:{id}")));
    }
    if let Some(rest) = path.strip_prefix("https://store.steampowered.com/app/") {
        let id = rest.split('/').next()?;
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        return Some(("Open in Steam", format!("steam://store/{id}")));
    }
    None
}

impl Plugin for Extras {
    fn hook(&mut self, _ctx: &Context<'_>, hook: HookRequest) -> Result<HookOutcome> {
        if let Some(link) = hook.link() {
            return Ok(match app_link(&link.url) {
                Some((label, uri)) => HookOutcome::offer(link, label, uri),
                None => HookOutcome::unchanged(),
            });
        }
        if let Some(found) = hook.switcher() {
            let dms = found.places.iter().filter(|place| place.kind == "dm");
            let rest = found.places.iter().filter(|place| place.kind != "dm");
            let order: Vec<&str> = dms.chain(rest).map(|place| place.id.as_str()).collect();
            return Ok(HookOutcome::ranked(found, &order));
        }
        Ok(HookOutcome::unchanged())
    }

    fn event(&mut self, _ctx: &Context<'_>, event: Event) -> Result<()> {
        let envelope = event.envelope();
        let person = envelope.payload["person"].as_str().unwrap_or("someone");
        let toast = |words: String| call::post("ui.toast", json!({ "text": words }));
        match event.kind().as_str() {
            "startup_timing" => {
                if let Some(ready) = envelope.payload["ready_ms"].as_u64() {
                    let seconds = ready as f64 / 1000.0;
                    call::post(
                        "window.set_title",
                        json!({ "suffix": format!("ready in {seconds:.1} s") }),
                    )?;
                }
            }
            "relationship_add" => toast(format!("{person} is your friend now"))?,
            "relationship_remove" if envelope.payload["was"] == "friend" => {
                toast(format!("{person} is no longer your friend"))?;
            }
            "relationship_request" if envelope.payload["incoming"] == true => {
                toast(format!("{person} sent you a friend request"))?;
            }
            "room_selected" => {
                // The welcome names the channel, as Yapfast's own does.
                let heading = match envelope.payload["name"].as_str() {
                    Some(name) if !name.is_empty() => format!("Welcome to #{name}"),
                    _ => "Welcome".to_owned(),
                };
                call::post(
                    "ui.set_slot",
                    json!({
                        "slot": "timeline.intro",
                        "tree": { "rows": [
                            { "cells": [{ "kind": "heading", "text": heading }] },
                            { "cells": [{
                                "kind": "image",
                                "src": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/47/PNG_transparency_demonstration_1.png/330px-PNG_transparency_demonstration_1.png",
                                "text": "Dice, from Wikimedia Commons"
                            }] },
                            { "cells": [{ "kind": "text", "text": "This welcome is the Extras plugin's, in place of Yapfast's." }] }
                        ] }
                    }),
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}

yapfast_plugin!(Extras);

#[cfg(test)]
mod tests {
    use super::app_link;

    #[test]
    fn store_links_open_in_their_apps_and_nothing_else_does() {
        assert_eq!(
            app_link("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=x"),
            Some((
                "Open in Spotify",
                "spotify:track:4uLU6hMCjMI75M1A2tKUQC".to_owned()
            ))
        );
        assert_eq!(
            app_link("https://open.spotify.com/intl-de/album/1A2b3C"),
            Some(("Open in Spotify", "spotify:album:1A2b3C".to_owned()))
        );
        assert_eq!(
            app_link("https://store.steampowered.com/app/620/Portal_2/"),
            Some(("Open in Steam", "steam://store/620".to_owned()))
        );
        assert_eq!(app_link("https://open.spotify.com/user/x/y"), None);
        assert_eq!(app_link("https://example.com/track/1"), None);
    }
}
