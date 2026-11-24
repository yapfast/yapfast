# Favorites

Favorites is Discord's account-wide pseudo-guild, stored in field 15 of
`PreloadedUserSettings`, independently of GIF favorites in settings type 2.
The [settings schema](https://docs.discord.food/resources/user-settings-proto#favorites-structure)
and [protobuf definition](https://github.com/discord-userdoccers/discord-protos/blob/master/discord_protos/discord_users/v1/PreloadedUserSettings.proto)
describe the map of fixed64 IDs to channel references or categories.

READY loads the list and `USER_SETTINGS_PROTO_UPDATE` of type 1 updates it.
Updates without Favorites leave it alone; an explicitly empty Favorites field
clears it. Snapshots share the parsed list until it changes and the disk cache
keeps it with the account's existing snapshot.

The rail's star opens Favorites when Discord's visibility setting permits it.
Its channels, DMs and group DMs open with the usual navigation and voice behavior.
Category names, channel nicknames, positions and initial collapsed states come
from Discord. Local category toggles don't modify Discord. Missing, inaccessible
and message-request channels stay out of the list; their saved references survive
all writes. A reference whose category is missing appears at the top level.

A connected account can right-click a channel or accepted DM to add or remove it.
New references go at the end of the top level. The service serializes local edits,
GETs `/users/@me/settings-proto/1`, and PATCHes only the complete Favorites field.
It preserves the other entries, categories, unknown fields, mute and visibility
settings. Malformed settings abort the write. A fresh GET reconciles the result;
the gateway distributes the change to other clients. A rejected edit shows a
notice and leaves the confirmed list intact. Discord decides whether the account
can write these settings. Simultaneous edits from independent clients can still
race between the GET and PATCH, as with other whole-field settings writes.

Category creation, renaming, moving references and drag-to-reorder are deferred.
No additional dependency or image download is introduced. Heap cost scales with
the number of entries and their names; a full application memory measurement is
required before marking the UI ready for review.

Tests cover codec preservation, partial tracker updates, navigation, category
ordering, local serialization, rejected writes and changes from another client.
`scripts/test-fake-discord channel_favorites` runs the service test. The fake's
`/_control/settings` returns submitted patches and accepts another client's
settings update or a request to reject the next PATCH.
