//! DAVE, Discord's end-to-end encryption, for calls and Go Live streams. Its
//! MLS handshake follows songbird's `driver/tasks/ws.rs` (ISC), which ran
//! calls before, but answers with the messages to send instead of sending
//! them.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU16,
};

use davey::{Codec, DaveSession, MediaType, ProposalsOperationType, SessionStatus};

use crate::rtp;
use serenity_voice_model::{
    Event,
    payload::{
        DaveMlsCommitWelcome, DaveMlsInvalidCommitWelcome, DaveMlsKeyPackage,
        DaveMlsProposalsOperationType, DaveTransitionReady,
    },
};

/// A message for the voice gateway.
#[derive(Debug)]
pub enum Out {
    Json(Event),
    Binary(Event),
}

pub struct Dave {
    user_id: u64,
    /// The MLS group's id: a guess until the voice server's messages name it.
    channel_id: u64,
    session: Option<DaveSession>,
    version: u16,
    pending: HashMap<u16, u16>,
    /// Who the voice server says is connected, which proposals must match.
    users: HashSet<u64>,
    /// Why the last encrypted frame couldn't be read, for the stream's log.
    pub unreadable: Option<String>,
}

impl Dave {
    pub fn new(user_id: u64, channel_id: u64) -> Self {
        Self {
            user_id,
            channel_id,
            session: None,
            version: 0,
            pending: HashMap::new(),
            users: HashSet::from([user_id]),
            unreadable: None,
        }
    }

    /// Someone the voice server says is in the stream: a stream server
    /// announces its streamer through video attributes rather than op 11.
    pub fn recognise(&mut self, user: u64) {
        self.users.insert(user);
    }

    /// Starts with the protocol version from the session description.
    pub fn start(&mut self, version: u16) -> Vec<Out> {
        self.version = version;
        self.reinit()
    }

    /// Encrypts a video frame once the group is ready. Until then frames go
    /// out as they are, as Discord's clients send audio, and receivers pass them
    /// through during the transition. None when a ready group couldn't
    /// encrypt it: the frame is dropped rather than sent in the clear.
    pub fn encrypt(&mut self, codec: rtp::Codec, frame: &[u8]) -> Option<Vec<u8>> {
        let codec = match codec {
            rtp::Codec::Vp8 => Codec::VP8,
            rtp::Codec::H264 => Codec::H264,
            rtp::Codec::H265 => Codec::H265,
            rtp::Codec::Av1 => Codec::AV1,
        };
        if self.version != 0
            && let Some(session) = self.session.as_mut()
            && session.is_ready()
        {
            return match session.encrypt(MediaType::VIDEO, codec, frame) {
                Ok(encrypted) => Some(encrypted.into_owned()),
                Err(error) => {
                    tracing::warn!(?error, "could not end-to-end encrypt a frame");
                    None
                }
            };
        }
        Some(frame.to_vec())
    }

    /// End-to-end encrypts an Opus frame once the group is ready, as
    /// [`Self::encrypt`] does video.
    pub fn encrypt_opus(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
        if self.version != 0
            && let Some(session) = self.session.as_mut()
            && session.is_ready()
        {
            return match session.encrypt_opus(frame) {
                Ok(encrypted) => Some(encrypted.into_owned()),
                Err(error) => {
                    tracing::warn!(?error, "could not end-to-end encrypt sound");
                    None
                }
            };
        }
        Some(frame.to_vec())
    }

    /// Decrypts a received frame from `sender`. Before the group is ready,
    /// frames without DAVE's marker pass through, as Discord's clients allow;
    /// once it is, only during a transition, which the session times, so the
    /// voice server can't slip unencrypted frames into an encrypted call.
    /// Frames that can't be decrypted are dropped.
    pub fn decrypt(&mut self, sender: u64, frame: Vec<u8>) -> Option<Vec<u8>> {
        self.decrypt_media(sender, MediaType::VIDEO, frame)
    }

    /// Decrypts a received Opus frame from `sender`, as [`Self::decrypt`]
    /// does video.
    pub fn decrypt_opus(&mut self, sender: u64, frame: Vec<u8>) -> Option<Vec<u8>> {
        self.decrypt_media(sender, MediaType::AUDIO, frame)
    }

    fn decrypt_media(&mut self, sender: u64, media: MediaType, frame: Vec<u8>) -> Option<Vec<u8>> {
        // https://daveprotocol.com/#protocol-frame-check: a tag, a size
        // byte and the magic marker at least.
        if self.version == 0 {
            return Some(frame);
        }
        let Some(session) = self.session.as_mut().filter(|session| session.is_ready()) else {
            if frame.len() < 11 || !frame.ends_with(&[0xfa, 0xfa]) {
                return Some(frame);
            }
            self.unreadable = Some("not in the group yet".to_owned());
            return None;
        };
        match session.decrypt(sender, media, &frame) {
            Ok(decrypted) => Some(decrypted),
            Err(error) => {
                self.unreadable = Some(format!("{error:?}"));
                None
            }
        }
    }

    /// Whether frames leave end-to-end encrypted.
    pub fn encrypting(&self) -> bool {
        self.version != 0 && self.session.as_ref().is_some_and(DaveSession::is_ready)
    }

    /// The group's voice privacy code, 30 digits, which Discord's clients
    /// show so the people in a call can compare it, while frames leave
    /// end-to-end encrypted.
    pub fn privacy_code(&self) -> Option<&str> {
        self.session
            .as_ref()
            .filter(|_| self.encrypting())
            .and_then(DaveSession::voice_privacy_code)
    }

    /// Where the MLS handshake is, for the log: with the group's epoch and
    /// voice privacy code, which Discord's clients show in a call's
    /// encryption details, so the two ends can be compared.
    pub fn status(&self) -> String {
        match &self.session {
            _ if self.version == 0 => "off".to_owned(),
            Some(session) => format!(
                "{:?}, ready {}, epoch {}, code {}",
                session.status(),
                session.is_ready(),
                session.epoch().map_or(0, |epoch| epoch.as_u64()),
                session.voice_privacy_code().unwrap_or("none"),
            ),
            None => "no session".to_owned(),
        }
    }

    pub fn handle(&mut self, event: Event) -> Vec<Out> {
        match event {
            Event::ClientsConnect(event) => {
                self.users.extend(event.user_ids.iter().map(|user| user.0));
                vec![]
            }
            Event::ClientDisconnect(event) => {
                self.users.remove(&event.user_id.0);
                vec![]
            }
            Event::DavePrepareTransition(event) => {
                tracing::info!(
                    transition_id = event.transition_id,
                    version = event.protocol_version,
                    "DAVE prepares a transition"
                );
                self.pending
                    .insert(event.transition_id, event.protocol_version);
                if event.transition_id == 0 {
                    self.execute(event.transition_id);
                    vec![]
                } else {
                    if event.protocol_version == 0
                        && let Some(session) = self.session.as_mut()
                    {
                        session.set_passthrough_mode(true, Some(120));
                    }
                    vec![self.ready(event.transition_id, event.protocol_version)]
                }
            }
            Event::DaveExecuteTransition(event) => {
                self.execute(event.transition_id);
                tracing::info!(
                    transition_id = event.transition_id,
                    dave = self.status(),
                    "DAVE executed a transition"
                );
                vec![]
            }
            Event::DavePrepareEpoch(event) if event.epoch == 1 => {
                tracing::info!(
                    version = event.protocol_version,
                    "DAVE starts a new group; sending a new key package"
                );
                self.version = event.protocol_version;
                self.reinit()
            }
            Event::DaveMlsExternalSender(event) => {
                if let Some(session) = self.session.as_mut()
                    && let Err(error) = session.set_external_sender(&event.external_sender)
                {
                    tracing::warn!(?error, "could not set the MLS external sender");
                }
                vec![]
            }
            Event::DaveMlsProposals(event) => {
                let operation = match event.operation_type {
                    DaveMlsProposalsOperationType::Append => ProposalsOperationType::APPEND,
                    DaveMlsProposalsOperationType::Revoke => ProposalsOperationType::REVOKE,
                };
                if operation == ProposalsOperationType::APPEND {
                    self.follow(proposals_group(&event.proposals));
                }
                let mut out = vec![];
                let users: Vec<u64> = self.users.iter().copied().collect();
                let result = self.session.as_mut().map(|session| {
                    session.process_proposals(operation, &event.proposals, Some(&users))
                });
                match result {
                    Some(Ok(Some(commit))) => {
                        tracing::info!(
                            welcome = commit.welcome.is_some(),
                            "committing DAVE proposals"
                        );
                        out.push(Out::Binary(Event::from(DaveMlsCommitWelcome {
                            commit: commit.commit,
                            welcome: commit.welcome,
                        })));
                    }
                    // Until the welcome arrives, proposals for the group
                    // others are in don't match Yapfast's empty one.
                    Some(Err(error))
                        if self
                            .session
                            .as_ref()
                            .is_some_and(|session| session.status() != SessionStatus::ACTIVE) =>
                    {
                        tracing::info!(?error, "DAVE proposals for a group Yapfast isn't in yet");
                    }
                    Some(Err(error)) => {
                        tracing::warn!(?error, "could not process MLS proposals");
                    }
                    _ => {}
                }
                out
            }
            Event::DaveMlsAnnounceCommitTransition(event) => {
                self.follow(message_group(&event.commit_message));
                // Before Yapfast has a group, or has committed to its pending
                // one, a commit is for a group a welcome will bring it
                // into. libdave ignores it; refusing it would start over
                // and lose the key package that welcome is sealed to.
                if !self.session.as_ref().is_some_and(|session| {
                    matches!(
                        session.status(),
                        SessionStatus::AWAITING_RESPONSE | SessionStatus::ACTIVE
                    )
                }) {
                    tracing::info!(
                        transition_id = event.transition_id,
                        "a DAVE commit before Yapfast is in the group; waiting for its welcome"
                    );
                    return vec![];
                }
                let result = self
                    .session
                    .as_mut()
                    .map(|session| session.process_commit(&event.commit_message));
                self.joined(
                    event.transition_id,
                    result.map(|result| result.map_err(|error| format!("{error:?}"))),
                )
            }
            Event::DaveMlsWelcome(event) => {
                let result = self
                    .session
                    .as_mut()
                    .map(|session| session.process_welcome(&event.welcome));
                self.joined(
                    event.transition_id,
                    result.map(|result| result.map_err(|error| format!("{error:?}"))),
                )
            }
            _ => vec![],
        }
    }

    /// Moves to the group the voice server's messages name, when it isn't
    /// the guessed one. A stream's group isn't its voice channel's.
    fn follow(&mut self, group: Option<u64>) {
        let Some(group) = group.filter(|&group| group != self.channel_id) else {
            return;
        };
        tracing::info!(
            group,
            guessed = self.channel_id,
            "the stream's DAVE group isn't the one guessed; moving to it"
        );
        self.channel_id = group;
        if let Some(session) = self.session.as_mut()
            && let Err(error) = session.set_channel_id(group)
        {
            tracing::warn!(?error, "could not move to the stream's DAVE group");
        }
    }

    /// After a commit or welcome: ready for the transition, or start over.
    fn joined(&mut self, transition_id: u16, result: Option<Result<(), String>>) -> Vec<Out> {
        if let Some(Ok(())) = result {
            tracing::info!(
                transition_id,
                dave = self.status(),
                "applied a DAVE commit or welcome"
            );
        }
        match result {
            Some(Ok(())) if transition_id != 0 => {
                self.pending.insert(transition_id, self.version);
                vec![self.ready(transition_id, self.version)]
            }
            Some(Err(error)) => {
                tracing::warn!(%error, "MLS commit or welcome failed");
                let mut out = vec![Out::Json(Event::from(DaveMlsInvalidCommitWelcome {
                    transition_id,
                }))];
                out.extend(self.reinit());
                out
            }
            _ => vec![],
        }
    }

    fn ready(&self, transition_id: u16, protocol_version: u16) -> Out {
        Out::Json(Event::from(DaveTransitionReady {
            transition_id,
            protocol_version,
        }))
    }

    fn reinit(&mut self) -> Vec<Out> {
        let Some(version) = NonZeroU16::new(self.version) else {
            if let Some(session) = self.session.as_mut() {
                if let Err(error) = session.reset() {
                    tracing::warn!(?error, "could not reset the DAVE session");
                }
                session.set_passthrough_mode(true, Some(10));
            }
            return vec![];
        };
        let key_package = match self.session.as_mut() {
            Some(session) => session
                .reinit(version, self.user_id, self.channel_id, None)
                .map_err(|error| format!("{error:?}"))
                .and_then(|()| {
                    session
                        .create_key_package()
                        .map_err(|error| format!("{error:?}"))
                }),
            None => DaveSession::new(version, self.user_id, self.channel_id, None)
                .map_err(|error| format!("{error:?}"))
                .and_then(|mut session| {
                    let key_package = session
                        .create_key_package()
                        .map_err(|error| format!("{error:?}"));
                    self.session = Some(session);
                    key_package
                }),
        };
        match key_package {
            Ok(key_package) => vec![Out::Binary(Event::DaveMlsKeyPackage(DaveMlsKeyPackage {
                key_package,
            }))],
            Err(error) => {
                tracing::warn!(%error, "could not start the DAVE session");
                vec![]
            }
        }
    }

    fn execute(&mut self, transition_id: u16) {
        let Some(version) = self.pending.remove(&transition_id) else {
            tracing::warn!(transition_id, "DAVE transition to an unknown id");
            return;
        };
        let old = std::mem::replace(&mut self.version, version);
        // Upgraded from transport-only encryption.
        if transition_id > 0
            && old == 0
            && version != 0
            && let Some(session) = self.session.as_mut()
        {
            session.set_passthrough_mode(true, Some(10));
        }
    }
}

/// The group the first of some appended proposals belongs to, as DAVE's
/// u64 id. Proposals are a length-prefixed run of MLS messages.
fn proposals_group(proposals: &[u8]) -> Option<u64> {
    message_group(vector(proposals)?.0)
}

/// The group of an MLS message: a version, a wire format of 1 (a public
/// message, the only kind DAVE sends in the clear) and then the group id
/// (RFC 9420, 6 and 6.2).
fn message_group(message: &[u8]) -> Option<u64> {
    let (version, rest) = message.split_first_chunk::<2>()?;
    let (format, rest) = rest.split_first_chunk::<2>()?;
    if u16::from_be_bytes(*version) != 1 || u16::from_be_bytes(*format) != 1 {
        return None;
    }
    let (group, _) = vector(rest)?;
    Some(u64::from_be_bytes(group.try_into().ok()?))
}

/// An MLS variable-length vector (RFC 9420, 2.1.2): its contents and what
/// follows.
fn vector(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let first = *bytes.first()?;
    let prefix = 1usize << (first >> 6);
    if prefix > 4 || bytes.len() < prefix {
        return None;
    }
    let length = bytes[1..prefix]
        .iter()
        .fold(usize::from(first & 0x3f), |length, &byte| {
            length << 8 | usize::from(byte)
        });
    let rest = &bytes[prefix..];
    (rest.len() >= length).then(|| rest.split_at(length))
}

#[cfg(test)]
mod tests {
    use serenity_voice_model::payload::{DaveExecuteTransition, DavePrepareTransition};

    use super::*;

    #[test]
    fn starts_with_a_key_package_and_passes_through_without_dave() {
        let mut dave = Dave::new(1, 2);
        assert!(dave.start(0).is_empty());
        assert_eq!(
            dave.encrypt(rtp::Codec::Vp8, b"vp8").unwrap(),
            b"vp8",
            "transport encryption only"
        );
        let marked = [&[0u8; 9][..], &[0xfa, 0xfa]].concat();
        assert_eq!(dave.decrypt(2, marked.clone()), Some(marked.clone()));
        let out = dave.start(1);
        assert!(matches!(
            out.as_slice(),
            [Out::Binary(Event::DaveMlsKeyPackage(_))]
        ));
        // No group yet, so frames still pass, and encrypted ones wait.
        assert_eq!(dave.encrypt(rtp::Codec::Vp8, b"vp8").unwrap(), b"vp8");
        assert_eq!(dave.decrypt(2, b"plain".to_vec()), Some(b"plain".to_vec()));
        assert_eq!(dave.decrypt(2, marked), None);
    }

    #[test]
    fn reads_the_group_proposals_belong_to() {
        let group = 1_400_000_000_000_000_777u64;
        // version 1, a public message, the group id, then the rest.
        let message = [&[0, 1, 0, 1, 8][..], &group.to_be_bytes(), &[0; 40]].concat();
        let short = [&[message.len() as u8][..], &message].concat();
        assert_eq!(proposals_group(&short), Some(group));
        let long = [&[0x40, message.len() as u8][..], &message].concat();
        assert_eq!(proposals_group(&long), Some(group));
        assert_eq!(proposals_group(&long[..20]), None, "cut short");
        let welcome = [&[0, 1, 0, 3][..], &message[4..]].concat();
        assert_eq!(
            proposals_group(&[&[welcome.len() as u8][..], &welcome].concat()),
            None
        );
        assert_eq!(message_group(&message), Some(group), "a commit");
    }

    /// The voice server's side of DAVE: the external sender that proposes
    /// people into groups.
    struct Server {
        key: openmls_basic_credential::SignatureKeyPair,
    }

    impl Server {
        fn new() -> Self {
            let key = openmls_basic_credential::SignatureKeyPair::new(
                openmls::prelude::SignatureScheme::ECDSA_SECP256R1_SHA256,
            )
            .unwrap();
            Self { key }
        }

        /// What op 25 carries.
        fn external_sender(&self) -> Vec<u8> {
            use openmls::prelude::{BasicCredential, ExternalSender, tls_codec::Serialize};
            ExternalSender::new(
                self.key.public().to_vec().into(),
                BasicCredential::new(vec![0; 8]).into(),
            )
            .tls_serialize_detached()
            .unwrap()
        }

        /// What op 27 carries to add the owner of a key package to `group`.
        fn propose_add(&self, key_package: &[u8], group: u64) -> Vec<u8> {
            use openmls::prelude::{
                ExternalProposal, GroupEpoch, GroupId, KeyPackageIn, ProtocolVersion,
                SenderExtensionIndex,
                tls_codec::{Deserialize, Serialize, VLBytes},
            };
            use openmls_rust_crypto::OpenMlsRustCrypto;
            let provider = OpenMlsRustCrypto::default();
            let key_package = KeyPackageIn::tls_deserialize_exact(key_package)
                .unwrap()
                .validate(
                    openmls::prelude::OpenMlsProvider::crypto(&provider),
                    ProtocolVersion::Mls10,
                )
                .unwrap();
            let message = ExternalProposal::new_add::<OpenMlsRustCrypto>(
                key_package,
                GroupId::from_slice(&group.to_be_bytes()),
                GroupEpoch::from(0),
                &self.key,
                SenderExtensionIndex::new(0),
            )
            .unwrap()
            .tls_serialize_detached()
            .unwrap();
            VLBytes::new(message).tls_serialize_detached().unwrap()
        }
    }

    fn key_package(out: &[Out]) -> Vec<u8> {
        out.iter()
            .find_map(|out| match out {
                Out::Binary(Event::DaveMlsKeyPackage(package)) => Some(package.key_package.clone()),
                _ => None,
            })
            .expect("a key package")
    }

    /// A viewer guesses the wrong group, sends its key package, then learns
    /// the group from the server's proposals while the streamer, already in
    /// it, commits them. The welcome must still open.
    #[test]
    fn joins_a_stream_group_it_guessed_wrong() {
        use davey::DaveSession;
        let (streamer, viewer, group) = (10, 20, 77);
        let server = Server::new();
        let sender = serenity_voice_model::payload::DaveMlsExternalSender {
            external_sender: server.external_sender(),
        };

        let mut host = DaveSession::new(1.try_into().unwrap(), streamer, group, None).unwrap();
        host.set_external_sender(&sender.external_sender).unwrap();
        host.create_key_package().unwrap();

        let mut dave = Dave::new(viewer, 2);
        dave.recognise(streamer);
        let package = key_package(&dave.start(1));
        dave.handle(Event::from(sender));

        let proposals = server.propose_add(&package, group);
        let commit = host
            .process_proposals(ProposalsOperationType::APPEND, &proposals, None)
            .unwrap()
            .expect("the streamer commits the viewer's addition");
        // Discord sends the proposals to everyone; the viewer learns the
        // group from them and doesn't make a new key package.
        let out = dave.handle(Event::from(
            serenity_voice_model::payload::DaveMlsProposals {
                operation_type: DaveMlsProposalsOperationType::Append,
                proposals: proposals.clone(),
            },
        ));
        assert_eq!(dave.channel_id, group);
        assert!(
            !out.iter()
                .any(|out| matches!(out, Out::Binary(Event::DaveMlsKeyPackage(_)))),
            "{out:?}"
        );

        let out = dave.handle(Event::from(serenity_voice_model::payload::DaveMlsWelcome {
            transition_id: 4,
            welcome: commit.welcome.expect("a welcome for the viewer"),
        }));
        assert!(
            matches!(out.as_slice(), [Out::Json(Event::DaveTransitionReady(ready))] if ready.transition_id == 4),
            "{out:?}"
        );
        dave.handle(Event::from(
            serenity_voice_model::payload::DaveExecuteTransition { transition_id: 4 },
        ));
        assert!(dave.session.as_ref().unwrap().is_ready());

        // The server announces the commit to the streamer, who then sends
        // an encrypted H264 frame. Packetized, reassembled and decrypted, it
        // arrives as it was.
        host.process_commit(&commit.commit).unwrap();
        assert!(host.is_ready());
        let idr: Vec<u8> = [0x65]
            .into_iter()
            .chain((0..3000u32).map(|index| (index % 251) as u8 + 1))
            .collect();
        let frame = [
            &[0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0x8c][..],
            &[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80],
            &[0, 0, 0, 1],
            &idr,
        ]
        .concat();
        let sealed = host
            .encrypt(MediaType::VIDEO, Codec::H264, &frame)
            .unwrap()
            .into_owned();
        assert!(sealed.ends_with(&[0xfa, 0xfa]));
        let mut packetizer = rtp::Packetizer::new(9, rtp::Codec::H264);
        let mut assembler = rtp::Assembler::new(rtp::Codec::H264);
        let whole = packetizer
            .packets(&sealed, 0, false)
            .iter()
            .find_map(|packet| assembler.push(&rtp::parse(packet).unwrap()).frame)
            .expect("the frame, reassembled");
        assert_eq!(dave.decrypt(streamer, whole.data), Some(frame));

        // AV1: a temporal delimiter, which DAVE and the packetizer drop, a
        // sequence header and a frame OBU of 2500 bytes; DAVE leaves the
        // last OBU without its size, before the trailer.
        let tile: Vec<u8> = (0..2500u32).map(|index| (index % 251) as u8).collect();
        let frame = [
            &[0x12, 0][..],
            &[1 << 3 | 2, 11],
            &[7; 11],
            &[6 << 3 | 2, 0xc4, 0x13],
            &tile,
        ]
        .concat();
        let sealed = host
            .encrypt(MediaType::VIDEO, Codec::AV1, &frame)
            .unwrap()
            .into_owned();
        let mut packetizer = rtp::Packetizer::new(9, rtp::Codec::Av1);
        let mut assembler = rtp::Assembler::new(rtp::Codec::Av1);
        let whole = packetizer
            .packets(&sealed, 3000, true)
            .iter()
            .find_map(|packet| assembler.push(&rtp::parse(packet).unwrap()).frame)
            .expect("the frame, reassembled");
        let opened = dave.decrypt(streamer, whole.data).expect("decrypted");
        assert_eq!(rtp::obu_elements(&opened), rtp::obu_elements(&frame));
    }

    /// Someone already in a call commits Yapfast's addition, and the voice
    /// server announces that commit before the welcome reaches Yapfast. Yapfast
    /// must wait for the welcome rather than refuse the commit and start
    /// over, then hear and be heard.
    #[test]
    fn waits_for_its_welcome_past_a_commit() {
        use davey::DaveSession;
        let (friend, me, channel) = (10, 20, 77);
        let server = Server::new();
        let sender = serenity_voice_model::payload::DaveMlsExternalSender {
            external_sender: server.external_sender(),
        };
        let mut other = DaveSession::new(1.try_into().unwrap(), friend, channel, None).unwrap();
        other.set_external_sender(&sender.external_sender).unwrap();
        other.create_key_package().unwrap();

        let mut dave = Dave::new(me, channel);
        dave.recognise(friend);
        let package = key_package(&dave.start(1));
        dave.handle(Event::from(sender));
        let commit = other
            .process_proposals(
                ProposalsOperationType::APPEND,
                &server.propose_add(&package, channel),
                None,
            )
            .unwrap()
            .expect("the friend commits Yapfast's addition");

        let out = dave.handle(Event::from(
            serenity_voice_model::payload::DaveMlsAnnounceCommitTransition {
                transition_id: 3,
                commit_message: commit.commit.clone(),
            },
        ));
        assert!(out.is_empty(), "no refusal, no new key package: {out:?}");
        let out = dave.handle(Event::from(serenity_voice_model::payload::DaveMlsWelcome {
            transition_id: 3,
            welcome: commit.welcome.expect("a welcome for Yapfast"),
        }));
        assert!(
            matches!(out.as_slice(), [Out::Json(Event::DaveTransitionReady(ready))] if ready.transition_id == 3),
            "{out:?}"
        );
        dave.handle(Event::from(DaveExecuteTransition { transition_id: 3 }));
        other.process_commit(&commit.commit).unwrap();

        let sealed = other.encrypt_opus(b"friend's voice").unwrap().into_owned();
        assert_eq!(
            dave.decrypt_opus(friend, sealed),
            Some(b"friend's voice".to_vec())
        );
        assert_eq!(
            dave.decrypt_opus(friend, b"the server's voice".to_vec()),
            None,
            "an unencrypted frame in an encrypted call"
        );
        let sealed = dave.encrypt_opus(b"my voice").unwrap();
        assert!(sealed.ends_with(&[0xfa, 0xfa]), "end-to-end encrypted");
        assert_eq!(
            other.decrypt(me, MediaType::AUDIO, &sealed).unwrap(),
            b"my voice"
        );
    }

    #[test]
    fn moves_to_the_proposed_group_keeping_its_key_package() {
        let mut dave = Dave::new(1, 2);
        dave.start(1);
        dave.handle(Event::from(
            serenity_voice_model::payload::DaveMlsExternalSender {
                external_sender: Server::new().external_sender(),
            },
        ));
        let message = [&[0, 1, 0, 1, 8][..], &77u64.to_be_bytes(), &[0; 40]].concat();
        let out = dave.handle(Event::from(
            serenity_voice_model::payload::DaveMlsProposals {
                operation_type: DaveMlsProposalsOperationType::Append,
                proposals: [&[message.len() as u8][..], &message].concat(),
            },
        ));
        let session = dave.session.as_ref().unwrap();
        assert_eq!(session.channel_id(), 77);
        assert!(
            session.group().is_some(),
            "a pending group under the new id"
        );
        assert!(out.is_empty(), "no new key package: {out:?}");
    }

    #[test]
    fn follows_the_group_a_commit_names() {
        let mut dave = Dave::new(1, 2);
        dave.start(1);
        let commit = [&[0, 1, 0, 1, 8][..], &77u64.to_be_bytes(), &[0; 40]].concat();
        dave.handle(Event::from(
            serenity_voice_model::payload::DaveMlsAnnounceCommitTransition {
                transition_id: 3,
                commit_message: commit,
            },
        ));
        assert_eq!(dave.session.as_ref().unwrap().channel_id(), 77);
    }

    #[test]
    fn answers_transitions() {
        let mut dave = Dave::new(1, 2);
        dave.start(1);
        let out = dave.handle(Event::from(DavePrepareTransition {
            protocol_version: 0,
            transition_id: 5,
        }));
        assert!(
            matches!(out.as_slice(), [Out::Json(Event::DaveTransitionReady(ready))] if ready.transition_id == 5)
        );
        dave.handle(Event::from(DaveExecuteTransition { transition_id: 5 }));
        assert_eq!(dave.version, 0);
    }
}
