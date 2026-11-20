# Media Picker, an example plugin (design)

A tab in the expression picker, beside GIFs, for the pictures and videos in a
folder you choose. Picking one sends it in the conversation you are in.

| Piece | Where it comes from |
| --- | --- |
| The folder | `fs.folder` with `"files": { "write": false }`: the person picks it in the system's folder picker the first time, and the plugin sees nothing else |
| The grid | `fs.list` of the folder, then `fs.thumbnail` for each picture in view, 128 px, decoded by Yapfast off every thread a person waits on and capped at a 32 MiB source |
| The tab | the `picker.tab` slot, which the interface branch adds to the picker; until it is merged, `ui.set_slot { "slot": "picker.tab" }` answers `{ "drawn": false }` and the plugin falls back to its command |
| Sending | `messages.send { room, body: "", files: ["holiday/beach.jpg"] }` with `messages.attach`, behind the click on the tile |

The click on a tile is the person's gesture, so the send needs no agent
permission and goes through the ordinary rails: five in ten seconds, at most
ten files, each at most 10 MiB, sent as a person's attached files are and never
in place of what they had attached themselves.

A video has no thumbnail yet: `fs.thumbnail` decodes PNG, JPEG, GIF and WebP,
so a video's tile shows its name and an icon from the plugin's assets. Decoding
a frame of a video is the interface's work, with its own player, and would come
as another thumbnail kind rather than a plugin running a decoder.

Until the `picker.tab` slot lands, the plugin offers `/media` and a composer
button that opens its dialog with the same grid as a widget tree.
