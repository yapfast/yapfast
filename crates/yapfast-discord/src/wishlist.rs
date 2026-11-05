//! Someone's wishlist, from `GET /wishlists/{id}`, which Discord's client asks
//! for both its own account's and others'. `wishlist_settings` in a profile
//! holds the ids, by which `{updated_at, visibility}`; `POST
//! /users/@me/wishlist/items` only adds, so Yapfast never sends it.

use serde_json::Value;

use crate::ready;

/// Discord's numbers for what an item is, `sku_product_line`.
const PREMIUM: u64 = 1;
const COLLECTIBLES: u64 = 7;

/// Discord's numbers for the kinds of collectible, `type` on one's `item`.
const DECORATION: u64 = 0;
const EFFECT: u64 = 1;
const NAMEPLATE: u64 = 2;
const FRAME: u64 = 3;

/// Where a wishlist is, as a profile says.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Place {
    pub id: String,
    /// Only its owner may see it.
    pub private: bool,
}

/// The wishlist in a profile, the first as in Discord's client.
pub fn place(profile: &Value) -> Option<Place> {
    let (id, settings) = profile["wishlist_settings"].as_object()?.iter().next()?;
    Some(Place {
        id: id.clone(),
        // Public unless it says otherwise; 0 is private and 1 public.
        private: settings["visibility"].as_u64() == Some(0),
    })
}

/// An item on a wishlist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    pub sku: String,
    pub name: String,
    /// The address of its picture. Nitro has none: Discord's client draws its
    /// own.
    pub picture: Option<String>,
}

/// The items in an answer from `GET /wishlists/{id}`, or none if it holds no
/// `wishlist_items`.
pub fn parse(value: &Value) -> Option<Vec<Item>> {
    Some(
        value["wishlist_items"]
            .as_array()?
            .iter()
            .filter_map(item)
            .collect(),
    )
}

fn item(item: &Value) -> Option<Item> {
    let sku = &item["sku"];
    let id = item["sku_id"].as_str().or_else(|| sku["id"].as_str())?;
    let name = [&sku["name"], &item["sku_name"]]
        .into_iter()
        .find_map(|name| name.as_str().filter(|name| !name.trim().is_empty()))?;
    let line = sku["product_line"]
        .as_u64()
        .or_else(|| item["sku_product_line"].as_u64());
    let picture = match line {
        Some(PREMIUM) => None,
        Some(COLLECTIBLES) => collectible_picture(id, sku).or_else(|| {
            collectible_item_picture(id, &item["collectibles_item"]).or_else(|| {
                item["bundle_items"]
                    .as_array()?
                    .iter()
                    .find_map(|inner| collectible_item_picture(id, inner))
            })
        }),
        _ => store_picture(sku),
    };
    Some(Item {
        sku: id.to_owned(),
        name: name.to_owned(),
        picture,
    })
}

/// A collectible's picture, from its SKU: a bundle's own, else the picture of
/// the first thing in it that has one.
fn collectible_picture(id: &str, sku: &Value) -> Option<String> {
    collectible_item_picture(id, &sku["tenant_metadata"]["collectibles"]["item"])
        .or_else(|| text(&sku["preview_asset_paths"]["fg_static"]).map(str::to_owned))
        .or_else(|| {
            sku["bundled_skus"]
                .as_array()?
                .iter()
                .find_map(|inner| collectible_picture(inner["id"].as_str().unwrap_or(id), inner))
        })
}

/// The picture for a collectible's item, by its kind, as Discord's client finds
/// it.
fn collectible_item_picture(id: &str, item: &Value) -> Option<String> {
    let id = item["sku_id"].as_str().unwrap_or(id);
    match item["type"].as_u64()? {
        DECORATION => text(&item["asset"]).map(ready::avatar_decoration),
        EFFECT => ["thumbnailPreviewSrc", "reducedMotionSrc", "staticFrameSrc"]
            .iter()
            .find_map(|source| text(&item[source]))
            .map(str::to_owned),
        NAMEPLATE => Some(shop_asset(id, None)),
        FRAME => {
            let layer = item["layers"]
                .as_array()?
                .iter()
                .find(|layer| layer["responsive"] != true)?;
            Some(shop_asset(id, Some(layer["id"].as_str()?)))
        }
        _ => None,
    }
}

/// A still of something from Discord's shop.
fn shop_asset(sku: &str, layer: Option<&str>) -> String {
    let layer = layer.map_or(String::new(), |layer| format!("{layer}/"));
    format!(
        "{}/media/v1/collectibles-shop/{sku}/{layer}static",
        ready::cdn()
    )
}

/// A game's item's card: its picture from its application's store assets.
fn store_picture(sku: &Value) -> Option<String> {
    let application = sku["application_id"].as_str()?;
    let asset = text(&sku["tenant_metadata"]["social_layer"]["card_image_asset_id"])
        .or_else(|| text(&sku["thumbnail_asset_id"]))?;
    Some(format!(
        "{}/app-assets/{application}/store/{asset}.webp?size=512",
        ready::cdn()
    ))
}

/// A text field that may be missing, null or empty.
fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_profile_says_which_wishlist_is_theirs_and_who_may_see_it() {
        let open = json!({"wishlist_settings": {
            "90": {"updated_at": "2026-11-01T00:00:00+00:00", "visibility": 1},
        }});
        let place_of = |profile: &Value| place(profile).unwrap();
        assert_eq!(place_of(&open).id, "90");
        assert!(!place_of(&open).private);
        let closed = json!({"wishlist_settings": {"91": {"visibility": 0}}});
        assert!(place_of(&closed).private);
        let unsaid = json!({"wishlist_settings": {"92": {}}});
        assert!(!place_of(&unsaid).private);
        assert_eq!(place(&json!({"wishlist_settings": {}})), None);
        assert_eq!(place(&json!({})), None);
    }

    /// What the client's `WishlistItem` classes read for each kind.
    #[test]
    fn each_kind_of_item_has_its_name_and_picture() {
        let answer = json!({"id": "90", "user_id": "1", "wishlist_items": [
            {"sku_id": "a", "sku_product_line": 1, "sku_name": "Nitro", "is_owned": false,
             "sku": {"id": "a", "name": "Nitro", "product_line": 1}},
            {"sku_id": "b", "sku_product_line": 7, "sku_name": "Blossom Cat",
             "collectibles_item": {"type": 0, "sku_id": "b", "asset": "a_cat", "label": "Cat"},
             "sku": {"id": "b", "name": "Blossom Cat", "product_line": 7,
                     "tenant_metadata": {"collectibles": {"type": 0, "item": {
                         "type": 0, "id": "b", "asset": "a_cat", "label": "Cat",
                     }}}}},
            {"sku_id": "c", "sku_product_line": 7, "sku_name": "Sparkles",
             "collectibles_item": {"type": 1, "sku_id": "c", "title": "Sparkles",
                                   "thumbnailPreviewSrc": "https://cdn.discordapp.com/assets/thumb.png"},
             "sku": {"id": "c", "name": "Sparkles", "product_line": 7}},
            {"sku_id": "d", "sku_product_line": 7, "sku_name": "Bonsai",
             "sku": {"id": "d", "name": "Bonsai", "product_line": 7,
                     "tenant_metadata": {"collectibles": {"item": {
                         "type": 2, "asset": "nameplates/bonsai/", "palette": "forest",
                     }}}}},
            {"sku_id": "e", "sku_product_line": 7, "sku_name": "Frame",
             "sku": {"id": "e", "name": "Frame", "product_line": 7,
                     "tenant_metadata": {"collectibles": {"item": {"type": 3, "layers": [
                         {"id": "wide", "responsive": true}, {"id": "still"},
                     ]}}}}},
            {"sku_id": "f", "sku_product_line": 7, "sku_name": "Bundle",
             "bundle_items": [{"type": 0, "sku_id": "f1", "asset": "inner"}],
             "sku": {"id": "f", "name": "Bundle", "product_line": 7, "type": 3,
                     "preview_asset_paths": {"fg_static": "https://cdn.discordapp.com/fg.png"}}},
            {"sku_id": "g", "sku_product_line": 7, "sku_name": "Bare Bundle",
             "bundle_items": [{"type": 0, "sku_id": "g1", "asset": "inner"}],
             "sku": {"id": "g", "name": "Bare Bundle", "product_line": 7}},
            {"sku_id": "h", "sku_product_line": 14, "sku_name": "Gold",
             "sku": {"id": "h", "name": "Gold", "product_line": 14, "application_id": "20",
                     "tenant_metadata": {"social_layer": {"card_image_asset_id": "card"}},
                     "thumbnail_asset_id": "thumb"}},
            {"sku_id": "i", "sku_product_line": 14, "sku_name": "Silver",
             "sku": {"id": "i", "name": "Silver", "product_line": 14, "application_id": "20",
                     "thumbnail_asset_id": "thumb"}},
            {"sku_id": "j", "sku_product_line": 6, "sku_name": "From the name alone"},
            {"sku_id": "k", "sku_product_line": 7},
        ]});
        let items = parse(&answer).unwrap();
        let names: Vec<_> = items.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Nitro",
                "Blossom Cat",
                "Sparkles",
                "Bonsai",
                "Frame",
                "Bundle",
                "Bare Bundle",
                "Gold",
                "Silver",
                "From the name alone",
            ],
            "an item with no name at all has nothing to show"
        );
        let picture = |sku: &str| {
            items
                .iter()
                .find(|item| item.sku == sku)
                .unwrap()
                .picture
                .as_deref()
        };
        assert_eq!(picture("a"), None);
        assert!(
            picture("b")
                .unwrap()
                .contains("/avatar-decoration-presets/a_cat.png")
        );
        assert_eq!(
            picture("c"),
            Some("https://cdn.discordapp.com/assets/thumb.png")
        );
        assert!(
            picture("d")
                .unwrap()
                .ends_with("/media/v1/collectibles-shop/d/static")
        );
        assert!(
            picture("e")
                .unwrap()
                .ends_with("/media/v1/collectibles-shop/e/still/static")
        );
        assert_eq!(picture("f"), Some("https://cdn.discordapp.com/fg.png"));
        assert!(
            picture("g")
                .unwrap()
                .contains("/avatar-decoration-presets/inner.png")
        );
        assert!(
            picture("h")
                .unwrap()
                .ends_with("/app-assets/20/store/card.webp?size=512")
        );
        assert!(
            picture("i")
                .unwrap()
                .ends_with("/app-assets/20/store/thumb.webp?size=512")
        );
        assert_eq!(picture("j"), None);
    }

    #[test]
    fn an_answer_without_items_is_not_an_empty_list() {
        assert_eq!(parse(&json!({"wishlist_items": []})), Some(vec![]));
        assert_eq!(parse(&json!({"message": "Unknown"})), None);
    }
}
