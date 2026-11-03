//! A Discord gift, such as Nitro, that a clicked gift link opens. The
//! gift is looked up and shown first; redeeming it waits for the click,
//! as joining a server by an invite does, and a gift this account
//! redeemed, or anyone used up, offers nothing to redeem. A gift only
//! Discord can redeem, one that needs a payment source or is from a
//! promotion, offers its link in the browser instead.

use super::*;
use crate::chat::GiftPreview;

/// The gift dialog, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GiftDialog {
    /// The code of the gift looked up or shown.
    pub code: String,
    /// What the gift shows, once Discord answered.
    pub preview: Option<GiftPreview>,
    /// The room its link was opened in, which redeeming names.
    pub room: Option<String>,
    pub resolving: bool,
    /// This account redeemed the gift, from this dialog or one closed
    /// while it was redeeming.
    pub redeemed: bool,
    pub error: Option<String>,
}

impl Application {
    /// Opens the dialog for a clicked gift link, by its code, and looks
    /// the gift up.
    pub(super) fn open_gift_link(&mut self, code: String) -> Vec<Effect> {
        if !self.state.discord_online && !self.state.preview {
            return vec![];
        }
        self.state.gift = Some(GiftDialog {
            code: code.clone(),
            room: self.state.selected.clone(),
            resolving: true,
            ..GiftDialog::default()
        });
        if self.state.preview {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ResolveGift { code },
        ))]
    }

    pub(super) fn gift_resolved(
        &mut self,
        code: String,
        result: Result<GiftPreview, String>,
    ) -> Vec<Effect> {
        let Some(gift) = self
            .state
            .gift
            .as_mut()
            .filter(|gift| gift.resolving && gift.code == code)
        else {
            return vec![];
        };
        gift.resolving = false;
        match result {
            Ok(preview) => gift.preview = Some(preview),
            Err(error) => gift.error = Some(error),
        }
        vec![]
    }

    /// Redeems the gift the dialog shows, when it still can be.
    pub(super) fn redeem_gift(&mut self) -> Vec<Effect> {
        let Some(gift) = self.state.gift.as_mut() else {
            return vec![];
        };
        if gift.redeemed
            || self.state.preview
            || !gift.preview.as_ref().is_some_and(GiftPreview::redeemable)
            || self.state.redeeming_gifts.contains(&gift.code)
        {
            return vec![];
        }
        gift.error = None;
        self.state.redeeming_gifts.push(gift.code.clone());
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::RedeemGift {
                code: gift.code.clone(),
                room: gift.room.clone(),
            },
        ))]
    }

    pub(super) fn gift_redeemed(
        &mut self,
        code: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let before = self.state.redeeming_gifts.len();
        self.state.redeeming_gifts.retain(|gift| *gift != code);
        if self.state.redeeming_gifts.len() == before {
            return vec![];
        }
        // The dialog shows the answer, whether it stayed open or was
        // opened again by the same link.
        let Some(gift) = self.state.gift.as_mut().filter(|gift| gift.code == code) else {
            return vec![];
        };
        match result {
            Ok(()) => {
                gift.redeemed = true;
                if let Some(preview) = gift.preview.as_mut() {
                    preview.redeemed = true;
                }
            }
            Err(error) => gift.error = Some(error),
        }
        vec![]
    }

    /// Opens the gift's link in the browser, for a gift only Discord can
    /// redeem, and closes the dialog.
    pub(super) fn open_gift_in_browser(&mut self) -> Vec<Effect> {
        match self.state.gift.take() {
            Some(gift) => vec![Effect::OpenPage(format!(
                "https://discord.com/gifts/{}",
                gift.code
            ))],
            None => vec![],
        }
    }
}
