use super::*;

#[test]
fn reads_report_blocks() {
    let report = receiver_report(1, 77, (1000, 12, 3), (0, 0));
    let both = [report.clone(), receiver_report(1, 78, (40, 0, 0), (0, 0))].concat();
    assert_eq!(report_blocks(&report), [(77, 12, 1000)]);
    assert_eq!(report_blocks(&both), [(77, 12, 1000), (78, 0, 40)]);
    assert!(report_blocks(&picture_loss(1, 77)).is_empty());
}

#[test]
fn reads_what_a_report_says_about_one_ssrc() {
    let report = receiver_report(1, 77, (1000, 12, 30), (0x1234_5678, 6554));
    let both = [receiver_report(1, 78, (40, 0, 0), (0, 0)), report].concat();
    assert_eq!(
        reception(&both, 77),
        Some(Reception {
            ssrc: 77,
            fraction_lost: 30,
            lost: 12,
            highest: 1000,
            jitter: 0,
            last_report: 0x1234_5678,
            delay: 6554,
        })
    );
    assert_eq!(reception(&both, 79), None);
}

#[test]
fn measures_the_round_trip_from_an_echoed_sender_report() {
    use std::time::{Duration, Instant};
    let sent = Instant::now();
    // Held 0.1 s (6554/65536) of the 0.35 s between.
    let received = sent + Duration::from_millis(350);
    let trip = round_trip(sent, received, 6554).unwrap();
    assert!(
        trip.abs_diff(Duration::from_millis(250)) < Duration::from_millis(1),
        "{trip:?}"
    );
    assert_eq!(
        round_trip(sent, received, 0),
        Some(Duration::from_millis(350))
    );
    assert_eq!(
        round_trip(sent, received, 65_536),
        None,
        "held longer than the trip"
    );
    assert_eq!(round_trip(received, sent, 0), None, "from the future");
}

#[test]
fn packetizes_frames_with_vp8_descriptors() {
    let mut packetizer = Packetizer::new(77, Codec::Vp8);
    let frame: Vec<u8> = (0..2500u32).map(|index| index as u8).collect();
    let packets = packetizer.packets(&frame, 9000, false);
    assert_eq!(packets.len(), 3);
    let mut joined = vec![];
    for (index, packet) in packets.iter().enumerate() {
        assert_eq!(packet[0], 0x90, "version 2, with extensions");
        assert_eq!(packet[1] & 0x7f, VP8);
        assert_eq!(packet[1] & 0x80 != 0, index == 2, "marker on the last");
        assert_eq!(&packet[4..8], &9000u32.to_be_bytes());
        assert_eq!(&packet[8..12], &77u32.to_be_bytes());
        assert_eq!(rtp_clear_len(packet), Some(16));
        // Transport sequence, screen content, then the stream id,
        // padded to whole words.
        let transport = (index + 1) as u8;
        assert_eq!(
            &packet[12..28],
            &[
                0xbe, 0xde, 0, 3, 0x51, 0, transport, 0x70, 1, 0xb2, b'1', b'0', b'0', 0, 0, 0
            ]
        );
        let descriptor = &packet[28..32];
        assert_eq!(descriptor[0], if index == 0 { 0x90 } else { 0x80 });
        joined.extend_from_slice(&packet[32..]);
    }
    assert_eq!(joined, frame);
    let sequence = |packet: &Vec<u8>| u16::from_be_bytes([packet[2], packet[3]]);
    assert_eq!(sequence(&packets[1]), sequence(&packets[0]).wrapping_add(1));
    let picture = |packet: &Vec<u8>| u16::from_be_bytes([packet[30], packet[31]]) & 0x7fff;
    let next = packetizer.packets(&[1], 12000, false);
    assert_eq!(picture(&next[0]), (picture(&packets[0]) + 1) & 0x7fff);
}

#[test]
fn seals_and_opens_both_modes() {
    for mode in Transport::MODES {
        let mut transport = Transport::new(mode, &[7; 32]).unwrap();
        let mut packet = Packetizer::new(1, Codec::Vp8)
            .packets(b"frame", 0, false)
            .remove(0);
        let plain = packet.clone();
        let clear = rtp_clear_len(&packet).unwrap();
        transport.seal(&mut packet, clear).unwrap();
        assert_eq!(packet.len(), plain.len() + 20);
        assert_eq!(packet[..clear], plain[..clear], "the header stays readable");
        assert_ne!(packet[clear..plain.len()], plain[clear..]);
        assert_eq!(transport.open(&packet, clear), Some(plain));
        packet[clear] ^= 1;
        assert_eq!(transport.open(&packet, clear), None, "tampering fails");
    }
    assert!(Transport::new("xsalsa20_poly1305", &[0; 32]).is_err());
}

fn packet(sequence: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![0x80, if marker { 0x80 } else { 0 } | H264];
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&9u32.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

#[test]
fn reassembles_what_the_packetizer_cut() {
    let mut packetizer = Packetizer::new(5, Codec::Vp8);
    let mut assembler = Assembler::new(Codec::Vp8);
    let keyframe: Vec<u8> = [0x10]
        .into_iter()
        .chain((0..3000).map(|index| index as u8))
        .collect();
    let delta = vec![0x11; 1500];
    let mut frames = vec![];
    for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
        for sent in packetizer.packets(frame, index as u32 * 3000, false) {
            let packet = parse(&sent).unwrap();
            assert_eq!((packet.ssrc, packet.payload_type), (5, VP8));
            frames.extend(assembler.push(&packet).frame);
        }
    }
    assert_eq!(frames.len(), 2);
    assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
    assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
}

#[test]
fn drops_frames_after_a_loss_until_a_keyframe() {
    let mut packetizer = Packetizer::new(5, Codec::Vp8);
    let mut assembler = Assembler::new(Codec::Vp8);
    let delta = vec![0x11; 2500];
    // A delta first: nothing to decode it against.
    let mut pushed: Vec<Pushed> = packetizer
        .packets(&delta, 0, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .collect();
    assert!(pushed.iter().all(|pushed| pushed.frame.is_none()));
    let keyframe = vec![0x10; 2500];
    let mut sent = packetizer.packets(&keyframe, 3000, false);
    sent.remove(1);
    pushed = sent
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .collect();
    assert!(
        pushed.last().unwrap().lost,
        "a missing packet asks for a keyframe"
    );
    let whole = packetizer.packets(&keyframe, 6000, false);
    let last = whole
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert_eq!(last.frame.map(|frame| frame.keyframe), Some(true));
}

#[test]
fn asks_for_a_keyframe_until_one_comes() {
    let mut packetizer = Packetizer::new(5, Codec::Vp8);
    let mut assembler = Assembler::new(Codec::Vp8);
    let delta = vec![0x11; 500];
    let pushed = packetizer
        .packets(&delta, 0, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert!(pushed.lost, "nothing to decode yet");
    // Padding between frames isn't a loss.
    let padding = Packet {
        payload_type: VP8,
        marker: false,
        sequence: packetizer.sequence,
        timestamp: 0,
        ssrc: 5,
        payload: &[],
        orientation: None,
    };
    packetizer.sequence = packetizer.sequence.wrapping_add(1);
    assert_eq!(assembler.push(&padding), Pushed::default());
    let keyframe = vec![0x10; 500];
    let pushed = packetizer
        .packets(&keyframe, 3000, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert_eq!(pushed.frame.map(|frame| frame.data), Some(keyframe));
    assembler.wait_for_keyframe();
    let pushed = packetizer
        .packets(&delta, 6000, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert!(
        pushed.frame.is_none() && pushed.lost,
        "the decoder missed a frame"
    );
}

#[test]
fn puts_packets_back_in_order_and_asks_for_lost_ones() {
    let start = std::time::Instant::now();
    let at = |ms: u64| start + std::time::Duration::from_millis(ms);
    let held = |sequence: u16| Held {
        payload_type: VP8,
        marker: false,
        sequence,
        timestamp: 0,
        ssrc: 5,
        payload: vec![sequence as u8],
        orientation: None,
    };
    let sequences =
        |ready: Vec<Held>| -> Vec<u16> { ready.iter().map(|held| held.sequence).collect() };
    let mut reorder = Reorder::default();
    // Across the wrap: 65534, 65535, then 1 before 0.
    for sequence in [65534, 65535, 1] {
        assert_eq!(reorder.insert(held(sequence), at(0)), Arrival::New);
    }
    assert_eq!(sequences(reorder.ready(at(0)).0), [65534, 65535]);
    assert_eq!(reorder.to_ask(at(0)), [0], "asked for at once");
    assert!(reorder.to_ask(at(10)).is_empty(), "not again so soon");
    assert_eq!(reorder.insert(held(0), at(20)), Arrival::Recovered);
    assert_eq!(sequences(reorder.ready(at(20)).0), [0, 1]);
    assert_eq!(reorder.insert(held(0), at(30)), Arrival::Late);

    // 2 and 3 never come: asked for three times, then given up on.
    reorder.insert(held(4), at(100));
    assert_eq!(reorder.to_ask(at(100)), [2, 3]);
    assert_eq!(reorder.to_ask(at(180)), [2, 3]);
    assert_eq!(reorder.to_ask(at(260)), [2, 3]);
    assert!(reorder.to_ask(at(340)).is_empty());
    assert_eq!(reorder.ready(at(399)), (vec![], 0));
    let (ready, skipped) = reorder.ready(at(400));
    assert_eq!((sequences(ready), skipped), (vec![4], 2));
    assert_eq!(reorder.insert(held(3), at(410)), Arrival::Late);
    assert_eq!(reorder.counts(), Some((65536 + 4, 2)), "one wrap, two lost");
}

#[test]
fn bounds_what_a_sender_skipping_ahead_runs_up() {
    let at = |ms: u64| std::time::Instant::now() + std::time::Duration::from_millis(ms);
    let held = |sequence: u16| Held {
        payload_type: VP8,
        marker: false,
        sequence,
        timestamp: 0,
        ssrc: 5,
        payload: vec![0],
        orientation: None,
    };
    let mut reorder = Reorder::default();
    let mut sequence = 0u16;
    for _ in 0..2000 {
        reorder.insert(held(sequence), at(0));
        sequence = sequence.wrapping_add(HELD as u16 - 1);
    }
    assert!(reorder.missing.len() <= MAX_MISSING);
    assert!(reorder.to_ask(at(0)).len() <= MAX_ASKED);
}

#[test]
fn drops_a_frame_that_never_ends() {
    let mut packetizer = Packetizer::new(5, Codec::Vp8);
    let mut assembler = Assembler::new(Codec::Vp8);
    let huge = vec![0x10; MAX_FRAME + 4096];
    let last = packetizer
        .packets(&huge, 0, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert_eq!(last.frame, None);
    assert!(assembler.frame.len() <= MAX_FRAME);
    let keyframe = vec![0x10; 2500];
    let last = packetizer
        .packets(&keyframe, 3000, false)
        .iter()
        .map(|sent| assembler.push(&parse(sent).unwrap()))
        .last()
        .unwrap();
    assert_eq!(last.frame.map(|frame| frame.data), Some(keyframe));
}

/// An AV1 element that carries on in every packet, never reaching the
/// frame, is held to the same limit.
#[test]
fn drops_an_av1_element_that_never_ends() {
    let mut assembler = Assembler::new(Codec::Av1);
    let mut payload = vec![0x50, 0x32];
    payload.resize(CHUNK, 7);
    for sequence in 0..(2 * MAX_FRAME / CHUNK) as u16 {
        assembler.push(&Packet {
            payload_type: AV1,
            marker: false,
            sequence,
            timestamp: 3000,
            ssrc: 5,
            payload: &payload,
            orientation: None,
        });
        // W=1, Z and Y: the one element continues the last packet's
        // and carries on in the next.
        payload[..2].copy_from_slice(&[0xd0, 7]);
    }
    assert!(assembler.obu.len() <= MAX_FRAME);
}

/// A packet with header extensions `profile` and `elements`, padded to
/// whole words, around a one-byte payload.
fn extended(profile: u16, elements: &[u8]) -> Vec<u8> {
    let mut packet = vec![0x90, VP8, 0, 1, 0, 0, 0, 9, 0, 0, 0, 5];
    let words = elements.len().div_ceil(4);
    packet.extend_from_slice(&profile.to_be_bytes());
    packet.extend_from_slice(&(words as u16).to_be_bytes());
    packet.extend_from_slice(elements);
    packet.resize(HEADER + 4 + words * 4, 0);
    packet.push(0x42);
    packet
}

#[test]
fn reads_the_video_orientation_at_discords_id() {
    // Among the transport sequence and the stream id, as Discord sends.
    for (byte, turns) in [(0, 0), (1, 1), (2, 2), (3, 3)] {
        let packet = extended(0xbede, &[0x51, 0, 7, 0x40, byte, 0xb2, b'1', b'0', b'0']);
        let parsed = parse(&packet).unwrap();
        assert_eq!(parsed.payload, &[0x42]);
        assert_eq!(
            parsed.orientation,
            Some(Orientation { turns, flip: false }),
            "{} degrees",
            u32::from(turns) * 90
        );
    }
    let turned =
        |profile, elements: &[u8]| parse(&extended(profile, elements)).unwrap().orientation;
    // The flip, with the front camera's bit, which changes nothing.
    assert_eq!(
        turned(0xbede, &[0x40, 0x0d]),
        Some(Orientation {
            turns: 1,
            flip: true
        })
    );
    // After padding, and in the two-byte form.
    assert_eq!(turned(0xbede, &[0, 0, 0x40, 2]).map(|o| o.turns), Some(2));
    assert_eq!(
        turned(0x1000, &[5, 2, 0, 9, 0, 4, 1, 3]).map(|o| o.turns),
        Some(3)
    );
    // Other ids, a one-byte value at another id, and none at all.
    assert_eq!(turned(0xbede, &[0x51, 0, 7, 0x70, 1]), None);
    assert_eq!(turned(0xbede, &[]), None);
    // A value running past the block is no orientation, not a panic.
    assert_eq!(turned(0xbede, &[0x5f, 0]), None);
    // Yapfast's own packets carry none.
    let mut packetizer = Packetizer::new(5, Codec::H264);
    let own = packetizer.packets(&[0, 0, 0, 1, 0x65, 1, 2], 0, true);
    assert_eq!(parse(&own[0]).unwrap().orientation, None);
    // A resent packet keeps it.
    let sent = extended(0xbede, &[0x40, 1, 0, 0, 0, 0]);
    let resent = parse(&sent).unwrap();
    let resent = Packet {
        payload: &[0, 7, 1],
        ..resent
    };
    assert_eq!(
        Held::unwrap_rtx(&resent, 4)
            .unwrap()
            .orientation
            .map(|o| o.turns),
        Some(1)
    );
}

#[test]
fn unwraps_resent_packets() {
    let mut packetizer = Packetizer::new(5, Codec::H264);
    let frame = [&[0, 0, 0, 1, 0x41][..], &[7; 20]].concat();
    let original = packetizer.packets(&frame, 3000, false).remove(0);
    let again = packetizer.retransmit(&original, 6).unwrap();
    let resent = parse(&again).unwrap();
    assert_eq!(resent.ssrc, 6);
    let unwrapped = Held::unwrap_rtx(&resent, 5).unwrap();
    let original = parse(&original).unwrap();
    assert_eq!(unwrapped, Held::new(&original));
    let padding = Packet {
        payload: &[],
        ..resent
    };
    assert_eq!(Held::unwrap_rtx(&padding, 5), None);
}

#[test]
fn writes_nacks_and_receiver_reports_that_read_back() {
    let lost = [65530, 65531, 65535, 4, 40];
    let packet = nack(1, 2, &lost);
    assert_eq!(
        packet.len(),
        12 + 2 * 4,
        "65531 to 4 ride on 65530's bitmask"
    );
    assert_eq!(nacked(&packet, 2), lost);
    assert!(nacked(&packet, 3).is_empty());
    let report = receiver_report(1, 2, (70000, 3, 25), (0x1234_5678, 655));
    assert_eq!(report.len(), 32);
    let described = &rtcp_kinds(&report)[0];
    assert_eq!(described.0, "201/1");
    assert_eq!(
        described.1,
        "from 1: ssrc 2 lost 25/256 total 3 highest 70000 jitter 0 lsr 305419896 dlsr 655"
    );
    let sent = sender_report(9, std::time::SystemTime::UNIX_EPOCH, 0, 0, 0);
    // 1970 in NTP time: 2208988800 seconds, no fraction.
    assert_eq!(sender_reports(&sent), [(9, (2_208_988_800u32 << 16))]);
}

#[test]
fn packetizes_and_reassembles_h265() {
    let mut packetizer = Packetizer::new(5, Codec::H265);
    let mut assembler = Assembler::new(Codec::H265);
    // An IDR slice (type 19), its header's second byte the temporal id.
    let idr: Vec<u8> = [19 << 1, 0x01]
        .into_iter()
        .chain((0..3000u32).map(|index| (index % 251) as u8 + 1))
        .collect();
    let keyframe = [
        &[0, 0, 0, 1, 32 << 1, 0x01, 0x0c][..],
        &[0, 0, 0, 1, 33 << 1, 0x01, 0x01],
        &[0, 0, 0, 1, 34 << 1, 0x01, 0xc1],
        &[0, 0, 0, 1],
        &idr,
    ]
    .concat();
    let delta = [&[0, 0, 0, 1, 1 << 1, 0x01][..], &[7; 200]].concat();
    let mut frames = vec![];
    for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
        let sent = packetizer.packets(frame, index as u32 * 3000, false);
        if index == 0 {
            // The parameter sets alone, then the slice in three fragments.
            assert_eq!(sent.len(), 6);
            assert_eq!(
                &sent[3][28..31],
                &[49 << 1, 0x01, 0x80 | 19],
                "starting the IDR slice"
            );
            assert_eq!(sent[5][30], 0x40 | 19, "ending it");
        }
        for packet in &sent {
            let packet = parse(packet).unwrap();
            assert_eq!(packet.payload_type, H265);
            frames.extend(assembler.push(&packet).frame);
        }
    }
    assert_eq!(frames.len(), 2);
    assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
    assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
}

/// An OBU of `kind` that gives its size.
fn obu(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut obu = vec![kind << 3 | OBU_HAS_SIZE];
    write_leb128(payload.len(), &mut obu);
    obu.extend_from_slice(payload);
    obu
}

/// libaom's AV1 of the H264 recording, cut into temporal units at their
/// delimiters.
fn av1_recording() -> Vec<Vec<u8>> {
    let file = include_bytes!("../../../../testing/fixtures/stream-av1.obu");
    let mut units: Vec<Vec<u8>> = vec![];
    let mut rest = &file[..];
    while let Some(&header) = rest.first() {
        let (size, used) = read_leb128(&rest[1..]).unwrap();
        let length = 1 + used + size;
        if (header >> 3) & 0x0f == OBU_TEMPORAL_DELIMITER {
            units.push(vec![]);
        }
        units.last_mut().unwrap().extend_from_slice(&rest[..length]);
        rest = &rest[length..];
    }
    units
}

#[test]
fn packetizes_and_reassembles_av1() {
    let mut packetizer = Packetizer::new(5, Codec::Av1);
    let mut assembler = Assembler::new(Codec::Av1);
    let units = av1_recording();
    assert_eq!(units.len(), 30);
    let mut frames = vec![];
    let mut keyframes = vec![];
    for (index, unit) in units.iter().enumerate() {
        let keyframe = obu_elements(unit)
            .unwrap()
            .iter()
            .any(|element| (element[0] >> 3) & 0x0f == OBU_SEQUENCE_HEADER);
        keyframes.push(keyframe);
        let sent = packetizer.packets(unit, index as u32 * 3000, keyframe);
        for (at, packet) in sent.iter().enumerate() {
            let packet = parse(packet).unwrap();
            assert_eq!(packet.payload_type, AV1);
            assert!(packet.payload.len() <= CHUNK);
            assert_eq!(packet.marker, at + 1 == sent.len());
            // N on a keyframe's first packet only, which starts with
            // its sequence header.
            assert_eq!(packet.payload[0] & 0x08 != 0, keyframe && at == 0);
            frames.extend(assembler.push(&packet).frame);
        }
    }
    assert_eq!(keyframes.iter().filter(|&&key| key).count(), 2);
    assert_eq!(frames.len(), 30);
    for ((frame, unit), keyframe) in frames.iter().zip(&units).zip(keyframes) {
        assert_eq!(frame.keyframe, keyframe);
        // The same OBUs, less the temporal delimiter.
        assert_eq!(obu_elements(&frame.data), obu_elements(unit));
        assert_ne!((frame.data[0] >> 3) & 0x0f, OBU_TEMPORAL_DELIMITER);
    }
}

/// A frame as DAVE leaves it: its last OBU without a size, running into
/// DAVE's trailer, which goes out with it; and an element across three
/// packets, then more than three in one packet.
#[test]
fn packetizes_av1_as_dave_leaves_it() {
    let mut packetizer = Packetizer::new(5, Codec::Av1);
    let mut assembler = Assembler::new(Codec::Av1);
    let tile: Vec<u8> = (0..2500u32).map(|index| (index % 251) as u8).collect();
    let last = [&[6 << 3][..], &[9; 40], b"trailer\xfa\xfa"].concat();
    let frame = [
        obu(OBU_TEMPORAL_DELIMITER, &[]),
        obu(OBU_SEQUENCE_HEADER, &[1; 11]),
        obu(6, &tile),
        obu(OBU_PADDING, &[0; 8]),
        last.clone(),
    ]
    .concat();
    let sent = packetizer.packets(&frame, 0, true);
    assert_eq!(sent.len(), 3);
    let payloads: Vec<&[u8]> = sent
        .iter()
        .map(|packet| parse(packet).unwrap().payload)
        .collect();
    // Z, Y, W and N: the sequence header and the tile's start, the
    // tile's middle alone, its end and the last OBU.
    let headers: Vec<u8> = payloads.iter().map(|payload| payload[0]).collect();
    assert_eq!(
        headers,
        [0x40 | 2 << 4 | 0x08, 0xc0 | 1 << 4, 0x80 | 2 << 4]
    );
    // Elements lose their size flag: the sequence header's is 1 << 3.
    assert_eq!(&payloads[0][1..3], &[12, OBU_SEQUENCE_HEADER << 3]);
    let mut frames = vec![];
    for packet in &sent {
        frames.extend(assembler.push(&parse(packet).unwrap()).frame);
    }
    // The last OBU stays without its size, as DAVE's decryptor needs.
    let expected = [obu(OBU_SEQUENCE_HEADER, &[1; 11]), obu(6, &tile), last].concat();
    assert_eq!(frames.len(), 1);
    assert_eq!((frames[0].keyframe, &frames[0].data), (true, &expected));

    let small: Vec<u8> = (0..5).flat_map(|index| obu(6, &[index; 20])).collect();
    let sent = packetizer.packets(&small, 3000, false);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        parse(&sent[0]).unwrap().payload[0],
        0,
        "five elements, each sized"
    );
    let frame = assembler.push(&parse(&sent[0]).unwrap()).frame.unwrap();
    assert!(!frame.keyframe);
    assert_eq!(obu_elements(&frame.data), obu_elements(&small));
    assert_eq!(
        frame.data.len(),
        small.len() - 1,
        "the last without its size"
    );

    // The tile's middle lost: the frame is dropped, and frames after it
    // until a keyframe.
    let sent = packetizer.packets(&obu(6, &tile), 6000, false);
    assert!(assembler.push(&parse(&sent[0]).unwrap()).frame.is_none());
    let pushed = assembler.push(&parse(&sent[2]).unwrap());
    assert_eq!((pushed.frame, pushed.lost), (None, true));
    let sent = packetizer.packets(&small, 9000, false);
    assert!(assembler.push(&parse(&sent[0]).unwrap()).frame.is_none());
}

#[test]
fn sends_nothing_of_a_malformed_av1_frame() {
    let malformed: [&[u8]; 6] = [
        &[0x80, 0x00],       // the forbidden bit
        &[0x0a],             // a size flag without its size
        &[0x0a, 0x80],       // a size that never ends
        &[0x0a, 0x02, 0xaa], // a payload past the frame
        &[0x0c],             // an extension flag without the extension
        &[0x0c, 0x01],       // the extension's reserved bits
    ];
    let mut packetizer = Packetizer::new(77, Codec::Av1);
    for frame in malformed {
        assert!(
            packetizer.packets(frame, 9000, true).is_empty(),
            "{frame:?}"
        );
    }
    assert!(
        packetizer
            .packets(&vec![0; MAX_FRAME + 1], 9000, false)
            .is_empty()
    );
}

/// OBUs whose extensions name different temporal or spatial layers go
/// in packets of their own; those without one go along with either.
#[test]
fn keeps_av1_layers_apart() {
    let mut packetizer = Packetizer::new(77, Codec::Av1);
    let layered = |id: u8| vec![6 << 3 | OBU_EXTENSION | OBU_HAS_SIZE, id << 5, 3, 1, 2, 3];
    let frame = [
        obu(OBU_SEQUENCE_HEADER, &[1; 4]),
        layered(0),
        layered(0),
        layered(1),
    ]
    .concat();
    let sent = packetizer.packets(&frame, 0, true);
    let counts: Vec<u8> = sent
        .iter()
        .map(|packet| (parse(packet).unwrap().payload[0] >> 4) & 0x03)
        .collect();
    assert_eq!(counts, [3, 1]);
}

#[test]
fn retransmits_av1_with_its_rtx_payload_type() {
    let mut packetizer = Packetizer::new(5, Codec::Av1);
    let original = packetizer
        .packets(&obu(OBU_SEQUENCE_HEADER, &[0xaa]), 3000, true)
        .remove(0);
    let resent = packetizer.retransmit(&original, 6).unwrap();
    let resent = parse(&resent).unwrap();
    assert_eq!(resent.payload_type, AV1_RTX);
    let restored = Held::unwrap_rtx(&resent, 5).unwrap();
    assert_eq!((restored.payload_type, restored.timestamp), (AV1, 3000));
    assert_eq!(restored.payload, parse(&original).unwrap().payload);
}

#[test]
fn reads_and_writes_leb128() {
    for value in [0, 1, 127, 128, 1100, 16383, 16384, 1 << 30] {
        let mut bytes = vec![];
        write_leb128(value, &mut bytes);
        assert_eq!(bytes.len(), leb128_size(value));
        assert_eq!(read_leb128(&bytes), Some((value, bytes.len())));
    }
    // An encoder may pad a size to more bytes than it needs.
    assert_eq!(read_leb128(&[0x85, 0x80, 0x00]), Some((5, 3)));
    assert_eq!(read_leb128(&[0x80]), None);
}

#[test]
fn packetizes_and_reassembles_h264() {
    let mut packetizer = Packetizer::new(5, Codec::H264);
    let mut assembler = Assembler::new(Codec::H264);
    let idr: Vec<u8> = [0x65]
        .into_iter()
        .chain((0..3000u32).map(|index| (index % 251) as u8 + 1))
        .collect();
    let keyframe = [
        &[0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f][..],
        &[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80],
        &[0, 0, 0, 1],
        &idr,
    ]
    .concat();
    let delta = [&[0, 0, 0, 1, 0x41][..], &[7; 200]].concat();
    let mut frames = vec![];
    for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
        let sent = packetizer.packets(frame, index as u32 * 3000, false);
        if index == 0 {
            // The parameter sets alone, then the slice in three fragments.
            assert_eq!(sent.len(), 5);
            assert_eq!(sent[2][28], (0x65 & 0xe0) | 28, "an FU-A indicator");
            assert_eq!(sent[2][29], 0x80 | 5, "starting an IDR slice");
            assert_eq!(sent[4][29], 0x40 | 5, "ending it");
        }
        for packet in &sent {
            let packet = parse(packet).unwrap();
            assert_eq!(packet.payload_type, H264);
            frames.extend(assembler.push(&packet).frame);
        }
    }
    assert_eq!(frames.len(), 2);
    assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
    assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
}

#[test]
fn finds_nal_units_as_libwebrtc_does() {
    // A unit ending in a zero before a long start code keeps it; a
    // short start code works too.
    let frame = [
        0, 0, 0, 1, 0x41, 9, 0, 0, 0, 0, 1, 0x41, 8, 0, 0, 1, 0x68, 7,
    ];
    assert_eq!(
        nal_units(&frame),
        [&[0x41, 9, 0][..], &[0x41, 8], &[0x68, 7]]
    );
    assert!(nal_units(&[1, 2, 3]).is_empty());
}

#[test]
fn reassembles_h264_units() {
    let mut assembler = Assembler::new(Codec::H264);
    // An SPS and PPS together, then an IDR slice in two fragments.
    let stap = [24, 0, 2, 0x67, 1, 0, 2, 0x68, 2];
    let first = [0x7c, 0x85, 10, 11];
    let last = [0x7c, 0x45, 12];
    assert_eq!(
        assembler.push(&parse(&packet(1, 90, false, &stap)).unwrap()),
        Pushed::default()
    );
    assembler.push(&parse(&packet(2, 90, false, &first)).unwrap());
    let frame = assembler
        .push(&parse(&packet(3, 90, true, &last)).unwrap())
        .frame
        .unwrap();
    assert!(frame.keyframe);
    assert_eq!(
        frame.data,
        [
            0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 10, 11, 12
        ]
    );
    // A slice alone is a delta frame.
    let delta = assembler
        .push(&parse(&packet(4, 180, true, &[0x41, 7])).unwrap())
        .frame
        .unwrap();
    assert_eq!(
        (delta.keyframe, delta.data),
        (false, vec![0, 0, 0, 1, 0x41, 7])
    );
    assert_eq!(
        picture_loss(1, 2),
        [0x81, 206, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2]
    );
}

#[test]
fn sender_reports_say_when_a_timestamp_was() {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_500);
    let report = sender_report(7, at, 90_000, 12, 3400);
    assert_eq!(report.len(), 28, "a header and 20 bytes of sender info");
    assert_eq!(&report[..4], &[0x80, 200, 0, 6]);
    assert!(is_rtcp(&report));
    let word = |at: usize| u32::from_be_bytes(report[at..at + 4].try_into().unwrap());
    assert_eq!(word(4), 7);
    assert_eq!(word(8), 2_208_988_801);
    assert_eq!(word(12), 1 << 31, "half a second");
    assert_eq!((word(16), word(20), word(24)), (90_000, 12, 3400));
}

#[test]
fn describes_receiver_reports_and_remb() {
    let mut report = vec![0x81, 201, 0, 7, 0, 0, 0, 1];
    for word in [55u32, (12 << 24) | 3, 70_000, 9, 0, 0] {
        report.extend_from_slice(&word.to_be_bytes());
    }
    let mut remb = vec![0x8f, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0];
    remb.extend_from_slice(b"REMB");
    // One SSRC, 2^2 * 250_000.
    remb.extend_from_slice(&[1, 2 << 2 | 3, 0xd0, 0x90]);
    remb.extend_from_slice(&55u32.to_be_bytes());
    let kinds = rtcp_kinds(&[report.clone(), remb.clone()].concat());
    assert_eq!(
        kinds,
        [
            (
                "201/1".to_owned(),
                "from 1: ssrc 55 lost 12/256 total 3 highest 70000 jitter 9 lsr 0 dlsr 0"
                    .to_owned()
            ),
            ("206/15".to_owned(), "1000000 bit/s for [55]".to_owned()),
        ]
    );
    let compound = [report, remb].concat();
    assert_eq!(bandwidth_estimate(&compound, 55), Some(1_000_000));
    assert_eq!(
        bandwidth_estimate(&compound, 56),
        None,
        "about another stream"
    );
}

#[test]
fn retransmits_what_viewers_report_lost() {
    let mut packetizer = Packetizer::new(77, Codec::H264);
    let mut history = History::default();
    let frame = [&[0, 0, 0, 1, 0x65][..], &[9; 3000]].concat();
    let sent = packetizer.packets(&frame, 9000, false);
    for packet in &sent {
        history.remember(packet);
    }
    let first = parse(&sent[0]).unwrap().sequence;
    // A NACK for the first packet and, by its bitmask, the third.
    let mut nack = vec![0x81, 205, 0, 3, 0, 0, 0, 1];
    nack.extend_from_slice(&77u32.to_be_bytes());
    nack.extend_from_slice(&first.to_be_bytes());
    nack.extend_from_slice(&0b10u16.to_be_bytes());
    let lost = nacked(&nack, 77);
    assert_eq!(lost, [first, first.wrapping_add(2)]);
    assert!(nacked(&nack, 78).is_empty(), "about another stream");
    let again = packetizer
        .retransmit(history.get(lost[1]).unwrap(), 79)
        .unwrap();
    let original = parse(&sent[2]).unwrap();
    let resent = parse(&again).unwrap();
    assert_eq!(
        (
            resent.payload_type,
            resent.ssrc,
            resent.timestamp,
            resent.marker
        ),
        (H264_RTX, 79, 9000, original.marker)
    );
    assert_eq!(&resent.payload[..2], &lost[1].to_be_bytes());
    assert_eq!(&resent.payload[2..], original.payload);
    assert_eq!(again[21], 0xc2, "the repaired stream id, not the stream id");
    assert!(history.get(first.wrapping_sub(1)).is_none(), "never sent");
    assert!(packetizer.retransmit(&again, 79).is_none(), "not video");
}

#[test]
fn finds_keyframe_requests() {
    // A receiver report, then a PLI.
    let report = [0x80, 201, 0, 1, 0, 0, 0, 1];
    let pli = [0x81, 206, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2];
    assert!(!wants_keyframe(&report));
    assert!(wants_keyframe(&[&report[..], &pli[..]].concat()));
    assert!(is_rtcp(&pli) && !is_rtcp(&[0x90, VP8]));
}
