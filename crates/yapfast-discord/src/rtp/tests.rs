use super::*;

use crate::congestion::PACE;

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
            reporter: 1,
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
        // Opening decrypts the packet where it lies: the buffer handed
        // over is the plaintext itself.
        let mut opening = packet.clone();
        assert_eq!(transport.open(&mut opening, clear), Some(&plain[..]));
        let mut tampered = packet.clone();
        tampered[clear] ^= 1;
        assert_eq!(
            transport.open(&mut tampered, clear),
            None,
            "tampering fails"
        );
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
    // 3 was given up on, and comes anyway: it arrived, so the report says
    // the path lost one rather than two.
    assert_eq!(reorder.insert(held(3), at(410)), Arrival::Recovered);
    assert_eq!(reorder.counts(), Some((65536 + 4, 1)), "one wrap, one lost");
    // And once it has come, another copy of it isn't a second packet.
    assert_eq!(reorder.insert(held(3), at(420)), Arrival::Late);
    assert_eq!(reorder.counts(), Some((65536 + 4, 1)));
}

#[test]
fn counts_a_packet_that_came_late_as_arrived() {
    // A report says what the path lost, not what arrived out of order: the
    // delivery point went on without 12, so a count read from it called 12
    // lost, and a streamer reads that as its own uplink losing it.
    let now = std::time::Instant::now();
    let later = now + std::time::Duration::from_millis(400);
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
    for sequence in [10, 11, 13, 14] {
        reorder.insert(held(sequence), now);
    }
    assert_eq!(reorder.ready(now).0.len(), 2, "10 and 11, without 12");
    assert_eq!(reorder.counts(), Some((14, 1)));
    assert_eq!(reorder.ready(later).0.len(), 2, "13 and 14, given up on 12");
    assert_eq!(reorder.counts(), Some((14, 1)), "12 never came");
    assert_eq!(reorder.insert(held(12), later), Arrival::Recovered);
    assert_eq!(reorder.counts(), Some((14, 0)), "and then it did");
    // A second copy of it is not a second packet.
    assert_eq!(reorder.insert(held(12), later), Arrival::Late);
    assert_eq!(reorder.counts(), Some((14, 0)));
}

#[test]
fn remembers_the_arrivals_near_the_highest() {
    let mut arrived = Arrived::default();
    assert!(arrived.note(1000));
    assert!(!arrived.note(1000), "a duplicate");
    assert!(arrived.note(999), "out of order, and new");
    assert_eq!(arrived.count, 2);
    // Older than what's remembered: it can't be told from a packet counted
    // when it first came, so it isn't counted again.
    assert!(!arrived.note(1000 - ARRIVED));
    assert_eq!(arrived.count, 2);
    // A window that moves keeps what it holds, across a word's worth of it.
    let mut arrived = Arrived::default();
    arrived.note(100);
    arrived.note(99);
    arrived.note(170);
    assert!(!arrived.note(100));
    assert!(!arrived.note(99));
    assert!(arrived.note(98));
    assert_eq!(arrived.count, 4);
    // Far ahead, everything before it is out of reach and it counts.
    assert!(arrived.note(170 + ARRIVED * 4));
    assert!(!arrived.note(169));
    assert_eq!(arrived.count, 5);
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
    let sent_at = Instant::now();
    let frame = [&[0, 0, 0, 1, 0x65][..], &[9; 3000]].concat();
    let sent = packetizer.packets(&frame, 9000, false);
    for packet in &sent {
        history.remember(packet, sent_at);
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
        .retransmit(history.get(lost[1], sent_at).unwrap(), 79)
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
    assert!(
        history.get(first.wrapping_sub(1), sent_at).is_none(),
        "never sent"
    );
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

/// Discord's stream server relays every viewer's report in one datagram, so
/// the same hole is named in two blocks: one resend answers both, and the
/// second name must not cost a second packet.
#[test]
fn a_packet_two_viewers_lost_is_named_once() {
    let mut packet = vec![0x81, 205, 0, 4, 0, 0, 0, 1];
    packet.extend_from_slice(&9u32.to_be_bytes());
    for _ in 0..2 {
        packet.extend_from_slice(&7u16.to_be_bytes());
        packet.extend_from_slice(&0b100u16.to_be_bytes());
    }
    assert_eq!(nacked(&packet, 9), [7, 10]);
}

/// Two sequences a history apart: they land on the same slot.
fn same_slot(sequence: u16, slots: usize) -> u16 {
    sequence + slots as u16
}

#[test]
fn reuses_the_vector_in_an_rtx_history_slot() {
    let mut history = History::default();
    let slots = history.packets.len();
    let first = 1000u16;
    let second = same_slot(first, slots);
    assert_eq!(
        usize::from(first) % slots,
        usize::from(second) % slots,
        "the same slot"
    );
    let before = packet(first, 500, false, &[1; 400]);
    let after = packet(second, 900, true, &[2; 300]);

    history.remember(&before, Instant::now());
    let slot = history.packets[usize::from(first) % slots]
        .as_ref()
        .expect("a slot filled");
    assert_eq!(
        (slot.sequence, slot.packet.as_slice()),
        (first, before.as_slice())
    );
    let storage = (slot.packet.as_ptr(), slot.packet.capacity());

    let replaced_at = Instant::now();
    history.remember(&after, replaced_at);
    let slot = history.packets[usize::from(second) % slots]
        .as_ref()
        .expect("the slot holds a packet still");
    assert_eq!(
        (slot.packet.as_ptr(), slot.packet.capacity()),
        storage,
        "the slot's own vector, not a new one"
    );
    assert_eq!(
        (slot.sequence, slot.packet.as_slice()),
        (second, after.as_slice()),
        "the packet just kept, with no bytes left over from the last"
    );
    assert_eq!(
        history.get(first, replaced_at),
        None,
        "replaced in that slot"
    );
    assert_eq!(history.get(second, replaced_at), Some(after.as_slice()));
}

#[test]
fn an_rtx_history_slot_holds_packets_of_changing_length() {
    let mut history = History::default();
    let slots = history.packets.len();
    let sequence = 7u16;
    let long_sequence = same_slot(sequence, slots);
    let shortest_sequence = sequence + 2 * (slots as u16);
    let short = packet(sequence, 1, false, &[3; 100]);
    let long = packet(long_sequence, 2, false, &[4; 2000]);
    let shortest = packet(shortest_sequence, 3, true, &[5; 20]);
    for other in [long_sequence, shortest_sequence] {
        assert_eq!(
            usize::from(other) % slots,
            usize::from(sequence) % slots,
            "the same slot"
        );
    }
    let slot = usize::from(sequence) % slots;
    let sent_at = Instant::now();

    history.remember(&short, sent_at);
    history.remember(&long, sent_at);
    let stored = &history.packets[slot]
        .as_ref()
        .expect("a slot filled")
        .packet;
    assert_eq!(stored.as_slice(), long.as_slice(), "grown to fit");
    assert!(stored.capacity() >= long.len(), "room for the whole packet");
    let grown = stored.capacity();

    history.remember(&shortest, sent_at);
    let stored = &history.packets[slot]
        .as_ref()
        .expect("a slot filled")
        .packet;
    assert_eq!(
        stored.as_slice(),
        shortest.as_slice(),
        "cut to the shorter packet"
    );
    assert_eq!(stored.capacity(), grown, "the larger vector kept");
    assert_eq!(history.get(long_sequence, sent_at), None, "replaced");
    assert_eq!(
        history.get(shortest_sequence, sent_at),
        Some(shortest.as_slice())
    );
}

#[test]
fn an_rtx_history_slot_validates_its_sequence_across_wraparound() {
    let mut history = History::default();
    let slots = history.packets.len();
    let before_wrap = 65_500u16;
    let after_wrap = before_wrap.wrapping_add(slots as u16);
    assert!(
        after_wrap < before_wrap,
        "across the end of the sequence numbers"
    );
    assert_eq!(
        usize::from(after_wrap) % slots,
        usize::from(before_wrap) % slots,
        "the same slot"
    );
    let tail = packet(before_wrap, 1, false, &[6; 200]);
    let head = packet(after_wrap, 2, true, &[7; 200]);

    let sent_at = Instant::now();
    history.remember(&tail, sent_at);
    assert_eq!(history.get(before_wrap, sent_at), Some(tail.as_slice()));

    history.remember(&head, sent_at);
    assert_eq!(history.get(after_wrap, sent_at), Some(head.as_slice()));
    assert_eq!(
        history.get(before_wrap, sent_at),
        None,
        "replaced in that slot"
    );
    assert!(
        history.get(1234, sent_at).is_none(),
        "a slot nothing has filled"
    );
}

#[test]
fn rtx_history_remembers_only_parsable_packets() {
    let mut history = History::default();
    let sent_at = Instant::now();
    history.remember(&[], sent_at);
    history.remember(&[0x80, H264], sent_at);
    history.remember(&[0x40; 20], sent_at);
    assert!(
        history.packets.iter().all(|slot| slot.is_none()),
        "no slot filled, nor partly"
    );
}

#[test]
fn an_rtx_slot_ages_out_rather_than_lasting_through_a_stall() {
    // Nothing ages a slot but the packets that follow it, so a stream that
    // stops sending leaves the history as it was: minutes later a viewer's
    // NACK for one of those packets is about a packet no viewer can still
    // be waiting for, and the uplink a recovering stream needs is not
    // spent on it. What the slot holds is never handed out in its place.
    let mut history = History::default();
    let sent_at = Instant::now();
    let stalled = sent_at + RESEND_WINDOW + Duration::from_millis(1);
    let old = packet(200, 1, false, &[8; 300]);
    let new = packet(same_slot(200, history.packets.len()), 2, true, &[9; 300]);

    history.remember(&old, sent_at);
    assert_eq!(history.get(200, sent_at), Some(old.as_slice()));
    assert_eq!(
        history.get(200, stalled),
        None,
        "in the history still, and too old to resend"
    );

    // The slot takes the packet the sequence numbers put in it next, with
    // their own bytes: neither the old sequence nor the new one is ever
    // answered with the other's packet.
    let replaced_at = Instant::now();
    history.remember(&new, replaced_at);
    let kept = parse(&new).unwrap().sequence;
    assert_eq!(history.get(200, replaced_at), None, "gone from that slot");
    assert_eq!(history.get(kept, replaced_at), Some(new.as_slice()));
    assert_eq!(
        history.get(kept, replaced_at + RESEND_WINDOW),
        None,
        "and ages out in its turn"
    );
}

#[test]
fn an_rtx_slot_costs_no_more_than_the_packet_alone() {
    // What a slot keeps for ageing it is the room the tuple had spare:
    // the store is bounded by its 2048 slots whatever the stream does.
    assert_eq!(
        std::mem::size_of::<Option<Sent>>(),
        std::mem::size_of::<Option<(u16, Vec<u8>)>>(),
        "a kept packet, its sequence and its age in a packet's worth of slot"
    );
}

/// What each turn of the send loop writes for a run of `packets` at `target`
/// and `factor`, on a timer whose turns take `turn`.
fn paced(target: u64, packets: usize, turn: Duration, factor: u64) -> Vec<usize> {
    let start = Instant::now();
    let mut pace = Pace::new(start);
    let mut at = start;
    let mut left = packets;
    let mut sizes = vec![];
    while left > 0 {
        let batch = pace.batch(target, factor, at).min(left);
        for _ in 0..batch {
            pace.went();
        }
        left -= batch;
        sizes.push(batch);
        pace.wrote(at);
        at += turn;
    }
    sizes
}

#[test]
fn a_run_goes_out_a_batch_a_turn() {
    // Each turn of the send loop writes one batch and polls the rest of its
    // arms before the next, so a keyframe of 250 packets is a hundred turns
    // for sound, feedback and the gateway to be heard in, rather than one
    // turn that lasts as long as the frame takes to put on the wire.
    let sizes = paced(9_000_000, 250, STEP, PACE);
    assert_eq!(sizes[0], BURST, "a run starts at once");
    assert!(
        sizes[1..].iter().all(|&batch| (2..=3).contains(&batch)),
        "and goes on two packets and a quarter a turn: {sizes:?}"
    );
    assert_eq!(sizes.len(), 109, "{sizes:?}");
}

#[test]
fn a_fine_timer_paces_at_the_multiple_of_the_target() {
    // A 1 ms turn pays for a packet and a fraction at most rates. Rounded up
    // to a floor it sent an 8.7 Mbit keyframe at 40 Mbit into a path the
    // controller had just backed off from; carried, a second of turns sends
    // what 2.5 times the target pays for, and no more.
    for target in [500_000, 2_500_000, 8_700_000, 20_000_000] {
        let sizes = paced(target, BURST + 6_000, STEP, PACE);
        let second: usize = sizes[1..=1000].iter().sum();
        let paid = (target * PACE / 100 / (8 * PACE_PACKET as u64)) as usize;
        assert!(
            second.abs_diff(paid) <= 1,
            "{target} bit/s sent {second} packets in a second for {paid}"
        );
    }
    // A rate that pays for less than a packet a turn writes none in most, and
    // waits for the next rather than sending ahead of the target.
    let sizes = paced(500_000, BURST + 10, STEP, PACE);
    assert!(sizes[1..].iter().all(|&batch| batch <= 1), "{sizes:?}");
}

#[test]
fn a_slow_turn_sends_what_the_target_pays_for() {
    // Windows serves a 1 ms sleep in 15.6 ms, measured, so a batch sized by
    // the constant alone would put a 20 Mbit stream out at under four Mbit.
    // The batch is paid for over the time the turn took, so a coarse timer
    // paces at the multiple a fine one does.
    let windows = Duration::from_micros(15_600);
    let turn = |target| paced(target, BURST + 1_000, windows, PACE)[1];
    assert_eq!(turn(9_000_000), 35, "2.5 times 9 Mbit for 15.6 ms");
    assert_eq!(turn(2_500_000), 9);
    assert_eq!(turn(20_805_000), MAX_BATCH, "still bounded");
    assert_eq!(turn(120_000_000), MAX_BATCH);
    // A turn that came late sends what the bound lets it, and owes the rest
    // of its wait to nobody.
    let start = Instant::now();
    let mut pace = Pace::new(start);
    for _ in 0..BURST {
        pace.went();
    }
    let late = start + Duration::from_secs(1);
    assert_eq!(pace.batch(9_000_000, PACE, late), MAX_BATCH);
    for _ in 0..MAX_BATCH {
        pace.went();
    }
    pace.wrote(late);
    assert_eq!(pace.batch(9_000_000, PACE, late + STEP), 2);
}

#[test]
fn resends_wait_their_turn_in_the_order_they_were_asked_for() {
    let at = Instant::now();
    let mut again = Resends::default();
    assert!(!again.waiting());
    // A viewer names what a gap left it missing: the packets a burst lost.
    for sequence in [40, 39, 41] {
        assert!(again.ask(sequence, at, None), "{sequence}");
    }
    assert!(again.waiting());
    assert_eq!(again.due(at), at, "the first batch is due at once");
    assert!(
        again.batch(9_000_000, PACE, at) >= 3,
        "the burst covers three"
    );
    assert_eq!(
        std::iter::from_fn(|| again.next(at)).collect::<Vec<_>>(),
        [40, 39, 41],
        "longest waiting first"
    );
    assert!(!again.waiting());
}

#[test]
fn a_repeat_request_is_answered_once_while_the_first_copy_is_on_its_way() {
    // A viewer asks again every 80 ms while it holds a gap, so on a path whose
    // round trip is worth waiting for, the second request is the same lost
    // packet: one copy is already on its way, and a second is the flood that
    // lost it. Past half a round trip the first copy would be a lost cause.
    let at = Instant::now();
    let window = Duration::from_millis(40);
    let mut again = Resends::default();
    assert!(again.ask(7, at, Some(window)));
    assert!(!again.ask(7, at, Some(window)), "named twice in one report");
    assert_eq!(again.next(at), Some(7));
    assert!(
        !again.ask(7, at + Duration::from_millis(20), Some(window)),
        "the copy is still on its way"
    );
    assert!(
        again.ask(7, at + Duration::from_millis(41), Some(window)),
        "that copy would have gone astray by now"
    );
    assert!(
        again.ask(8, at + Duration::from_millis(41), Some(window)),
        "another packet is its own"
    );
    // With no round trip measured, nothing is assumed to be on its way.
    let mut unmeasured = Resends::default();
    assert!(unmeasured.ask(9, at, None));
    assert_eq!(unmeasured.next(at), Some(9));
    assert!(unmeasured.ask(9, at, None), "asked again, sent again");
}

#[test]
fn a_run_of_resends_is_paced_past_its_burst() {
    // A NACK can name a hundred packets, and it names them because a burst
    // just left the path's queue full: one burst back fills it again. So they
    // go out as a frame's packets do, eight at once and the rest a turn at the
    // rate, where the whole hundred went at once before.
    let at = Instant::now();
    let mut again = Resends::default();
    for sequence in 0..100 {
        assert!(again.ask(sequence, at, None), "{sequence}");
    }
    assert_eq!(again.batch(9_000_000, PACE, at), BURST);
    for _ in 0..BURST {
        assert!(again.next(at).is_some());
    }
    again.wrote(at);
    assert_eq!(again.due(at), at + STEP, "the next batch waits a turn");
    assert_eq!(again.batch(9_000_000, PACE, at + STEP), 2);
}

#[test]
fn an_audio_packet_takes_the_room_the_last_one_left() {
    // A call and a stream write a frame every 20 ms into the same buffer:
    // the room it has is room the allocator isn't asked for again.
    let mut packet = Vec::new();
    write_audio_into(&mut packet, 1, 0, 0, true, b"the first frame");
    let room = packet.capacity();
    write_audio_into(&mut packet, 1, 1, 960, false, b"the next frame");
    assert_eq!(packet.capacity(), room, "the room carries over");
    assert_eq!(packet.len(), HEADER + b"the next frame".len());
    assert_eq!(&packet[HEADER..], b"the next frame", "the frame it holds");
    assert_eq!(packet[1] & 0x7f, OPUS);
    assert_eq!(packet[1] & 0x80, 0, "only the first frame marks a burst");
    assert_eq!(packet, opus_packet(1, 1, 960, false, b"the next frame"));
    // The room past the frame is for sealing it where it lies.
    assert!(packet.capacity() >= packet.len() + TAG + 4);
}
