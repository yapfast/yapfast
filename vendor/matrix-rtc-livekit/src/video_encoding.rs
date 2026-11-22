// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Maps the transport-neutral video publish settings ([`VideoCodecPreference`],
//! [`VideoEncodingLimits`]) onto livekit's `TrackPublishOptions`, and reports
//! which codecs this platform's libwebrtc build can encode.
//!
//! The encoders available depend on the platform and the build (VP8/VP9
//! always; H.264 through OpenH264, MediaCodec on Android, VideoToolbox on
//! Apple, NVENC/VAAPI when compiled in), so they are asked at runtime.

use livekit::options::{
    TrackPublishOptions, VideoCodec, VideoEncoding, compute_appropriate_encoding,
};
use livekit::prelude::TrackSource;
use livekit::rtc_engine::lk_runtime::LkRuntime;
use livekit::webrtc::MediaType;
use matrix_rtc_transport::{VideoCodecPreference, VideoEncodingLimits, VideoSourceConfig};

/// The video codecs this platform can encode, in the encoder factory's order.
///
/// Asks the same libwebrtc factory livekit publishes with. livekit does not
/// fail a publish whose codec is missing here: it silently falls back to its
/// default, so a host offering a codec choice should offer only these.
pub fn supported_video_codecs() -> Vec<VideoCodecPreference> {
    let capabilities = LkRuntime::instance()
        .pc_factory()
        .get_rtp_sender_capabilities(MediaType::Video);
    codecs_from_mime_types(
        capabilities
            .codecs
            .iter()
            .map(|codec| codec.mime_type.as_str()),
    )
}

/// The codecs named by `mime_types`, deduplicated (H.264 appears once per
/// profile), skipping the non-codec entries (RTX, RED, FEC) and codecs
/// [`VideoCodecPreference`] has no variant for.
fn codecs_from_mime_types<'a>(
    mime_types: impl IntoIterator<Item = &'a str>,
) -> Vec<VideoCodecPreference> {
    let mut codecs = Vec::new();
    for mime_type in mime_types {
        let codec = match mime_type.to_ascii_lowercase().as_str() {
            "video/vp8" => VideoCodecPreference::Vp8,
            "video/h264" => VideoCodecPreference::H264,
            "video/vp9" => VideoCodecPreference::Vp9,
            "video/av1" => VideoCodecPreference::Av1,
            _ => continue,
        };
        if !codecs.contains(&codec) {
            codecs.push(codec);
        }
    }
    codecs
}

/// The livekit publish options for a video publication.
///
/// When any limit is given, the fields left out are filled with what livekit
/// would compute for this source, so a bitrate-only cap keeps livekit's
/// per-source framerate (lower for screen shares) and a framerate-only cap
/// keeps its bitrate.
pub(crate) fn video_publish_options(
    source: TrackSource,
    config: VideoSourceConfig,
    simulcast: bool,
    codec: VideoCodecPreference,
    limits: VideoEncodingLimits,
) -> TrackPublishOptions {
    let video_codec = match codec {
        VideoCodecPreference::Auto | VideoCodecPreference::Vp8 => VideoCodec::VP8,
        VideoCodecPreference::H264 => VideoCodec::H264,
        VideoCodecPreference::Vp9 => VideoCodec::VP9,
        VideoCodecPreference::Av1 => VideoCodec::AV1,
    };
    let video_encoding = (limits != VideoEncodingLimits::default()).then(|| {
        let computed = compute_appropriate_encoding(
            source == TrackSource::Screenshare,
            config.width,
            config.height,
            video_codec,
        );
        VideoEncoding {
            max_bitrate: limits.max_bitrate_bps.unwrap_or(computed.max_bitrate),
            max_framerate: limits
                .max_framerate
                .map_or(computed.max_framerate, f64::from),
        }
    });
    TrackPublishOptions {
        source,
        simulcast,
        video_codec,
        video_encoding,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: VideoSourceConfig = VideoSourceConfig {
        width: 1280,
        height: 720,
    };

    fn options(codec: VideoCodecPreference, limits: VideoEncodingLimits) -> TrackPublishOptions {
        video_publish_options(TrackSource::Camera, HD, true, codec, limits)
    }

    #[test]
    fn defaults_leave_livekit_to_decide() {
        let options = options(VideoCodecPreference::Auto, VideoEncodingLimits::default());
        assert_eq!(options.video_codec, VideoCodec::VP8);
        assert!(options.video_encoding.is_none());
        assert!(options.simulcast);
        assert_eq!(options.source, TrackSource::Camera);
    }

    #[test]
    fn both_limits_are_passed_through() {
        let options = options(
            VideoCodecPreference::H264,
            VideoEncodingLimits {
                max_bitrate_bps: Some(2_500_000),
                max_framerate: Some(30),
            },
        );
        assert_eq!(options.video_codec, VideoCodec::H264);
        let encoding = options.video_encoding.expect("an encoding");
        assert_eq!(encoding.max_bitrate, 2_500_000);
        assert_eq!(encoding.max_framerate, 30.0);
    }

    #[test]
    fn a_missing_limit_is_computed() {
        let computed = compute_appropriate_encoding(false, 1280, 720, VideoCodec::VP8);

        let bitrate_only = options(
            VideoCodecPreference::Auto,
            VideoEncodingLimits {
                max_bitrate_bps: Some(1_000_000),
                max_framerate: None,
            },
        )
        .video_encoding
        .expect("an encoding");
        assert_eq!(bitrate_only.max_bitrate, 1_000_000);
        assert_eq!(bitrate_only.max_framerate, computed.max_framerate);

        let framerate_only = options(
            VideoCodecPreference::Auto,
            VideoEncodingLimits {
                max_bitrate_bps: None,
                max_framerate: Some(15),
            },
        )
        .video_encoding
        .expect("an encoding");
        assert_eq!(framerate_only.max_bitrate, computed.max_bitrate);
        assert_eq!(framerate_only.max_framerate, 15.0);
    }

    #[test]
    fn capability_mime_types_map_to_codecs() {
        let codecs = codecs_from_mime_types([
            "video/VP8",
            "video/rtx",
            "video/H264",
            "video/H264",
            "video/VP9",
            "video/AV1",
            "video/H265",
            "video/red",
            "video/ulpfec",
        ]);
        assert_eq!(
            codecs,
            [
                VideoCodecPreference::Vp8,
                VideoCodecPreference::H264,
                VideoCodecPreference::Vp9,
                VideoCodecPreference::Av1,
            ]
        );
    }
}
