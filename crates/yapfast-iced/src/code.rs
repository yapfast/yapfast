//! What plugins made of the code blocks on screen, as the window reads it: the
//! tones one fenced block was coloured with, by the key the application asked
//! the plugins with (the hash of the block's language and text).
//!
//! The application keeps the answers (`yapfast_plugins::code_block`), asking
//! about the blocks of the rows on screen and no others, so a window reads here
//! and never asks. `markdown::colour_code` is the only reader: it cuts a block
//! into the spans kept for it, and a block nobody coloured, or one never asked
//! about, draws plain.

#![cfg_attr(
    not(feature = "plugins"),
    allow(
        dead_code,
        reason = "a build without the plugin system colours no code block"
    )
)]

use yapfast_core::application::AppState;

/// What a plugin made of one code block, or nothing: a span a tone each, kept
/// by the application and shared by every row quoting the block.
#[cfg(feature = "plugins")]
pub type Spans = std::sync::Arc<Vec<yapfast_plugin_api::code::Span>>;

/// A build without the plugin system has no tone to draw, and never asks.
#[cfg(not(feature = "plugins"))]
pub type Spans = ();

/// What plugins drew on one message, as the timeline lays its text out: one
/// number that changes whenever a decoration of it changes, and the marks over
/// its words. A build without plugins draws nothing on any message.
pub fn decor(state: &AppState, item: &str) -> (u64, crate::markdown::Decor) {
    #[cfg(feature = "plugins")]
    {
        let (key, marks) = match state.plugin_slots.as_deref() {
            Some(slots) => crate::plugin_slots::decor(slots, item),
            None => (0, crate::markdown::Decor::default()),
        };
        // A message a plugin sent says so under it, which changes its height.
        let sent = u64::from(state.plugin_sent_by.contains_key(item)) << 1;
        (key ^ sent, marks)
    }
    #[cfg(not(feature = "plugins"))]
    {
        let _ = (state, item);
        (0, crate::markdown::Decor::default())
    }
}

/// The colours of one code block, or nothing: a block nobody coloured draws
/// plain, the same as one nobody asked about.
pub fn spans(state: &AppState, block: &str) -> Option<Spans> {
    #[cfg(feature = "plugins")]
    {
        state.plugins.as_ref()?.code_block(block)
    }
    #[cfg(not(feature = "plugins"))]
    {
        let _ = (state, block);
        None
    }
}
