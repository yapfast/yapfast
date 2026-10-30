//! Gifts one Discord account gives another, such as Nitro: the codes in
//! the links people send, what a gift shows before it is redeemed, and
//! redeeming one. Discord's client resolves a gift with
//! `GET /entitlements/gift-codes/{code}` and redeems it with
//! `POST /entitlements/gift-codes/{code}/redeem`; see
//! https://docs.discord.food/resources/entitlement#gift-code-object.

use serde_json::Value;

/// Discord's gift codes are letters and digits; the ones its links carry
/// run from sixteen to twenty-four of them.
pub(crate) fn valid(code: &str) -> bool {
    (8..=32).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// What a gift shows before it is redeemed, from
/// `GET /entitlements/gift-codes/{code}` with its subscription plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub code: String,
    /// What the gift grants, such as "Nitro", by the name of its SKU.
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
    /// When the gift stops being redeemable, as Discord timestamps it,
    /// when it does.
    pub expires_at: Option<String>,
}

impl Preview {
    /// The preview of a gift code object; none for one that grants
    /// nothing Yapfast can name.
    pub fn of(gift: &Value) -> Option<Self> {
        let sku = &gift["store_listing"]["sku"];
        let plan = &gift["subscription_plan"];
        let gifter = &gift["user"];
        let uses = gift["uses"].as_u64().unwrap_or_default();
        let max_uses = gift["max_uses"].as_u64().unwrap_or(1);
        Some(Self {
            code: gift["code"].as_str()?.to_owned(),
            name: sku["name"].as_str().unwrap_or_default().to_owned(),
            plan: plan["name"].as_str().map(str::to_owned),
            gifter: gifter["global_name"]
                .as_str()
                .or_else(|| gifter["username"].as_str())
                .map(str::to_owned),
            redeemed: gift["redeemed"] == true,
            spent: uses >= max_uses,
            expires_at: gift["expires_at"].as_str().map(str::to_owned),
        })
    }

    /// Whether this account may still redeem the gift: it hasn't, and no
    /// one has used it up.
    pub fn redeemable(&self) -> bool {
        !self.redeemed && !self.spent
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
    fn gift_codes_are_letters_and_digits_of_their_length() {
        assert!(valid("2CG6SV9QtRxerJTgCYNDnU7M"));
        assert!(valid("AbCdEfGhIjKlMnOp"));
        assert!(!valid("gift"));
        assert!(!valid("has-dashes-in-it-123456"));
        assert!(!valid(""));
    }
}
