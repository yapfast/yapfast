//! Gifts one Discord account gives another, such as Nitro: the codes in
//! the links people send, what a gift shows before it is redeemed, and
//! redeeming one. Discord's client resolves a gift with
//! `GET /entitlements/gift-codes/{code}` and redeems it with
//! `POST /entitlements/gift-codes/{code}/redeem`; see
//! https://docs.discord.food/resources/entitlement#gift-code-object.

use serde_json::Value;

/// The gift code a link carries, as Discord's client reads it: letters,
/// digits and dashes, with the dashes that may group them dropped. Yapfast
/// takes eight to thirty-two letters and digits, which Discord's gift codes
/// fall within; a shorter word after `/gifts/` stays an ordinary link.
pub(crate) fn code(link: &str) -> Option<String> {
    if !link
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    let code: String = link.chars().filter(|c| *c != '-').collect();
    (8..=32).contains(&code.len()).then_some(code)
}

/// The gift code flags that keep a gift from being redeemed with a click:
/// it needs a payment source, or it's from a promotion, which Discord's
/// client redeems only on its promotion page.
const PAYMENT_SOURCE_REQUIRED: u64 = 1 << 0;
const PROMOTION: u64 = 1 << 3;

/// What a gift shows before it is redeemed, from
/// `GET /entitlements/gift-codes/{code}` with its subscription plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub code: String,
    /// What the gift grants, such as "Nitro", by the name of its SKU, or
    /// of its plan when Discord names no SKU.
    pub name: String,
    /// The subscription plan it grants, such as a month of it, by the
    /// plan's name, when the gift is one.
    pub plan: Option<String>,
    /// Who made the gift, by the name Discord shows for them, when it
    /// says.
    pub gifter: Option<String>,
    /// This account has redeemed the gift already.
    pub redeemed: bool,
    /// Every use of the gift has been redeemed, by anyone.
    pub spent: bool,
    /// Only Discord can redeem the gift: it needs a payment source, or
    /// it's from a promotion.
    pub in_discord: bool,
}

impl Preview {
    /// The preview of a gift code object; none for an answer without a
    /// code.
    pub fn of(gift: &Value) -> Option<Self> {
        let plan = gift["subscription_plan"]["name"].as_str();
        let gifter = &gift["user"];
        let uses = gift["uses"].as_u64().unwrap_or_default();
        let max_uses = gift["max_uses"].as_u64().unwrap_or(1);
        let flags = gift["flags"].as_u64().unwrap_or_default();
        Some(Self {
            code: gift["code"].as_str()?.to_owned(),
            name: gift["store_listing"]["sku"]["name"]
                .as_str()
                .or(plan)
                .unwrap_or("A gift")
                .to_owned(),
            plan: plan.map(str::to_owned),
            gifter: gifter["global_name"]
                .as_str()
                .or_else(|| gifter["username"].as_str())
                .map(str::to_owned),
            redeemed: gift["redeemed"] == true,
            spent: uses >= max_uses,
            in_discord: flags & (PAYMENT_SOURCE_REQUIRED | PROMOTION) != 0
                || !gift["promotion"].is_null(),
        })
    }

    /// Whether this account may still redeem the gift here: it hasn't, no
    /// one has used it up, and it needs nothing only Discord can do.
    pub fn redeemable(&self) -> bool {
        !self.redeemed && !self.spent && !self.in_discord
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn gift(uses: u64, redeemed: bool) -> Value {
        json!({
            "code": "2CG6SV9QtRxerJTgCYNDnU7M",
            "sku_id": "521847234246082599",
            "uses": uses,
            "max_uses": 1,
            "redeemed": redeemed,
            "expires_at": null,
            "user": {"id": "10", "username": "gifter", "global_name": "Gift Giver"},
            "store_listing": {"sku": {"id": "521847234246082599", "name": "Nitro"}},
            "subscription_plan": {"id": "642251038925127690", "name": "Nitro Monthly"},
        })
    }

    #[test]
    fn reads_what_a_gift_shows_before_it_is_redeemed() {
        let preview = Preview::of(&gift(0, false)).unwrap();
        assert_eq!(preview.code, "2CG6SV9QtRxerJTgCYNDnU7M");
        assert_eq!(preview.name, "Nitro");
        assert_eq!(preview.plan.as_deref(), Some("Nitro Monthly"));
        assert_eq!(preview.gifter.as_deref(), Some("Gift Giver"));
        assert!(preview.redeemable());
    }

    #[test]
    fn a_redeemed_or_spent_gift_cannot_be_redeemed_again() {
        assert!(!Preview::of(&gift(1, true)).unwrap().redeemable());
        assert!(!Preview::of(&gift(1, false)).unwrap().redeemable());
    }

    #[test]
    fn a_gift_that_needs_a_payment_source_or_is_a_promotion_is_redeemed_in_discord() {
        assert!(!Preview::of(&gift(0, false)).unwrap().in_discord);
        for (key, value) in [
            ("flags", json!(1)),
            ("flags", json!(8)),
            ("promotion", json!({"id": "20"})),
        ] {
            let mut gift = gift(0, false);
            gift[key] = value;
            let preview = Preview::of(&gift).unwrap();
            assert!(preview.in_discord && !preview.redeemable(), "{key}");
        }
        // Not for users with a subscription already: Discord says so on
        // redeeming.
        let mut subscribers = gift(0, false);
        subscribers["flags"] = json!(2);
        assert!(Preview::of(&subscribers).unwrap().redeemable());
    }

    #[test]
    fn a_gift_without_a_store_listing_goes_by_its_plan() {
        let mut gift = gift(0, false);
        gift.as_object_mut().unwrap().remove("store_listing");
        assert_eq!(Preview::of(&gift).unwrap().name, "Nitro Monthly");
        gift.as_object_mut().unwrap().remove("subscription_plan");
        assert_eq!(Preview::of(&gift).unwrap().name, "A gift");
    }

    #[test]
    fn gift_codes_are_letters_and_digits_with_dashes_dropped() {
        let code = |link| super::code(link);
        assert_eq!(
            code("2CG6SV9QtRxerJTgCYNDnU7M").as_deref(),
            Some("2CG6SV9QtRxerJTgCYNDnU7M")
        );
        assert_eq!(
            code("AbCdEfGhIjKlMnOp").as_deref(),
            Some("AbCdEfGhIjKlMnOp")
        );
        assert_eq!(
            code("AbCd-EfGh-IjKl-MnOp").as_deref(),
            Some("AbCdEfGhIjKlMnOp")
        );
        assert_eq!(
            code("has-dashes-in-it-123456").as_deref(),
            Some("hasdashesinit123456")
        );
        assert_eq!(code("gift"), None);
        assert_eq!(code("g-i-f-t"), None);
        assert_eq!(code("not_a_gift_code"), None);
        assert_eq!(code(""), None);
    }
}
