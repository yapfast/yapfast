//! `media.get`: the bytes of a picture Yapfast already keeps.
//!
//! §17.2 of the plan asks for an attachment's, an avatar's or an emoji's bytes
//! by URL, capped, and with no token: a plugin reads a text attachment or
//! transforms an image without ever holding what Yapfast signs its requests
//! with.
//!
//! The bytes come from the picture store and from nowhere else. That is the
//! whole security argument: a plugin can read what the conversation on screen
//! has already shown, and cannot make Yapfast fetch a URL on its behalf, which
//! would be a request the network gate never saw and the manifest never
//! declared. A URL the store doesn't hold is refused with a sentence saying so.
//!
//! One answer is one frame, so a picture larger than a frame is refused **by
//! name** and not cut: a plugin handed half of a PNG would fail in a way that
//! says nothing about why.

use base64::Engine;
use yapfast_plugin_api::call::HostReturn;
use yapfast_plugin_api::frame::MAX_FRAME;

/// The most bytes one `media.get` answers with, once base64 is taken into
/// account: base64 costs a third more, and the answer has a few fields of its
/// own.
pub const MAX_BYTES: u64 = (MAX_FRAME as u64 - 4096) / 4 * 3;

/// Where the bytes of a picture come from: the application's picture store,
/// which is the Discord client's disk cache.
pub trait Pictures: Send + Sync {
    /// The bytes kept for `url`, or nothing when the store doesn't hold it.
    fn bytes(&self, url: &str) -> Option<Vec<u8>>;
}

/// The picture store as Yapfast keeps it: the Discord client's disk cache, which
/// holds what a conversation, a member list or a profile has already shown.
///
/// It is read here and never filled here. A URL the store doesn't hold is a miss
/// and not a request: asking Discord for it would be a fetch the network gate
/// never saw.
#[cfg(feature = "discord")]
pub struct Disk;

#[cfg(feature = "discord")]
impl Pictures for Disk {
    fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        use std::io::Read;
        let kept = yapfast_discord::cache::kept_picture(url)?;
        let mut reader = kept.open().ok()?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }
}

/// What `media.get` answers for one URL.
pub fn answer(pictures: &dyn Pictures, url: &str) -> HostReturn {
    if url.is_empty() || url.len() > yapfast_plugin_api::identity::MAX_URL {
        return HostReturn::Error {
            message: "`media.get` takes the address of an attachment, an avatar or an \
                      emoji, and this is not one"
                .to_owned(),
        };
    }
    let Some(bytes) = pictures.bytes(url) else {
        return HostReturn::Error {
            message: format!(
                "no picture is kept for `{url}`: `media.get` reads what a conversation \
                 has already shown, and never asks the network for anything"
            ),
        };
    };
    if bytes.len() as u64 > MAX_BYTES {
        return HostReturn::Denied {
            reason: format!(
                "`{url}` is {} bytes, and one answer carries at most {MAX_BYTES}: \
                 read a smaller picture, or ask for one the conversation has shown",
                bytes.len()
            ),
        };
    }
    let length = bytes.len();
    HostReturn::Ok(serde_json::json!({
        "bytes": length,
        "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Kept(Vec<u8>);

    impl Pictures for Kept {
        fn bytes(&self, url: &str) -> Option<Vec<u8>> {
            (url == "https://cdn.example.test/a.png").then(|| self.0.clone())
        }
    }

    #[test]
    fn a_kept_picture_comes_back_as_its_bytes() {
        let store = Kept(b"hello".to_vec());
        let answer = answer(&store, "https://cdn.example.test/a.png");
        let HostReturn::Ok(value) = answer else {
            panic!("a kept picture is an answer: {answer:?}");
        };
        assert_eq!(value["bytes"], 5);
        assert_eq!(value["bytes_base64"], "aGVsbG8=");
    }

    #[test]
    fn a_picture_the_store_doesnt_hold_is_refused_and_says_why() {
        let store = Kept(b"hello".to_vec());
        let answer = answer(&store, "https://cdn.example.test/b.png");
        let HostReturn::Error { message } = answer else {
            panic!("a picture nobody keeps is an error: {answer:?}");
        };
        assert!(message.contains("never asks the network"), "{message}");
    }

    #[test]
    fn a_picture_too_large_for_a_frame_is_refused_by_name_not_cut() {
        let store = Kept(vec![0; (MAX_BYTES + 1) as usize]);
        let answer = answer(&store, "https://cdn.example.test/a.png");
        let HostReturn::Denied { reason } = answer else {
            panic!("a picture over the cap is refused: {answer:?}");
        };
        assert!(reason.contains(&MAX_BYTES.to_string()), "{reason}");
    }

    #[test]
    fn an_address_that_isnt_one_is_an_error() {
        let store = Kept(Vec::new());
        assert!(matches!(answer(&store, ""), HostReturn::Error { .. }));
        assert!(matches!(
            answer(&store, &"https://example.test/".repeat(200)),
            HostReturn::Error { .. }
        ));
    }
}
