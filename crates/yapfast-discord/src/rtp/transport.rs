//! `rtp.rs`'s transport.

use super::*;

/// The bytes authenticated but not encrypted at the start of an RTP packet:
/// the fixed header, CSRCs, and the extension preamble but not its body.
pub fn rtp_clear_len(packet: &[u8]) -> Option<usize> {
    let first = *packet.first()?;
    let csrcs = usize::from(first & 0x0f) * 4;
    let extension = if first & 0x10 != 0 { 4 } else { 0 };
    let clear = HEADER + csrcs + extension;
    (packet.len() >= clear).then_some(clear)
}

/// RTCP's clear part: its header and the sender's SSRC.
pub const RTCP_CLEAR: usize = 8;

pub(crate) enum Cipher {
    Aes(Box<Aes256Gcm>),
    XChaCha(XChaCha20Poly1305),
}

/// Discord's transport encryption: the clear part is authenticated data,
/// the rest is encrypted, then the tag and a 32-bit counter, which is the
/// start of the nonce, follow.
pub struct Transport {
    pub(crate) cipher: Cipher,
    pub(crate) counter: u32,
}

pub(crate) const TAG: usize = 16;

impl Transport {
    /// The modes Yapfast asks for, best first.
    pub const MODES: [&str; 2] = ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"];

    pub fn new(mode: &str, key: &[u8]) -> Result<Self> {
        let cipher = match mode {
            "aead_aes256_gcm_rtpsize" => Cipher::Aes(Box::new(
                Aes256Gcm::new_from_slice(key)
                    .map_err(|_| anyhow!("the stream key isn't 32 bytes"))?,
            )),
            "aead_xchacha20_poly1305_rtpsize" => Cipher::XChaCha(
                XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| anyhow!("the stream key isn't 32 bytes"))?,
            ),
            _ => bail!("Discord chose an encryption mode Yapfast doesn't know: {mode}"),
        };
        Ok(Self {
            cipher,
            counter: rand::random(),
        })
    }

    /// Encrypts `packet` after its first `clear` bytes, in place.
    pub fn seal(&mut self, packet: &mut Vec<u8>, clear: usize) -> Result<()> {
        let counter = self.counter.to_be_bytes();
        self.counter = self.counter.wrapping_add(1);
        let (header, body) = packet.split_at_mut(clear);
        let sealed = match &self.cipher {
            Cipher::Aes(cipher) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(&counter);
                cipher.encrypt_in_place_detached(&nonce, header, body)
            }
            Cipher::XChaCha(cipher) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(&counter);
                cipher.encrypt_in_place_detached(&nonce, header, body)
            }
        };
        let sealed = sealed.map_err(|_| anyhow!("could not encrypt a stream packet"))?;
        // The tag comes back on the stack, so a packet sent allocates only
        // for the bytes of the packet itself.
        let mut tag = [0u8; TAG];
        tag.copy_from_slice(&sealed);
        packet.extend_from_slice(&tag);
        packet.extend_from_slice(&counter);
        Ok(())
    }

    /// Decrypts a sealed packet where it lies, returning its clear part and
    /// plaintext: everything but the tag and the counter, which close it.
    /// The caller's own buffer is the plaintext, which is what makes a call's
    /// hundreds of received packets a second cost no allocation each.
    pub fn open<'a>(&self, packet: &'a mut [u8], clear: usize) -> Option<&'a [u8]> {
        if packet.len() < clear + TAG + 4 {
            return None;
        }
        let sealed = packet.len() - TAG - 4;
        let (header, rest) = packet.split_at_mut(clear);
        let (body, tail) = rest.split_at_mut(sealed - clear);
        let (tag, counter) = tail.split_at(TAG);
        let tag: &[u8] = tag;
        let opened = match &self.cipher {
            Cipher::Aes(cipher) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(counter);
                cipher.decrypt_in_place_detached(&nonce, header, body, tag.into())
            }
            Cipher::XChaCha(cipher) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(counter);
                cipher.decrypt_in_place_detached(&nonce, header, body, tag.into())
            }
        };
        opened.ok()?;
        Some(&packet[..sealed])
    }
}
