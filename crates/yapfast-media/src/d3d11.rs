//! Encoding on the GPU with no pixel copy: native/d3d11enc.c opens NVENC on a
//! D3D11 device and reads the caller's texture there, so a captured frame never
//! becomes an I420 picture in memory on its way to the encoder.
//!
//! native/hwenc.c is the other way to a GPU's encoder, and the one a stream
//! uses today: it takes I420, which costs the processor a conversion and an
//! upload of every frame. This is the same encoders reading the frame where it
//! already is, and it needs two things of its caller that hwenc.c doesn't: the
//! texture has to live on this encoder's device ([`D3d11Encoder::device`]), and
//! it has to be a concrete format — BGRA, RGBA or NV12, and the half-float scRGB
//! an HDR screen hands over where the session was opened for one
//! ([`Light`]) — since the driver reads the surface's own format and refuses a
//! typeless one. That last one is never registered: the card's video processor
//! tone-maps it into BGRA in the blit that already scales the picture, so the
//! encoder reads what it always read.
//!
//! The one copy is the bitstream: the driver's mapped output is moved into the
//! caller's buffer once per frame, so a packet can outlive the encoder's lock.
//! Nothing else of a frame is touched by the processor, and
//! [`D3d11Encoder::copied`] answers zero while that holds. The API mirrors
//! [`Encoder`](crate::encode::Encoder)'s where it can — an engine and its
//! settings, a bitrate that moves mid-stream, a frame with its time — so a
//! caller that has textures can hold this instead of that, and the stream's
//! bytes and timing are the same either way.
//!
//! A capture is rarely the size of a stream — a display wider than 1080p against
//! a 1080p stream is the ordinary case — so the encoder also scales:
//! [`D3d11Encoder::scale`] puts a captured picture at the stream's size in a
//! texture the encoder keeps and reuses, and [`D3d11Encoder::readback`] reads a
//! tile of it back at whatever size a preview asks for through one staging
//! texture, so a preview costs that tile rather than the screen. Both stay on the
//! GPU and go through the driver's video processor where it takes the texture,
//! and through a pixel shader where it doesn't; [`D3d11Encoder::filter`] says
//! which ran.
//!
//! Wiring this into a stream needs a capture source that fills a texture on
//! [`D3d11Encoder::device`] instead of handing over I420, or an encoder opened on
//! the capture's device with [`D3d11Encoder::open_on`], which is the same one
//! device seen from the other end — [`crate::wgc::Capture::open_on`] and
//! [`crate::dup::Capture::open_on`] are that direction. Either way there is one
//! D3D11 device for a capture and its encoder rather than two, and a texture made
//! by one is read by the other because it is the same object. What the capture
//! path will need from here: its textures made with `D3D11_BIND_SHADER_RESOURCE`,
//! or as render targets, which is what a video processor input view and a shader
//! resource view each take; a `scale` to the stream's size before each
//! `encode_texture` where the capture's size differs; and a `readback` at tile
//! size where a preview used to open a second capture of the same screen.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulonglong, c_void},
    sync::Arc,
};

use crate::codec::Codec;
use crate::pool::Pool;
use anyhow::{Result, anyhow, bail};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};

use crate::{
    encode::{Configuration, Frame},
    frames::SharedTexture,
    gpu,
    lease::{Lease, Loan, Loans},
};

// SAFETY: the shim's own declarations; every pointer passed is one it returned,
// and the buffers are as large as their sizes say.
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        luid_low: c_uint,
        luid_high: c_int,
        tuning: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_new_on(
        device: *mut c_void,
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        surfaces: c_uint,
        hdr: c_int,
        tuning: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_encode(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    /// Copies the access unit the last encode produced into the buffer given,
    /// which must hold it, and lets the encoder give up its copy.
    fn yapfast_d3d11enc_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_d3d11enc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_d3d11enc_scale(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        width: c_uint,
        height: c_uint,
        upscale: c_int,
    ) -> *mut c_void;
    fn yapfast_d3d11enc_readback(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        width: c_uint,
        height: c_uint,
        pixels: *mut c_void,
        size: usize,
    ) -> c_long;
    fn yapfast_d3d11enc_filter(encoder: *mut Raw, filter: *mut c_char, filter_size: usize);
    fn yapfast_d3d11enc_scaled_textures(encoder: *mut Raw) -> c_uint;
    fn yapfast_d3d11enc_staging_textures(encoder: *mut Raw) -> c_uint;
    fn yapfast_d3d11enc_asked(encoder: *mut Raw, asked: *mut c_char, asked_size: usize);
    fn yapfast_d3d11enc_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_d3d11enc_tuning(encoder: *mut Raw, tuning: *mut c_char, tuning_size: usize);
    fn yapfast_d3d11enc_codecs(encoder: *mut Raw, codecs: *mut c_char, codecs_size: usize);
    fn yapfast_d3d11enc_error(encoder: *mut Raw, error: *mut c_char, error_size: usize);
    fn yapfast_d3d11enc_device(encoder: *mut Raw) -> *mut c_void;
    fn yapfast_d3d11enc_copied(encoder: *mut Raw) -> c_ulonglong;
    fn yapfast_d3d11enc_free(encoder: *mut Raw);
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_shared_new(
        encoder: *mut Raw,
        width: c_uint,
        height: c_uint,
        process: c_uint,
        fence: *mut c_ulonglong,
        luid: *mut c_ulonglong,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut RawShared;
    fn yapfast_d3d11enc_shared_draw(
        encoder: *mut Raw,
        shared: *mut RawShared,
        at: c_uint,
        texture: *mut c_void,
        subresource: c_uint,
        handle: *mut c_ulonglong,
        value: *mut c_ulonglong,
    ) -> c_int;
    fn yapfast_d3d11enc_shared_picture(shared: *mut RawShared) -> *mut c_void;
    /// The number of the set of pictures and fence, which only grows: what the
    /// UI tells one set of the worker's textures from the next by.
    fn yapfast_d3d11enc_shared_set(shared: *mut RawShared) -> c_ulonglong;
    fn yapfast_d3d11enc_shared_free(shared: *mut RawShared);
    fn yapfast_d3d11enc_source_new(
        encoder: *mut Raw,
        frames: c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut RawSource;
    fn yapfast_d3d11enc_source_draw(source: *mut RawSource, frame: c_uint);
    fn yapfast_d3d11enc_source_texture(source: *mut RawSource) -> *mut c_void;
    fn yapfast_d3d11enc_source_quads(
        source: *mut RawSource,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_d3d11enc_source_free(source: *mut RawSource);
}

#[cfg(d3d11conv)]
unsafe extern "C" {
    fn yapfast_d3d11conv_new(
        device: *mut std::ffi::c_void,
        from_width: c_uint,
        from_height: c_uint,
        to_width: c_uint,
        to_height: c_uint,
        hdr: c_int,
        why: *mut c_char,
        size: usize,
    ) -> *mut Raw;
    fn yapfast_d3d11conv_run(
        converter: *mut Raw,
        texture: *mut std::ffi::c_void,
        subresource: c_uint,
        why: *mut c_char,
        size: usize,
    ) -> *mut std::ffi::c_void;
    fn yapfast_d3d11conv_converted(converter: *mut Raw) -> c_ulonglong;
    fn yapfast_d3d11conv_free(converter: *mut Raw);
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the shim measures for a test that asks it, and nothing a stream
    // runs: the devices this process has opened, so a second frame can be held
    // to the first's session, and the processor time an encode costs, which a
    // wall clock would charge the driver's wait on the GPU to.
    unsafe extern "C" {
        fn yapfast_d3d11enc_devices_created() -> c_uint;
        fn yapfast_d3d11enc_cpu_time() -> c_ulonglong;
    }
    /// One GPU test at a time: the encoder counts the devices this process has
    /// opened, so a test held to its own count has to be the only one opening
    /// devices while it counts. Taken for a test's whole body, and taken again
    /// after a panic in another test rather than poisoning every test after it.
    static ONE_GPU_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn one_gpu_test_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        ONE_GPU_TEST
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // The settings native/d3d11enc.c opens NVENC with for the tuning named, and
    // whether the stream can't do without each: the list that open reads, so a
    // test holds it to what native/hwenc.c's own list says rather than to a copy
    // of either.
    #[cfg(hwenc)]
    unsafe extern "C" {
        fn yapfast_d3d11enc_tuning_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
    }

    /// The two GPU backends ask NVENC for the same thing, for every tuning a
    /// stream may ask for: low-latency tuning, capped VBR, no frame allowed to
    /// wait and an IDR on demand, with the same settings marked as the ones the
    /// stream can't do without. A tuning that changed on one side and not the
    /// other would encode a picture nobody measured, and neither a stream nor a
    /// comparison would say which it got.
    #[cfg(hwenc)]
    #[test]
    fn both_gpu_backends_ask_nvenc_for_the_same_thing() {
        let mut d3d11 = [0u8; 192];
        let mut ffmpeg = [0u8; 192];
        // SAFETY: none for the shipped tuning, and a buffer of the size given on
        // each side.
        let (ours, theirs) = unsafe {
            (
                yapfast_d3d11enc_tuning_settings(
                    std::ptr::null(),
                    d3d11.as_mut_ptr().cast(),
                    d3d11.len(),
                ),
                yapfast_hwenc_nvenc_settings(
                    std::ptr::null(),
                    ffmpeg.as_mut_ptr().cast(),
                    ffmpeg.len(),
                ),
            )
        };
        assert_eq!((ours, theirs), (0, 0));
        assert_eq!(
            c_text(&d3d11),
            c_text(&ffmpeg),
            "the two backends' settings"
        );
        // A tuning no row has is refused rather than quietly replaced by the
        // shipped one, on both sides of the comparison: the two tables being
        // one name apart is a share that silently changes how its frames reach
        // the encoder.
        let mut none = [0u8; 192];
        let mut nothing = [0u8; 192];
        // SAFETY: as above, with a name no row has.
        let refused = unsafe {
            (
                yapfast_d3d11enc_tuning_settings(
                    c"p7".as_ptr(),
                    none.as_mut_ptr().cast(),
                    none.len(),
                ),
                yapfast_hwenc_nvenc_settings(
                    c"p7".as_ptr(),
                    nothing.as_mut_ptr().cast(),
                    nothing.len(),
                ),
            )
        };
        assert_eq!(refused, (-1, -1));
    }

    /// The setting chooses between the adapters this machine really presents,
    /// and the adapter it can't encode on says so rather than letting a stream
    /// find out from a driver's status number.
    ///
    /// `Integrated` is the case that matters: on a machine whose processor has
    /// graphics the device would be made on Intel's, where NVENC isn't, so the
    /// open has to fail naming that adapter. A machine without one falls back to
    /// its card, as it always did. What was enumerated is printed either way, so
    /// a run says which machine it checked.
    #[test]
    fn the_setting_picks_an_adapter_and_nvenc_only_exists_on_a_card() {
        let machine = adapters().expect("DXGI enumerates this machine's adapters");
        eprintln!("adapters: {}", listed(&machine));
        // What `Integrated` would be made a device on, read with the same policy
        // the choice uses rather than by repeating Intel's vendor id here.
        let integrated = machine.iter().find(|named| {
            gpu::Preference::Integrated.rank(&named.facts) == Some(gpu::Rank::Preferred)
        });
        match chosen(gpu::Preference::Dedicated) {
            Ok(picked) => assert_eq!(picked.facts.vendor, NVIDIA, "{} isn't a card", picked.name),
            Err(error) => {
                // No NVIDIA card here, or none that ranked above the rest: that
                // is a machine this backend has never encoded on.
                eprintln!("dedicated: {error}");
            }
        }
        match chosen(gpu::Preference::Integrated) {
            Ok(picked) => assert!(
                integrated.is_none(),
                "Integrated chose {} and this machine's integrated GPU can't encode",
                picked.name
            ),
            Err(error) => {
                let said = error.to_string();
                eprintln!("integrated: {said}");
                if let Some(named) = integrated {
                    assert!(said.contains("no NVENC"), "{said}");
                    assert!(
                        said.contains(&named.name),
                        "the failure doesn't name {}: {said}",
                        named.name
                    );
                }
            }
        }
    }

    /// Thirty frames of each codec NVENC has here, read from a texture the GPU
    /// fills: every frame is an access unit, the first is a keyframe and the rest
    /// follow it, and the session encodes all thirty — one device, one session, no
    /// pixel of any frame touched by the processor. The first frames are read as
    /// the codec's own structure, since that is what the packetizer and every
    /// viewer's client depend on and the one thing a wrong driver call would break
    /// without looking broken.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn thirty_frames_of_each_codec_encode_from_a_texture() {
        let _gpu = one_gpu_test_at_a_time();
        // 1080p at 60, the shape a stream of a game is, so the numbers this prints
        // are the ones a stream would see.
        let (width, height, fps, kbps) = (1920u32, 1080u32, 60u32, 8000u32);
        let before = unsafe { yapfast_d3d11enc_devices_created() };
        let mut opened = 0;
        for codec in [Codec::H264, Codec::H265, Codec::Av1] {
            let mut encoder = match D3d11Encoder::new(codec, width, height, fps, kbps) {
                Ok(encoder) => encoder,
                Err(error) => {
                    // A card without the codec is not a failure of this backend:
                    // AV1 needs an RTX 40 series or newer, and the open says which
                    // codecs the driver has.
                    eprintln!("{codec:?} didn't open here: {error:#}");
                    continue;
                }
            };
            opened += 1;
            eprintln!(
                "{codec:?}: {} ({}), {} fps, driver has {}",
                encoder.engine(),
                encoder.configuration().tuning,
                fps,
                encoder.codecs()
            );
            let device = encoder.device();
            assert!(!device.is_null());
            let source = encoder.source(60).unwrap();
            assert!(!source.texture().handle.is_null());

            // The frames' processor time alone: the driver waits on the GPU inside
            // this call, and a wall clock would charge that to the processor.
            let mut frames = vec![];
            for frame in 0..30u32 {
                source.draw(frame);
                // Half a millisecond a frame at 60 fps, in 90 kHz units: one
                // stream's clock, the same one every other engine runs on.
                let pts = u64::from(frame) * 1500;
                if frame == 15 {
                    // A bitrate that moves mid-stream, without a keyframe: the
                    // network changed and the viewers keep their references.
                    encoder.set_bitrate(kbps / 2).unwrap();
                    assert_eq!(encoder.bitrate(), kbps / 2);
                }
                let encoded = encoder
                    .encode_texture(source.texture(), pts, frame == 0)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is an empty access unit"
                );
                assert_eq!(encoded.timestamp, pts as u32);
                if frame == 0 {
                    assert!(
                        encoded.keyframe,
                        "the first frame of {codec:?} isn't a keyframe"
                    );
                    // Its structure, as the packetizer reads it: parameter sets
                    // before the picture, in the form this codec's packets take.
                    match codec {
                        Codec::H264 => assert_eq!(
                            crate::bitstream::nal_units(&encoded.data)[0][0] & 0x1f,
                            7,
                            "an H.264 keyframe starts with its sequence parameter set"
                        ),
                        Codec::H265 => assert_eq!(
                            (crate::bitstream::nal_units(&encoded.data)[0][0] >> 1) & 0x3f,
                            32,
                            "an H.265 keyframe starts with its video parameter set"
                        ),
                        _ => assert_eq!(
                            (crate::bitstream::obu_elements(&encoded.data)
                                .expect("OBUs within the frame")[0][0]
                                >> 3)
                                & 0x0f,
                            1,
                            "an AV1 keyframe starts with its sequence header"
                        ),
                    }
                }
                frames.push(encoded);
            }
            let bytes: usize = frames.iter().map(|frame| frame.data.len()).sum();
            // Only the frame it was asked for is a keyframe: the interval is three
            // seconds, and these are half a second.
            assert_eq!(
                frames.iter().filter(|frame| frame.keyframe).count(),
                1,
                "a keyframe the stream wasn't asked for"
            );
            // What a frame costs the processor, over a run long enough to read:
            // the process's own counter moves in ticks of about 15 ms, so thirty
            // frames that take a quarter of one can only be timed by the score.
            // Three hundred of them are two and a half seconds of a stream, and
            // the same run the reference measurement used, so the number below is
            // comparable with it.
            const TIMED: u32 = 300;
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() };
            for frame in 30..30 + TIMED {
                source.draw(frame);
                let encoded = encoder
                    .encode_texture(source.texture(), u64::from(frame) * 1500, false)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is empty"
                );
            }
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() }.saturating_sub(cpu);
            eprintln!(
                "  {bytes} bytes in the first {}, {:.4} ms of the processor per frame",
                frames.len(),
                cpu as f64 / f64::from(TIMED) / 10_000.0
            );
            // The pixels went to the encoder on the GPU: a converted frame would
            // say so here without any frame looking wrong.
            assert_eq!(encoder.copied(), 0, "the processor moved a frame's pixels");
            // And the thirty frames took the session that was opened, not another
            // device or another encoder.
            assert_eq!(encoder.device(), device);
            assert_eq!(
                unsafe { yapfast_d3d11enc_devices_created() },
                before + opened,
                "an encode opened another D3D11 device"
            );
        }
        assert_eq!(opened, 3, "not every codec opened on this GPU");
    }
    /// The four quadrant colours as memory holds them: a BGRA texture's first
    /// byte is blue, and 0.2 and 0.8 of full scale are 51 and 204.
    const BLUE: [u8; 4] = [204, 51, 51, 255];
    const RED: [u8; 4] = [51, 51, 204, 255];
    const GREEN: [u8; 4] = [51, 204, 51, 255];
    const WHITE: [u8; 4] = [204, 204, 204, 255];
    /// Where each quadrant's own colour is read: each quarter's middle, which is
    /// nowhere near a seam, and the colour the picture has there.
    fn quarters(width: u32, height: u32) -> [(&'static str, u32, u32, [u8; 4]); 4] {
        [
            ("top left", width / 4, height / 4, BLUE),
            ("top right", width * 3 / 4, height / 4, RED),
            ("bottom left", width / 4, height * 3 / 4, GREEN),
            ("bottom right", width * 3 / 4, height * 3 / 4, WHITE),
        ]
    }
    /// The BGRA pixel at (`x`, `y`) of a picture `width` pixels wide.
    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        pixels[at..at + 4].try_into().expect("four bytes a pixel")
    }
    /// Holds a scaled pixel to the colour its quarter had in the source: a solid
    /// colour through four taps and a bilinear filter loses a rounding or two,
    /// and a driver converting it on its way through loses a couple more, so four
    /// of 255 is the tolerance a colour is held to here.
    fn assert_solid(got: [u8; 4], colour: [u8; 4], quarter: &str) {
        for (channel, (got, had)) in ["blue", "green", "red", "alpha"]
            .into_iter()
            .zip(got.iter().zip(colour.iter()))
        {
            assert!(
                got.abs_diff(*had) <= 4,
                "{quarter}'s {channel} is {got} where the picture had {had}"
            );
        }
    }
    /// A known picture scaled down keeps its structure: four solid quadrants of a
    /// 1080p capture are four quadrants of a 480x270 picture, each in the corner
    /// it was in and each still one colour. That is the whole of what a display
    /// wider than 1080p needs to reach a 1080p stream, and a filter that mixed the
    /// quarters or lost one would look like a smeared screen rather than a wrong
    /// number.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_scaled_picture_keeps_its_four_quadrants() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (480u32, 270u32);
        let encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        // Four times smaller on each side, as a 3840-wide screen is on its way to
        // a 1080p stream.
        let scaled = encoder
            .scale(source.texture(), width, height, Resize::Shrink)
            .expect("a picture scaled down four times");
        eprintln!("scaled by {}", encoder.filter());
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(scaled, width, height, &mut pixels)
                .expect("the scaled picture read back"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_solid(pixel(&pixels, width, x, y), colour, quarter);
        }
    }
    /// A picture drawn for the UI is a whole tile of the capture, drawn again and
    /// again without it being written over: the two draws answer different
    /// textures, so the one the UI would be showing is never the one the next
    /// frame fills, and climbing fence values, so a wait can tell one from the
    /// other. What the second one holds is the capture's own quarters, which is
    /// the picture the UI would put on screen.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_shared_picture_is_drawn_in_two_before_either_is_drawn_over() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (480u32, 270u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        // This process, which DuplicateHandle answers as readily as another one:
        // what the UI does with the handles it is given is the UI's test, and a
        // handle here is a handle whether or not anybody else opens it.
        let mut shared = Shared::new(&encoder, width, height, std::process::id())
            .expect("a tile-sized picture to share");
        assert_eq!(shared.size(), (width, height));
        assert_ne!(
            shared.luid(),
            0,
            "the picture has no adapter to be shown on"
        );
        assert_ne!(shared.fence(), 0, "the picture has no fence to wait on");
        let (first, first_value) = shared
            .draw(0, &mut encoder, source.texture())
            .expect("the first picture drawn");
        let (second, second_value) = shared
            .draw(1, &mut encoder, source.texture())
            .expect("the second picture drawn");
        assert_ne!(
            first.handle, second.handle,
            "both pictures are one texture, which is one written under the UI"
        );
        assert_eq!(
            (first.set, second.set),
            (shared.set(), shared.set()),
            "one set of pictures, and the pictures say which"
        );
        assert!(
            second_value > first_value,
            "the fence went back, to {second_value} from {first_value}"
        );
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(shared.picture(), width, height, &mut pixels)
                .expect("the shared picture read back"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_solid(pixel(&pixels, width, x, y), colour, quarter);
        }
        eprintln!(
            "two {width}x{height} pictures at {} and {} behind a fence at {second_value}",
            first.handle, second.handle
        );
    }

    /// A set is three textures and one fence with a number of its own, and a set
    /// built after another is a larger number: the UI lets go of what it opened
    /// for the older one when a picture names the newer, and only a number can
    /// say that, since Windows hands a closed handle's value out again.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_shared_set_has_a_number_that_only_grows() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (64u32, 36u32);
        let encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        // This process, which DuplicateHandle answers as readily as another one.
        let first =
            Shared::new(&encoder, width, height, std::process::id()).expect("a picture to share");
        assert_ne!(first.set(), 0, "a set has no number");
        let second = Shared::new(&encoder, width, height, std::process::id())
            .expect("another picture to share");
        assert!(
            second.set() > first.set(),
            "the second set is numbered {} after {}",
            second.set(),
            first.set()
        );
        eprintln!("sets {} and {}", first.set(), second.set());
    }
    /// A readback at a picture's own size is that picture's pixels, exactly: the
    /// copy is a copy, with no scaler in it to move a value, so the two pixels
    /// either side of a quadrant's seam keep the colours the picture put there.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_readback_returns_the_textures_own_pixels() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (1920u32, 1080u32);
        let encoder = D3d11Encoder::new(Codec::H264, width, height, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(source.texture(), width, height, &mut pixels)
                .expect("the picture read back at its own size"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_eq!(pixel(&pixels, width, x, y), colour, "{quarter}");
        }
        // The seam, on the row above the halves' own: the last column of the left
        // half is the left half's colour to the byte and the first of the right
        // half is the right half's, where a scale would have blended them.
        let middle = (width / 2, height / 2 - 1);
        assert_eq!(pixel(&pixels, width, middle.0 - 1, middle.1), BLUE);
        assert_eq!(pixel(&pixels, width, middle.0, middle.1), RED);
        eprintln!("{} bytes read back at the picture's own size", pixels.len());
    }
    /// A readback at a smaller size is genuinely smaller: the bytes it answers
    /// with, and the buffer they fit in, are the tile's and not the frame's. A
    /// preview that cost a whole frame's read per frame is what this replaces, and
    /// the tile still holds a picture of the whole screen rather than the corner
    /// of one.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_readback_at_a_smaller_size_is_genuinely_smaller() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (1920u32, 1080u32);
        let (tile_width, tile_height) = (480u32, 270u32);
        let encoder = D3d11Encoder::new(Codec::H264, width, height, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let mut tile = vec![0u8; (tile_width * tile_height * 4) as usize];
        let read = encoder
            .readback(source.texture(), tile_width, tile_height, &mut tile)
            .expect("a tile of a 1080p picture");
        let frame = (width * height * 4) as usize;
        assert_eq!(read, tile.len(), "the bytes written are the buffer's");
        assert_eq!(read, frame / 16, "a sixteenth of the frame, by area");
        assert!(read < frame, "a tile is smaller than the frame it is of");
        for (quarter, x, y, colour) in quarters(tile_width, tile_height) {
            assert_solid(pixel(&tile, tile_width, x, y), colour, quarter);
        }
        eprintln!(
            "{read} bytes of a {frame}-byte frame, scaled by {}",
            encoder.filter()
        );
    }
    /// A hundred readbacks reuse the one scaled picture and the one staging
    /// texture: nothing is allocated per call and the hundredth tile is the first
    /// one's picture. Memory is a product goal here, so this is a count the
    /// encoder reports rather than a hope about the code — and a preview reads a
    /// tile thirty times a second for as long as somebody is watching.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_hundred_readbacks_reuse_their_textures() {
        let _gpu = one_gpu_test_at_a_time();
        let (tile_width, tile_height) = (480u32, 270u32);
        let encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let mut tile = vec![0u8; (tile_width * tile_height * 4) as usize];
        encoder
            .readback(source.texture(), tile_width, tile_height, &mut tile)
            .expect("the first tile");
        let first = tile.clone();
        assert_eq!(
            (encoder.scaled_textures(), encoder.staging_textures()),
            (1, 1),
            "the first readback made more than one texture"
        );
        for read in 1..100 {
            let written = encoder
                .readback(source.texture(), tile_width, tile_height, &mut tile)
                .unwrap_or_else(|why| panic!("tile {read}: {why:#}"));
            assert_eq!(written, tile.len());
            assert_eq!(tile, first, "tile {read} isn't the picture the first was");
        }
        assert_eq!(
            (encoder.scaled_textures(), encoder.staging_textures()),
            (1, 1),
            "a hundred readbacks made another texture"
        );
        eprintln!(
            "{} bytes a tile, a hundred of them, through {} staging texture(s) and {} scaled",
            tile.len(),
            encoder.staging_textures(),
            encoder.scaled_textures()
        );
    }

    /// A captured texture is the encoder's texture: the capture makes the device,
    /// the encoder opens on that same one instead of making its own, and the very
    /// texture the compositor filled encodes to a bitstream whose first frame is a
    /// keyframe. That is the whole texture path end to end — screen, texture,
    /// driver's encoder, packet — and the device count is the half of it no picture
    /// can show: one device for a capture and its encoder, not one each.
    ///
    /// Nothing here wires the pair into a stream; it is the same two calls a
    /// stream will make, in the same order, so that the API is proven rather than
    /// assumed.
    #[test]
    #[ignore = "needs a screen and an NVIDIA GPU: it captures the primary display and encodes it"]
    fn a_captured_frame_encodes_on_the_captures_own_device() {
        use std::time::{Duration, Instant};

        use crate::wgc::{Arrival, Capture, Monitor};

        let _gpu = one_gpu_test_at_a_time();
        let mut capture =
            Capture::open(Monitor::PRIMARY).expect("a capture of the primary display");
        eprintln!("{}", capture.describe());
        let device = capture.device();
        assert!(!device.is_null(), "a capture with no device");

        // One frame, waited for rather than demanded: the compositor makes one
        // when it has something to draw, which on the first call is a few tens of
        // milliseconds.
        let started = Instant::now();
        let (captured_width, captured_height) = loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no frame in 10 s, which is what a capture of a screen that isn't drawing does"
            );
            match capture.frame().expect("a frame") {
                Arrival::Frame => break capture.content_size(),
                Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                // The pool is rebuilt at the new size by the shim, so the next
                // frame is the one to encode.
                Arrival::Resized => continue,
                Arrival::Closed => panic!("the primary display's capture closed"),
            }
        };
        let texture = capture.texture();
        assert!(!texture.is_null(), "a frame arrived with no texture");

        // The captured texture itself is what is encoded, so that the frame the
        // encoder reads is the frame the compositor filled. NVENC takes even
        // dimensions; H.264 stops at 4096 a side where H.265 and AV1 go to 8192,
        // and a display past that goes through the scaler, which is what a stream
        // does with any display wider than the stream's own size.
        let (captured_even_width, captured_even_height) =
            (captured_width & !1, captured_height & !1);
        assert!(
            captured_even_width > 0 && captured_even_height > 0,
            "a {captured_width}x{captured_height} display"
        );
        let within = |side: u32| captured_even_width <= side && captured_even_height <= side;
        let (codec, width, height) = if within(4096) {
            (Codec::H264, captured_even_width, captured_even_height)
        } else if within(8192) {
            (Codec::H265, captured_even_width, captured_even_height)
        } else {
            (Codec::H264, 1920u32, 1080u32)
        };

        // The count is this process's, so it is read either side of the open that
        // must not move it: what an encoder on a device handed to it must not do is
        // make a second device.
        let devices_before = unsafe { yapfast_d3d11enc_devices_created() };
        let encoder = D3d11Encoder::open_on(
            device,
            codec,
            width,
            height,
            60,
            8000,
            Surfaces::Ring,
            Light::Sdr,
        )
        .unwrap_or_else(|why| panic!("no encoder on the capture's device: {why:#}"));
        assert_eq!(
            unsafe { yapfast_d3d11enc_devices_created() },
            devices_before,
            "the encoder opened on a device and made a second one anyway"
        );
        assert_eq!(
            encoder.device(),
            device,
            "the encoder is on a device of its own rather than the capture's"
        );
        let scaled = (width, height) != (captured_width, captured_height);
        let texture = if scaled {
            encoder
                .scale(
                    // SAFETY: the capture's own texture, alive until it goes out of
                    // scope at the end of this test, and on the device the encoder
                    // was opened on.
                    unsafe { Texture::new(texture) },
                    width,
                    height,
                    Resize::Shrink,
                )
                .expect("the capture's picture scaled to the encoder's size")
        } else {
            // SAFETY: as above: the capture's own texture, alive for the rest of
            // the test.
            unsafe { Texture::new(texture) }
        };

        eprintln!(
            "{} ({}), {width}x{height} from a {captured_width}x{captured_height} capture ({}), \
             device {device:p}",
            encoder.engine(),
            encoder.configuration().tuning,
            if scaled {
                "through the scaler"
            } else {
                "the captured texture itself"
            }
        );
        let packet = encoder
            .encode_texture(texture, 0, true)
            .expect("the captured frame encoded")
            .expect("a packet for the captured frame");
        assert!(!packet.data.is_empty(), "an empty access unit");
        assert!(packet.keyframe, "the first frame isn't a keyframe");
        // Its structure, as the packetizer reads it: parameter sets before the
        // picture, in the form this codec's packets take — the same check the
        // drawn-texture test makes, now made about a screen.
        match codec {
            Codec::H265 => assert_eq!(
                (crate::bitstream::nal_units(&packet.data)[0][0] >> 1) & 0x3f,
                32,
                "an H.265 keyframe starts with its video parameter set"
            ),
            _ => assert_eq!(
                crate::bitstream::nal_units(&packet.data)[0][0] & 0x1f,
                7,
                "an H.264 keyframe starts with its sequence parameter set"
            ),
        }
        // No pixel of it touched by the processor: the same claim the drawn-texture
        // tests make, now made about a screen.
        assert_eq!(
            encoder.copied(),
            0,
            "the capture's pixels crossed the processor"
        );
        eprintln!(
            "{} bytes for the captured frame, a keyframe, {} frames' pixels copied by the processor",
            packet.data.len(),
            encoder.copied()
        );

        // The capture outlives the encoder here, which is the ownership rule read
        // the other way: the encoder borrowed the device and released nothing, so
        // the capture still has one to close.
        drop(encoder);
        assert_eq!(
            capture.device(),
            device,
            "the encoder took the device with it"
        );
        drop(capture);
    }
}

mod device;
mod session;
mod shared;

pub(crate) use device::*;

#[cfg(d3d11conv)]
pub use session::Converter;
pub use session::D3d11Encoder;
pub use session::Light;
pub use session::Resize;
pub use session::Surfaces;
pub use session::Texture;
pub use shared::Shared;
pub use shared::Source;
