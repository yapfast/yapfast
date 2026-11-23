//! The person's clipboard: `clipboard.write`, which needs the
//! `clipboard.write` capability and a click of the person's behind it, the
//! same rule a send follows. Call it from `yf_input` (a click on the plugin's
//! own row, button or menu row), a command or a menu row; without a recent
//! gesture the answer is `denied`.

use serde_json::{Value, json};

use crate::call::{self, method};
use crate::{Error, Result};

/// The most text one write takes: 64 KiB. Longer text is refused whole, never
/// cut, so it's refused here before the call is made.
pub const MAX_BYTES: usize = 64 << 10;

fn payload(text: &str) -> Result<Value> {
    if text.len() > MAX_BYTES {
        return Err(Error::msg(format!(
            "`clipboard.write` takes at most {MAX_BYTES} bytes of text; this was {}",
            text.len()
        )));
    }
    Ok(json!({ "text": text }))
}

/// Puts `text` on the clipboard, as a person's Copy would, such as a user's
/// ID when they click it.
pub fn write(text: &str) -> Result<()> {
    call::ok(method::CLIPBOARD_WRITE, payload(text)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_is_the_shape_the_app_reads_and_too_much_is_refused() {
        assert_eq!(
            payload("1234567890").unwrap(),
            json!({ "text": "1234567890" })
        );
        assert!(payload(&"a".repeat(MAX_BYTES)).is_ok());
        assert!(payload(&"a".repeat(MAX_BYTES + 1)).is_err());
    }
}
