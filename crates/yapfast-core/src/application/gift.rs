//! A Discord gift, such as Nitro, that a clicked gift link opens. The
//! gift is looked up and shown first; redeeming it waits for the click,
//! as joining a server by an invite does, and a gift this account
//! redeemed, or anyone used up, offers nothing to redeem.

use super::*;
use crate::chat::GiftPreview;

/// The gift dialog, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GiftDialog {
    /// The code of the gift looked up or shown.
    pub code: Option<String>,
    /// What the gift shows, once Discord answered.
    pub preview: Option<GiftPreview>,
    /// The room its link was opened in, which redeeming names.
    pub room: Option<String>,
    pub resolving: bool,
    pub redeeming: bool,
    /// This account redeemed the gift, from this dialog.
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
            code: Some(code.clone()),
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
            .filter(|gift| gift.resolving && gift.code.as_ref() == Some(&code))
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
        let Some(gift) = self
            .state
            .gift
            .as_mut()
            .filter(|gift| !gift.redeeming && !gift.redeemed)
        else {
            return vec![];
        };
        let Some(preview) = gift.preview.clone() else {
            return vec![];
        };
        if preview.redeemed || preview.spent || self.state.preview {
            return vec![];
        }
        gift.redeeming = true;
        gift.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::RedeemGift {
                preview: Box::new(preview),
                room: gift.room.clone(),
            },
        ))]
    }

    pub(super) fn gift_redeemed(
        &mut self,
        code: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let Some(gift) = self
            .state
            .gift
            .as_mut()
            .filter(|gift| gift.redeeming && gift.code.as_ref() == Some(&code))
        else {
            return vec![];
        };
        gift.redeeming = false;
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
}
