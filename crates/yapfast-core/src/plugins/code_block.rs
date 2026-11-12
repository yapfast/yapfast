//! The colours plugins give the code blocks on screen, kept by the block's own
//! content hash.
//!
//! §17.2 of the plan asks for coloured spans for one block, for on-screen
//! blocks only, cached by content hash. That is two things: a scan of the
//! markdown of the messages the person can see for its fenced blocks
//! ([`fences`]), and a cache of the answers ([`CodeBlocks`]), so a block that
//! scrolls away and back costs nothing and two messages quoting the same
//! snippet are one answer.
//!
//! A block is its own key because a code block never changes: it is a value in a
//! message, and an edit produces a message whose block hashes differently. So
//! there is no staleness here and nothing to expire; the cache is capped, and
//! the least lately asked block goes.

use std::collections::VecDeque;
use std::sync::Arc;

use yapfast_plugin_api::code::{MAX_BLOCK, Span};

/// How many blocks' colours are kept, the least lately asked going first.
pub const KEPT: usize = 128;

/// One block's colours, or that nobody coloured it.
struct Kept {
    block: String,
    /// `None` when no plugin coloured this block, which is an answer worth
    /// keeping: without it every frame would ask again.
    spans: Option<Arc<Vec<Span>>>,
    /// Where the plugin that answered sits in the chain, so the earlier plugin
    /// wins whatever order two answers arrive in.
    rank: usize,
}

/// Every block's colours kept, and a count of the ones that changed.
#[derive(Default)]
pub struct CodeBlocks {
    kept: VecDeque<Kept>,
    changes: u64,
}

impl CodeBlocks {
    /// Whether this block has been asked about, either way.
    pub fn knows(&self, block: &str) -> bool {
        self.position(block).is_some()
    }

    /// The spans of one block, or nothing: a block nobody coloured reads the
    /// same as a block nobody asked about, and the caller draws it plain either
    /// way.
    pub fn get(&self, block: &str) -> Option<Arc<Vec<Span>>> {
        let at = self.position(block)?;
        self.kept[at].spans.clone()
    }

    /// Keeps what a plugin made of one block, and answers whether what a
    /// surface draws changed.
    ///
    /// The earlier plugin in the chain wins while its answer is kept, and the
    /// same answer again changes nothing, so a block that comes back on screen
    /// redraws nothing.
    pub fn note(&mut self, block: &str, rank: usize, spans: Option<Vec<Span>>) -> bool {
        let spans = spans.map(Arc::new);
        match self.position(block) {
            Some(at) => {
                if rank > self.kept[at].rank {
                    return false;
                }
                let changed = match (&self.kept[at].spans, &spans) {
                    (Some(kept), Some(new)) => **kept != **new,
                    (None, None) => false,
                    _ => true,
                };
                let Some(_) = self.kept.remove(at) else {
                    return false;
                };
                self.kept.push_back(Kept {
                    block: block.to_owned(),
                    spans,
                    rank,
                });
                if changed {
                    self.changes += 1;
                }
                changed
            }
            None => {
                let changed = spans.is_some();
                self.kept.push_back(Kept {
                    block: block.to_owned(),
                    spans,
                    rank,
                });
                while self.kept.len() > KEPT {
                    self.kept.pop_front();
                }
                if changed {
                    self.changes += 1;
                }
                changed
            }
        }
    }

    /// The count of answers that changed what a surface draws, so the interface
    /// redraws a block that was coloured and not every block asked about.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// Forgets every block, which is what changing which plugins are enabled
    /// asks for. Answers whether there was anything to forget.
    pub fn forget(&mut self) -> bool {
        let had = !self.kept.is_empty();
        self.kept.clear();
        if had {
            self.changes += 1;
        }
        had
    }

    fn position(&self, block: &str) -> Option<usize> {
        self.kept.iter().position(|kept| kept.block == block)
    }
}

/// The content hash a block is kept under: its language and its text, because
/// the language is what colours it.
///
/// It is a hash and not the text, so the key of a cache of a hundred blocks is a
/// hundred short strings rather than a hundred snippets of code.
pub fn hash(language: Option<&str>, text: &str) -> String {
    crate::plugins::store::sha256(format!("{}\u{0}{text}", language.unwrap_or_default()).as_bytes())
}

/// The fenced code blocks in one message's markdown, in the order they appear.
///
/// A fence is three backticks, an optional language and the rest of the line,
/// and closes at the next line that is three backticks. What is between them is
/// the block, without its closing newline. A fence that never closes is not a
/// block: it is what a message being edited looks like, and colouring half of it
/// would be colouring something the person can't see.
///
/// A block longer than [`MAX_BLOCK`] is left out: it isn't asked about, and it
/// is drawn as the markdown says.
pub fn fences(body: &str) -> Vec<(Option<String>, String)> {
    let mut blocks = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim_start().strip_prefix("```") else {
            continue;
        };
        let language = rest.trim();
        let language = Some(language.to_owned()).filter(|language| !language.is_empty());
        let mut text = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim_start().starts_with("```") {
                closed = true;
                break;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line);
        }
        if closed && !text.is_empty() && text.len() <= MAX_BLOCK {
            blocks.push((language, text));
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::code::Tone;

    fn spans(text: &str) -> Vec<Span> {
        vec![Span::new(text, Tone::Plain)]
    }

    #[test]
    fn the_fences_of_a_message_are_its_blocks() {
        let body = "before\n```rust\nlet x = 1;\nlet y = 2;\n```\nafter\n```\nplain\n```\n";
        assert_eq!(
            fences(body),
            vec![
                (Some("rust".to_owned()), "let x = 1;\nlet y = 2;".to_owned()),
                (None, "plain".to_owned()),
            ]
        );
    }

    #[test]
    fn a_fence_that_never_closes_is_not_a_block() {
        assert!(fences("```rust\nlet x = 1;\n").is_empty());
        assert!(fences("no code here").is_empty());
        assert!(fences("```\n```\n").is_empty(), "an empty block is nothing");
    }

    #[test]
    fn a_block_nobody_asked_about_reads_as_one_nobody_coloured() {
        let mut blocks = CodeBlocks::default();
        assert!(!blocks.knows("hash-1"));
        assert_eq!(blocks.get("hash-1"), None);
        assert!(!blocks.note("hash-1", 0, None));
        assert!(blocks.knows("hash-1"));
        assert_eq!(blocks.changes(), 0, "nothing changed how anything draws");
    }

    #[test]
    fn a_block_is_its_own_key_and_the_oldest_goes() {
        let mut blocks = CodeBlocks::default();
        assert!(blocks.note("hash-1", 0, Some(spans("let x = 1;"))));
        assert!(blocks.knows("hash-1"));
        assert!(!blocks.knows("hash-2"));
        for index in 0..KEPT + 5 {
            blocks.note(&format!("hash-fill-{index}"), 0, None);
        }
        assert!(!blocks.knows("hash-1"), "the oldest is forgotten");
    }

    #[test]
    fn the_earlier_plugin_wins_and_the_same_answer_changes_nothing() {
        let mut blocks = CodeBlocks::default();
        assert!(blocks.note("hash-1", 0, Some(spans("a"))));
        assert!(
            !blocks.note("hash-1", 1, Some(spans("b"))),
            "a later plugin doesn't win"
        );
        let changes = blocks.changes();
        assert!(!blocks.note("hash-1", 0, Some(spans("a"))));
        assert_eq!(
            blocks.changes(),
            changes,
            "the same spans draw nothing again"
        );
        assert_eq!(
            blocks
                .get("hash-1")
                .and_then(|kept| kept.first().map(|s| s.text.clone())),
            Some("a".to_owned())
        );
    }

    #[test]
    fn a_blocks_hash_covers_its_language_and_its_text() {
        let rust = hash(Some("rust"), "let x = 1;");
        assert_eq!(rust, hash(Some("rust"), "let x = 1;"));
        assert_ne!(rust, hash(Some("python"), "let x = 1;"));
        assert_ne!(rust, hash(Some("rust"), "let y = 1;"));
        assert_ne!(rust, hash(None, "let x = 1;"));
        assert_eq!(rust.len(), 64, "lowercase hex of the SHA-256");
    }

    #[test]
    fn forgetting_every_block_says_whether_there_was_any() {
        let mut blocks = CodeBlocks::default();
        assert!(!blocks.forget());
        blocks.note("hash-1", 0, None);
        assert!(blocks.forget());
        assert!(!blocks.knows("hash-1"));
    }
}
