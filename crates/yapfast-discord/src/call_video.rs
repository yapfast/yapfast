//! A call's camera video: this account's camera out, announced with op 12,
//! and everyone else's in, as op 15 asks for it. Sending and receiving work
//! as a Go Live stream's do (`stream_connection.rs`): the same packetizing,
//! resending on NACK, reassembly and keyframe requests, and DAVE.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc as channel,
    },
    time::Duration,
};

use anyhow::Result;
use serde_json::{Map, Value, json};
use tokio::{sync::mpsc, time::Instant};

use crate::{
    rtp::{self, Assembler, Codec, Packetizer},
    stream_connection::{Frame, SEND_CODEC, video_attributes},
    voice_connection::{Connection, send_json},
};

/// How often a receiver may ask a sender for a keyframe.
const KEYFRAME_REQUESTS: Duration = Duration::from_millis(500);
/// How often a sender sends an RTCP sender report.
const SENDER_REPORTS: Duration = Duration::from_secs(1);
/// The most packets resent for one NACK.
const RESENDS: usize = 128;

/// The camera's frames on their way to the call, and what the call asks of
/// the encoder.
pub struct Camera {
    pub(crate) frames: mpsc::Receiver<Frame>,
    pub(crate) keyframe: Arc<AtomicBool>,
    pub(crate) size: (u32, u32),
    pub(crate) fps: u32,
}

/// Where the encoder sends the camera's frames.
pub struct CameraSender {
    frames: mpsc::Sender<Frame>,
    keyframe: Arc<AtomicBool>,
}

impl CameraSender {
    /// Queues a frame. A full queue drops it and asks for a keyframe.
    pub fn send(&self, frame: Frame) {
        if self.frames.try_send(frame).is_err() {
            self.keyframe.store(true, Ordering::Relaxed);
        }
    }

    /// Whether the next frame should be a keyframe: at the start, after a
    /// dropped frame, or when someone watching asked.
    pub fn take_keyframe_request(&self) -> bool {
        self.keyframe.swap(false, Ordering::Relaxed)
    }
}

/// A camera's frames from the encoder to the call: `width` by `height` at
/// `fps`, in [`SEND_CODEC`].
pub fn camera(width: u32, height: u32, fps: u32) -> (CameraSender, Camera) {
    // A few frames of slack; beyond that the network can't keep up.
    let (frames, receiver) = mpsc::channel(4);
    let keyframe = Arc::new(AtomicBool::new(true));
    (
        CameraSender {
            frames,
            keyframe: keyframe.clone(),
        },
        Camera {
            frames: receiver,
            keyframe,
            size: (width, height),
            fps,
        },
    )
}

/// Someone else's camera, whole frames as they sent them.
pub struct VideoFrame {
    pub user: u64,
    pub codec: Codec,
    pub data: Vec<u8>,
    pub keyframe: bool,
}

/// This account's camera, sending.
pub(crate) struct Sending {
    camera: Camera,
    packetizer: Packetizer,
    history: rtp::History,
    packets: u32,
    octets: u32,
    reported: Instant,
}

impl Sending {
    pub fn new(camera: Camera, connection: &Connection) -> Self {
        Self {
            camera,
            packetizer: Packetizer::new(connection.video, SEND_CODEC),
            history: rtp::History::default(),
            packets: 0,
            octets: 0,
            reported: Instant::now() - SENDER_REPORTS,
        }
    }

    /// Announces the camera with op 12, as on a new connection.
    pub async fn announce(&mut self, connection: &mut Connection) -> Result<()> {
        self.packetizer = Packetizer::new(connection.video, SEND_CODEC);
        self.camera.keyframe.store(true, Ordering::Relaxed);
        let (ssrc, video, rtx) = (connection.ssrc, connection.video, connection.rtx);
        send_json(
            &mut connection.ws,
            12,
            video_attributes(ssrc, video, rtx, self.camera.size, self.camera.fps),
        )
        .await
    }

    /// The camera's next frame, waiting for it.
    pub async fn next(&mut self) -> Option<Frame> {
        self.camera.frames.recv().await
    }

    pub async fn send(&mut self, connection: &mut Connection, frame: Frame) -> Result<()> {
        let Some(payload) = connection.dave.encrypt(SEND_CODEC, &frame.data) else {
            return Ok(());
        };
        for mut packet in self
            .packetizer
            .packets(&payload, frame.timestamp, frame.keyframe)
        {
            self.history.remember(&packet);
            let clear = rtp::rtp_clear_len(&packet).unwrap_or(packet.len());
            self.packets = self.packets.wrapping_add(1);
            self.octets = self.octets.wrapping_add((packet.len() - clear) as u32);
            connection.transport.seal(&mut packet, clear)?;
            connection.udp.send(&packet).await?;
        }
        if self.reported.elapsed() >= SENDER_REPORTS {
            self.reported = Instant::now();
            let mut report = rtp::sender_report(
                connection.video,
                std::time::SystemTime::now(),
                frame.timestamp,
                self.packets,
                self.octets,
            );
            connection.transport.seal(&mut report, rtp::RTCP_CLEAR)?;
            connection.udp.send(&report).await?;
        }
        Ok(())
    }

    /// Answers RTCP about the camera: resends what was lost, and passes a
    /// keyframe request to the encoder.
    pub async fn feedback(&mut self, connection: &mut Connection, rtcp: &[u8]) -> Result<()> {
        if rtp::wants_keyframe(rtcp) {
            self.camera.keyframe.store(true, Ordering::Relaxed);
        }
        for lost in rtp::nacked(rtcp, connection.video)
            .into_iter()
            .take(RESENDS)
        {
            let Some(mut again) = self
                .history
                .get(lost)
                .and_then(|original| self.packetizer.retransmit(original, connection.rtx))
            else {
                continue;
            };
            let clear = rtp::rtp_clear_len(&again).unwrap_or(again.len());
            connection.transport.seal(&mut again, clear)?;
            connection.udp.send(&again).await?;
        }
        Ok(())
    }

    /// Tells the call the camera stopped: no video SSRC, and no streams.
    pub async fn clear(connection: &mut Connection) -> Result<()> {
        send_json(
            &mut connection.ws,
            12,
            json!({"audio_ssrc": connection.ssrc, "video_ssrc": 0, "rtx_ssrc": 0, "streams": []}),
        )
        .await
    }
}

/// One person's camera on its way to whole frames.
struct Incoming {
    user: u64,
    rtx: Option<u32>,
    reorder: rtp::Reorder,
    assembler: Option<Assembler>,
    asked: Instant,
}

/// Everyone else's cameras: who films on which SSRC, what Yapfast asked for,
/// and the frames on their way.
pub(crate) struct Receiving {
    /// By video SSRC.
    incoming: HashMap<u32, Incoming>,
    /// The quality asked for each person's camera, 0 to 100, and the
    /// pixels it shows at.
    wants: HashMap<u64, (u8, u32)>,
    frames: channel::SyncSender<VideoFrame>,
}

impl Receiving {
    pub fn new(frames: channel::SyncSender<VideoFrame>) -> Self {
        Self {
            incoming: HashMap::new(),
            wants: HashMap::new(),
            frames,
        }
    }

    /// A new connection: SSRCs start over, and are announced again.
    pub fn reset(&mut self) {
        self.incoming.clear();
    }

    /// Learns who films on which SSRC from op 12; a zero video SSRC means
    /// their camera stopped. Says whether op 15 should be sent again.
    pub fn learn(&mut self, op: u64, data: &Value) -> bool {
        if op != 12 {
            return false;
        }
        let Some(user) = data["user_id"]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok())
        else {
            return false;
        };
        self.incoming.retain(|_, incoming| incoming.user != user);
        let Some(video) = data["video_ssrc"].as_u64().filter(|&ssrc| ssrc != 0) else {
            return false;
        };
        let video = video as u32;
        let stream = data["streams"].as_array().and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream["ssrc"].as_u64() == Some(u64::from(video)))
        });
        // RTX follows the video SSRC when the stream doesn't say.
        let rtx = stream
            .and_then(|stream| stream["rtx_ssrc"].as_u64())
            .map_or(video.wrapping_add(1), |ssrc| ssrc as u32);
        self.incoming.insert(
            video,
            Incoming {
                user,
                rtx: Some(rtx),
                reorder: rtp::Reorder::default(),
                assembler: None,
                asked: Instant::now() - KEYFRAME_REQUESTS,
            },
        );
        true
    }

    /// Sets the quality asked for each person's camera; those left out get
    /// none.
    pub fn want(&mut self, wants: HashMap<u64, (u8, u32)>) {
        self.wants = wants;
    }

    /// Asks the server for the cameras wanted, at their quality, and none
    /// of the others (op 15). Everything else, everyone's sound above all,
    /// is wanted in full: `any` covers every stream left unnamed, and 0
    /// stops one.
    pub async fn ask(&mut self, connection: &mut Connection) -> Result<()> {
        let mut asked = Map::new();
        let mut pixels = Map::new();
        asked.insert("any".into(), json!(100));
        for (&ssrc, incoming) in &mut self.incoming {
            let (quality, shown) = self.wants.get(&incoming.user).copied().unwrap_or((0, 0));
            asked.insert(ssrc.to_string(), json!(quality));
            if quality > 0 {
                pixels.insert(ssrc.to_string(), json!(shown));
                // The decoder starts from a keyframe.
                incoming.assembler = None;
            }
        }
        asked.insert("pixelCounts".into(), Value::Object(pixels));
        send_json(&mut connection.ws, 15, Value::Object(asked)).await
    }

    /// Takes a received video packet, if it's a camera's.
    pub async fn receive(
        &mut self,
        connection: &mut Connection,
        packet: &rtp::Packet<'_>,
    ) -> Result<bool> {
        let held = if let Some((&video, _)) = self
            .incoming
            .iter()
            .find(|(_, incoming)| incoming.rtx == Some(packet.ssrc))
        {
            match rtp::Held::unwrap_rtx(packet, video) {
                Some(held) => held,
                None => return Ok(true),
            }
        } else if self.incoming.contains_key(&packet.ssrc) {
            rtp::Held::new(packet)
        } else {
            return Ok(false);
        };
        let Some(incoming) = self.incoming.get_mut(&held.ssrc) else {
            return Ok(true);
        };
        let Some(codec) = Codec::from_payload_type(held.packet().payload_type)
            .or(connection.codec)
            .or_else(|| incoming.assembler.as_ref().map(Assembler::codec))
        else {
            return Ok(true);
        };
        if incoming
            .assembler
            .as_ref()
            .is_none_or(|assembler| assembler.codec() != codec)
        {
            incoming.assembler = Some(Assembler::new(codec));
        }
        incoming.reorder.insert(held, std::time::Instant::now());
        Ok(true)
    }

    /// Puts frames together from what arrived, hands them on, and asks
    /// again for what's missing.
    pub async fn repair(&mut self, connection: &mut Connection) -> Result<()> {
        let now = std::time::Instant::now();
        for (&ssrc, incoming) in &mut self.incoming {
            let Some(assembler) = incoming.assembler.as_mut() else {
                continue;
            };
            let codec = assembler.codec();
            let mut lost = false;
            let (ready, _) = incoming.reorder.ready(now);
            for held in ready {
                let pushed = assembler.push(&held.packet());
                lost |= pushed.lost;
                let Some(frame) = pushed.frame else { continue };
                let Some(data) = connection.dave.decrypt(incoming.user, frame.data) else {
                    continue;
                };
                let frame = VideoFrame {
                    user: incoming.user,
                    codec,
                    data,
                    keyframe: frame.keyframe,
                };
                if let Err(channel::TrySendError::Full(_)) = self.frames.try_send(frame) {
                    // Decoding is behind; what follows needs this frame.
                    assembler.wait_for_keyframe();
                    lost = true;
                }
            }
            let missing = incoming.reorder.to_ask(now);
            if !missing.is_empty() {
                let mut request = rtp::nack(connection.ssrc, ssrc, &missing);
                connection.transport.seal(&mut request, rtp::RTCP_CLEAR)?;
                connection.udp.send(&request).await?;
            }
            if lost && incoming.asked.elapsed() >= KEYFRAME_REQUESTS {
                incoming.asked = Instant::now();
                let mut request = rtp::picture_loss(connection.ssrc, ssrc);
                connection.transport.seal(&mut request, rtp::RTCP_CLEAR)?;
                connection.udp.send(&request).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_cameras_from_op_12_and_forgets_stopped_ones() {
        let (frames, _) = channel::sync_channel(1);
        let mut receiving = Receiving::new(frames);
        assert!(receiving.learn(
            12,
            &json!({"user_id": "7", "audio_ssrc": 100, "video_ssrc": 101,
                    "streams": [{"type": "video", "rid": "100", "ssrc": 101, "rtx_ssrc": 102, "active": true}]})
        ));
        assert_eq!(receiving.incoming[&101].rtx, Some(102));
        assert!(receiving.learn(
            12,
            &json!({"user_id": "8", "audio_ssrc": 200, "video_ssrc": 201, "streams": []})
        ));
        assert_eq!(
            receiving.incoming[&201].rtx,
            Some(202),
            "RTX follows when unsaid"
        );
        assert!(!receiving.learn(
            12,
            &json!({"user_id": "7", "audio_ssrc": 100, "video_ssrc": 0, "streams": []})
        ));
        assert!(!receiving.incoming.contains_key(&101), "camera off");
        assert!(!receiving.learn(5, &json!({"user_id": "9", "ssrc": 300})));
    }
}
