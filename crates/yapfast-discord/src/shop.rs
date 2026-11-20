//! Discord's Shop: collectibles (avatar decorations, profile effects,
//! nameplates, bundles) in categories, from `GET /collectibles-categories`,
//! and the account's Orbs from `GET /users/@me/virtual-currency/balance`.
//! Yapfast lists them with their pictures and prices; paying is done on
//! discord.com, as a card or Orbs needs Discord's own checkout.

use serde_json::Value;

use crate::collectibles;

/// Discord's numbers for what a product is, `type`.
const DECORATION: u64 = 0;
const EFFECT: u64 = 1;
const NAMEPLATE: u64 = 2;
const FRAME: u64 = 3;
const BUNDLE: u64 = 1000;

/// What a product is, as its card words it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Other,
    Decoration,
    Effect,
    Nameplate,
    Frame,
    Bundle,
}

/// A price: Discord's `amount` in the currency's smallest unit, and its
/// `exponent` places, 2 for euros.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Price {
    pub amount: u64,
    pub currency: String,
    pub exponent: u32,
}

impl Price {
    /// The price as a person reads it, such as "$8.99".
    pub fn label(&self) -> String {
        let places = self.exponent.min(6) as usize;
        let whole = self.amount / 10u64.pow(self.exponent.min(6));
        let number = if places == 0 {
            whole.to_string()
        } else {
            let fraction = self.amount % 10u64.pow(self.exponent.min(6));
            format!("{whole}.{fraction:0places$}")
        };
        match self.currency.as_str() {
            "usd" => format!("${number}"),
            "eur" => format!("\u{20ac}{number}"),
            "gbp" => format!("\u{a3}{number}"),
            other => format!("{number} {}", other.to_uppercase()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub summary: String,
    pub kind: Kind,
    pub picture: Option<String>,
    pub price: Option<Price>,
    /// What it costs in Orbs, if Orbs buy it.
    pub orbs: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Category {
    pub sku: String,
    pub name: String,
    pub summary: String,
    /// The wide picture Discord's page opens it with.
    pub banner: Option<String>,
    pub logo: Option<String>,
    /// Every product costs Orbs.
    pub orbs_only: bool,
    pub products: Vec<Product>,
}

/// The categories in an answer from `/collectibles-categories`, a list, or
/// from `/collectibles-shop`, which holds it as `categories`.
pub fn parse(value: &Value) -> Vec<Category> {
    value
        .as_array()
        .or_else(|| value["categories"].as_array())
        .map(|categories| categories.iter().filter_map(category).collect())
        .unwrap_or_default()
}

/// The Orbs in an answer from `/users/@me/virtual-currency/balance`.
pub fn orbs(value: &Value) -> Option<u64> {
    value["balance"]
        .as_u64()
        .or_else(|| value["balance"].as_str()?.parse().ok())
}

fn url(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|url| url.starts_with("http"))
        .map(str::to_owned)
}

/// The first address among `candidates`, which are fields that may hold
/// one, or a `{static, animated}` object holding it.
fn first_url(candidates: &[&Value]) -> Option<String> {
    candidates.iter().find_map(|value| {
        url(value)
            .or_else(|| url(&value["static"]))
            .or_else(|| url(&value["url"]))
    })
}

fn category(category: &Value) -> Option<Category> {
    let sku = category["sku_id"].as_str()?.to_owned();
    let name = category["name"].as_str()?.to_owned();
    let products: Vec<Product> = category["products"]
        .as_array()
        .map(|products| products.iter().filter_map(product).collect())
        .unwrap_or_default();
    Some(Category {
        sku,
        name,
        summary: category["summary"].as_str().unwrap_or_default().to_owned(),
        banner: first_url(&[
            &category["hero_banner_asset"],
            &category["catalog_banner_asset"],
            &category["wide_banner_asset"],
            &category["featured_block_url"],
            &category["mobile_banner_url"],
        ]),
        logo: first_url(&[&category["logo_url"], &category["hero_logo_url"]]),
        orbs_only: category["is_orbs_exclusive"].as_bool().unwrap_or(false),
        products,
    })
}

fn kind(product: &Value) -> Kind {
    match product["type"].as_u64() {
        Some(DECORATION) => Kind::Decoration,
        Some(EFFECT) => Kind::Effect,
        Some(NAMEPLATE) => Kind::Nameplate,
        Some(FRAME) => Kind::Frame,
        Some(BUNDLE) => Kind::Bundle,
        _ => Kind::Other,
    }
}

/// A product's picture: what it names, else the still Discord's CDN keeps
/// for a decoration or a nameplate.
fn picture(product: &Value, kind: Kind) -> Option<String> {
    let item = product["items"].as_array().and_then(|items| items.first());
    if let Some(item) = item
        && let Some(found) = url(&item["assets"]["static_image_url"])
            .or_else(|| url(&item["staticFrameSrc"]))
            .or_else(|| url(&item["thumbnailPreviewSrc"]))
    {
        return Some(found);
    }
    let preview = &product["preview_assets"];
    if let Some(found) = url(&preview["fg_static"]).or_else(|| url(&preview["bg_static"])) {
        return Some(found);
    }
    if let Some(inner) = product["bundled_products"].as_array().and_then(|inner| {
        inner
            .iter()
            .find_map(|inner| picture(inner, self::kind(inner)))
    }) {
        return Some(inner);
    }
    match kind {
        Kind::Decoration | Kind::Nameplate => {
            let sku = item
                .and_then(|item| item["sku_id"].as_str())
                .or_else(|| product["sku_id"].as_str())?
                .parse()
                .ok()?;
            Some(collectibles::picture(sku))
        }
        _ => None,
    }
}

/// The first price among Discord's `prices`, a map by purchase type, each
/// with prices by country or payment source: money, and Orbs apart.
fn prices(product: &Value) -> (Option<Price>, Option<u64>) {
    let (mut money, mut orbs) = (None, None);
    if let Some(types) = product["prices"].as_object() {
        for value in types.values() {
            find_prices(value, 0, &mut money, &mut orbs);
        }
    }
    (money, orbs)
}

fn find_prices(value: &Value, depth: u32, money: &mut Option<Price>, orbs: &mut Option<u64>) {
    if depth > 6 {
        return;
    }
    if let (Some(amount), Some(currency)) = (value["amount"].as_u64(), value["currency"].as_str()) {
        if currency.contains("orb") {
            orbs.get_or_insert(amount);
        } else if money.is_none() {
            *money = Some(Price {
                amount,
                currency: currency.to_lowercase(),
                exponent: value["exponent"].as_u64().unwrap_or(2) as u32,
            });
        }
        return;
    }
    match value {
        Value::Array(list) => {
            for inner in list {
                find_prices(inner, depth + 1, money, orbs);
            }
        }
        Value::Object(map) => {
            for inner in map.values() {
                find_prices(inner, depth + 1, money, orbs);
            }
        }
        _ => {}
    }
}

fn product(product: &Value) -> Option<Product> {
    let kind = kind(product);
    let (price, orbs) = prices(product);
    Some(Product {
        sku: product["sku_id"].as_str()?.to_owned(),
        name: product["name"].as_str()?.to_owned(),
        summary: product["summary"].as_str().unwrap_or_default().to_owned(),
        kind,
        picture: picture(product, kind),
        price,
        orbs,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_categories_with_their_products_and_prices() {
        let shop = parse(&json!([{
            "sku_id": "10", "name": "Vice City", "summary": "Neon.",
            "hero_banner_asset": {"static": "https://cdn.example/hero.png", "animated": null},
            "logo_url": "https://cdn.example/logo.png",
            "products": [
                {
                    "sku_id": "11", "name": "Palm", "summary": "A palm.", "type": 0,
                    "items": [{"type": 0, "sku_id": "11", "assets": {"static_image_url": "https://cdn.example/palm.png"}}],
                    "prices": {"0": {"country_prices": {"country_code": "DE", "prices": [{"amount": 899, "currency": "eur", "exponent": 2}]}}}
                },
                {
                    "sku_id": "12", "name": "Neon plate", "type": 2,
                    "items": [{"type": 2, "sku_id": "12"}],
                    "prices": {"4": {"country_prices": {"prices": [{"amount": 1200, "currency": "discord_orb", "exponent": 0}]}}}
                }
            ]
        }]));
        let category = &shop[0];
        assert_eq!(category.name, "Vice City");
        assert_eq!(
            category.banner.as_deref(),
            Some("https://cdn.example/hero.png")
        );
        let palm = &category.products[0];
        assert_eq!(palm.kind, Kind::Decoration);
        assert_eq!(
            palm.picture.as_deref(),
            Some("https://cdn.example/palm.png")
        );
        assert_eq!(palm.price.as_ref().unwrap().label(), "\u{20ac}8.99");
        assert_eq!(palm.orbs, None);
        let plate = &category.products[1];
        assert_eq!(plate.orbs, Some(1200));
        assert!(plate.price.is_none());
        assert!(
            plate
                .picture
                .as_deref()
                .unwrap()
                .ends_with("/media/v1/collectibles-shop/12/static"),
            "a nameplate without a picture named gets the CDN's still"
        );
    }

    #[test]
    fn reads_orbs_and_skips_what_it_cannot_name() {
        assert_eq!(orbs(&json!({"balance": 550})), Some(550));
        assert_eq!(orbs(&json!({"balance": "70"})), Some(70));
        assert_eq!(orbs(&json!({})), None);
        assert!(parse(&json!({"categories": [{"sku_id": "1"}]})).is_empty());
        assert!(parse(&json!(null)).is_empty());
        assert_eq!(
            Price {
                amount: 500,
                currency: "chf".into(),
                exponent: 2
            }
            .label(),
            "5.00 CHF"
        );
    }
}
