// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! The native LiveKit backing for `matrix-rtc-media`'s key handler.
//!
//! The bookkeeping — recording, the ring-size guard, the rejected-key rule,
//! the local sender's index switch, the MSC4143 `delayBeforeUse` wait — lives
//! in [`matrix_rtc_transport::keys`], shared with the web binding. This module
//! contributes only what is LiveKit-native: a [`KeyProvider`] configured for
//! MSC4195 per-participant mode, one per SFU connection
//! ([`Msc4195KeyProviders`]), and their [`FrameKeyRing`] adapter.
//!
//! `MediaKeyBridge` remains this crate's name for the shared
//! [`MediaKeyHandler`]; build one over a provider with
//! [`msc4195_media_key_bridge`].

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use livekit::e2ee::key_provider::{KeyDerivationAlgorithm, KeyProvider, KeyProviderOptions};
use livekit::id::ParticipantIdentity;
use matrix_rtc_transport::keys::{FrameKeyRing, MediaKeyHandler};

pub use matrix_rtc_transport::keys::{
    KeyDiscardListener, KeyImportListener, LocalKeyIndexHook, ParticipantKey,
    SwitchCompleteListener,
};

/// This crate's name for the shared key handler, kept from when the type lived
/// here.
pub type MediaKeyBridge = MediaKeyHandler;

/// The native frame cryptor's key ring: every MSC4143 key index, 0 to 255
/// (spec 038 R38).
///
/// With `livekit` 0.9.7 a ring of 256 stores and returns index 255. An index
/// past the ring is not an error there: `set_key` reports success and
/// `get_key` returns nothing, so [`FrameKeyRing::ring_size`] still guards it.
pub const NATIVE_KEY_RING_MAX: i32 = 256;

/// [`KeyProviderOptions`] for MSC4195 per-participant mode.
///
/// MSC4195 specifies that per-participant keys are "imported as the byte array
/// input to the LiveKit key derivation function (which uses HKDF)" and that the
/// `index` field of `m.rtc.encryption_keys` is used as the key index. The ring
/// is sized to [`NATIVE_KEY_RING_MAX`], one slot per index.
pub fn msc4195_key_provider_options() -> KeyProviderOptions {
    KeyProviderOptions {
        key_derivation_algorithm: KeyDerivationAlgorithm::HKDF,
        key_ring_size: NATIVE_KEY_RING_MAX,
        ..KeyProviderOptions::default()
    }
}

/// A per-participant (non-shared) [`KeyProvider`] configured for MSC4195.
///
/// Pass the same provider to [`msc4195_media_key_bridge`] and to
/// [`livekit::RoomOptions`] (the `encryption` field) so keys signalled by
/// `matrix-rtc-core` reach the frame cryptor of the connected room.
pub fn msc4195_key_provider() -> KeyProvider {
    KeyProvider::new(msc4195_key_provider_options())
}

/// The key providers of a call's SFU connections, one per connection, with
/// every key installed in each.
///
/// LiveKit keeps an SFU room's SIF trailer on the key provider, set at each
/// room connect (`livekit` 0.7.48 `Room::connect`). The trailer marks the
/// unencrypted silence frames the SFU itself sends while a publisher is muted,
/// which the frame cryptor passes through. A provider shared by two SFU
/// connections holds only the last one's trailer, so the other SFU's mute
/// frames fail to decrypt, and a member muting shows as a decryption failure
/// to anyone receiving them through a second SFU.
#[derive(Clone, Default)]
pub struct Msc4195KeyProviders(Arc<Mutex<Providers>>);

#[derive(Default)]
struct Providers {
    next_id: u64,
    live: Vec<(u64, KeyProvider)>,
    /// Every key installed, so a provider made later starts with them all.
    keys: HashMap<String, BTreeMap<u8, Vec<u8>>>,
}

impl Msc4195KeyProviders {
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider for one SFU connection, holding every key installed so far
    /// and every key installed while it lives.
    pub fn connection_provider(&self) -> ConnectionKeyProvider {
        let provider = msc4195_key_provider();
        let mut providers = self.0.lock().expect("key providers mutex poisoned");
        for (identity, ring) in &providers.keys {
            let identity = ParticipantIdentity::from(identity.clone());
            for (index, key) in ring {
                provider.set_key(&identity, i32::from(*index), key.clone());
            }
        }
        let id = providers.next_id;
        providers.next_id += 1;
        providers.live.push((id, provider.clone()));
        ConnectionKeyProvider {
            id,
            provider,
            providers: self.clone(),
        }
    }

    /// `false` when a live provider refused the key.
    fn set_key(&self, identity: &str, index: u8, key: Vec<u8>) -> bool {
        let mut providers = self.0.lock().expect("key providers mutex poisoned");
        providers
            .keys
            .entry(identity.to_owned())
            .or_default()
            .insert(index, key.clone());
        // `rtc_backend_identity` is what the member's token flow minted
        // (`identity_mapper`), so it maps directly onto the LiveKit
        // participant identity.
        let identity = ParticipantIdentity::from(identity.to_owned());
        providers
            .live
            .iter()
            .fold(true, |installed, (_, provider)| {
                provider.set_key(&identity, i32::from(index), key.clone()) && installed
            })
    }
}

/// One SFU connection's provider; it stops receiving keys when dropped.
pub struct ConnectionKeyProvider {
    id: u64,
    provider: KeyProvider,
    providers: Msc4195KeyProviders,
}

impl ConnectionKeyProvider {
    /// The handle to give the connection's [`livekit::RoomOptions`].
    pub fn provider(&self) -> &KeyProvider {
        &self.provider
    }
}

impl Drop for ConnectionKeyProvider {
    fn drop(&mut self) {
        if let Ok(mut providers) = self.providers.0.lock() {
            providers.live.retain(|(id, _)| *id != self.id);
        }
    }
}

/// [`FrameKeyRing`] over every SFU connection's [`KeyProvider`].
struct LiveKitKeyRing(Msc4195KeyProviders);

#[async_trait]
impl FrameKeyRing for LiveKitKeyRing {
    fn ring_size(&self) -> u16 {
        NATIVE_KEY_RING_MAX as u16
    }

    async fn set_key(&self, identity: &str, index: u8, key: Vec<u8>) -> bool {
        self.0.set_key(identity, index, key)
    }
}

/// A [`MediaKeyBridge`] that installs every signalled key in each of
/// `providers` via [`KeyProvider::set_key`], keyed by the participant's
/// pseudonymous identity and the signalled key index.
///
/// Give the same `providers` to the transport, which takes one per connection.
pub fn msc4195_media_key_bridge(providers: Msc4195KeyProviders) -> MediaKeyBridge {
    MediaKeyHandler::with_ring(Arc::new(LiveKitKeyRing(providers)))
}

#[cfg(test)]
mod tests {
    use matrix_rtc_core::{EncryptionKeySignalHandler, KeyMaterialSignal};

    use super::*;

    #[test]
    fn msc4195_provider_accepts_per_participant_hkdf_import() {
        let provider = msc4195_key_provider();
        let identity = ParticipantIdentity::from("participant-abc".to_owned());

        // Every index the `u8` field can carry, 255 included (R38).
        assert!(provider.set_key(&identity, 0, vec![1u8; 32]));
        assert!(provider.set_key(&identity, 255, vec![2u8; 32]));
        assert!(provider.get_key(&identity, 0).is_some());
        assert_eq!(provider.get_key(&identity, 255), Some(vec![2u8; 32]));
    }

    #[tokio::test]
    async fn bridge_installs_key_index_255() {
        let providers = Msc4195KeyProviders::new();
        let connection = providers.connection_provider();
        let provider = connection.provider().clone();
        let bridge = msc4195_media_key_bridge(providers);

        bridge
            .on_new_key_material(KeyMaterialSignal {
                key: vec![4u8; 32],
                key_index: u8::MAX,
                rtc_backend_identity: "participant-p2".to_owned(),
                use_after_ms: 0,
            })
            .await;

        assert_eq!(bridge.key_for("participant-p2").unwrap().key_index, u8::MAX);
        let identity = ParticipantIdentity::from("participant-p2".to_owned());
        assert_eq!(provider.get_key(&identity, 255), Some(vec![4u8; 32]));
    }

    #[tokio::test]
    async fn bridge_forwards_key_material_to_provider() {
        let providers = Msc4195KeyProviders::new();
        let connection = providers.connection_provider();
        let provider = connection.provider().clone();
        let bridge = msc4195_media_key_bridge(providers);

        bridge
            .on_new_key_material(KeyMaterialSignal {
                key: vec![9u8; 32],
                key_index: 3,
                rtc_backend_identity: "participant-p1".to_owned(),
                use_after_ms: 0,
            })
            .await;

        // Recorded, as before.
        assert!(bridge.key_for("participant-p1").is_some());
        // And imported into the connection's provider.
        let identity = ParticipantIdentity::from("participant-p1".to_owned());
        assert!(provider.get_key(&identity, 3).is_some());
    }

    /// Each SFU connection keeps its own trailer, so each has its own provider,
    /// and a connection made after a key was signalled still gets it.
    #[tokio::test]
    async fn every_connection_gets_every_key_in_its_own_provider() {
        let providers = Msc4195KeyProviders::new();
        let first = providers.connection_provider();
        let bridge = msc4195_media_key_bridge(providers.clone());
        bridge
            .on_new_key_material(KeyMaterialSignal {
                key: vec![7u8; 32],
                key_index: 1,
                rtc_backend_identity: "participant-p3".to_owned(),
                use_after_ms: 0,
            })
            .await;
        let second = providers.connection_provider();

        let identity = ParticipantIdentity::from("participant-p3".to_owned());
        assert!(first.provider().get_key(&identity, 1).is_some());
        assert!(second.provider().get_key(&identity, 1).is_some());

        drop(first);
        assert_eq!(providers.0.lock().unwrap().live.len(), 1);
    }

    #[test]
    fn room_options_encrypt_exactly_when_given_a_key_provider() {
        use livekit::e2ee::EncryptionType;

        let encrypted = crate::room_options(Some(msc4195_key_provider()), true);
        assert_eq!(
            encrypted.encryption.map(|e2ee| e2ee.encryption_type),
            Some(EncryptionType::Gcm)
        );
        // Spec 038 R11: a room whose encryption is off connects in the clear.
        assert!(crate::room_options(None, true).encryption.is_none());
    }
}
