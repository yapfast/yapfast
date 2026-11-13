//! Video frames from the stream worker to the UI, through a memory-mapped
//! file both processes open. Three slots make a triple buffer: the worker
//! fills its back slot and swaps it into the middle; the UI swaps the middle
//! out when it's newer than what it shows. Neither waits for the other, and
//! neither reads a slot the other is writing.
//!
//! The file lives in the runtime directory, which is memory on Linux, and is
//! removed once the worker has it open, so the memory returns when both
//! processes let go. Windows won't remove a mapped file, so there it's a
//! temporary file, kept in memory while it fits, that goes once the last
//! process closes it.
//!
//! The file is made for the size its frames show at, so a stream shown in a
//! small tile costs a small file. One shown larger than the file was made for
//! grows it: the worker lengthens the file and maps it again, and the header
//! says where the slots lie now, which is how the UI's mapping learns it must
//! be made again too. A file gives its room back as well: a share that went
//! full-screen once doesn't hold four-K slots for the stream's life, since the
//! worker shortens the file once the frames have needed a fraction of it for
//! long enough. A reader keeps its view from take to take, holding the file
//! shared while it does, so a frame costs no mapping of the file and no page
//! faults over it; nothing is ever shortened under a mapping, since the worker
//! takes the file to itself for it. It asks for that in the header first, and
//! a reader that sees the request lets its view go after the take it is in
//! and maps the file for each take until the room has gone back.
//!
//! A slot holds either pixels or a picture the worker drew on the GPU and
//! shared with the UI's process, which the UI shows without copying it: the
//! header says which, slot by slot, so a reader never takes one for the
//! other. A stream that doesn't draw on the GPU is a stream of pixels as it
//! always was, and one whose pictures the UI can't open says so in the header
//! and goes on with pixels from its next frame: a picture that can't be shown
//! is a slow stream, not a blank one.
//!
//! On Windows a picture crosses as a shared texture's handle, a number that
//! names the same object in both processes, so the header carries it. On Linux
//! it is a DMA-BUF, and a descriptor names a different object in every process:
//! the header carries the shape of the picture and the number of the socket it
//! is sent over instead, and the worker sends one message for each picture it
//! publishes with the descriptors attached; see [`crate::dmabuf`]. The slot,
//! generation, take and retirement protocol is the same on both, so a slot is
//! never drawn into again while the window may still be sampling it.
//!
//! A GPU slot is the worker's to draw into again only once the UI says it has
//! finished with the picture that was there, slot by slot and generation by
//! generation. Taking a picture is not finishing with it: the window shows it
//! for as long as it is on screen, and the GPU may still be sampling the
//! texture for a frame after the next one replaced it. So the reader says so
//! itself, through [`Frames::retire`], when the rendering queue has passed the
//! picture it took, and nothing else counts as the worker's leave to draw
//! there. A window that stops saying it — one that was closed, or whose
//! renderer lost the stream — is not waited on for the rest of the share: after
//! a second, the worker stops asking for a slot and writes the frame's pixels,
//! which touch no texture and never wait on the reader.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use memmap2::MmapMut;

#[cfg(target_os = "linux")]
use crate::dmabuf;

const _: () = assert!(size_of::<Header>() <= HEADER);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_pass_from_writer_to_reader() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        assert!(frames.path().is_none());
        if cfg!(not(windows)) {
            assert!(!path.exists());
        }
        assert!(frames.take(|_, _, _| ()).is_none(), "nothing yet");
        assert!(writer.fits(2, 4) && !writer.fits(5, 2));

        writer.write(2, 1, |pixels| pixels.fill(1));
        writer.write(4, 2, |pixels| pixels.fill(2));
        let newest = frames.take(|width, height, pixels| (width, height, pixels.to_vec()));
        assert_eq!(newest, Some((4, 2, vec![2; 32])), "only the newest");
        assert!(frames.take(|_, _, _| ()).is_none(), "taken once");

        // Many frames while the UI holds one: it never sees a partial frame.
        for value in 3..10 {
            writer.write(1, 1, |pixels| pixels.fill(value));
        }
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![9; 4])
        );

        // A video's frames carry their time.
        writer.write_timed(1, 1, 1234, |pixels| pixels.fill(0));
        assert_eq!(frames.take_timed(|_, _, _| ()), Some(((), 1234)));

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A frame larger than the slots grows the file, and the UI's mapping with
    /// it: a stream shown in a larger window is written at its size, rather
    /// than scaled down to what the file was made for. Growing moves every
    /// slot, so what the UI reads after it is the frame the worker wrote
    /// there, at the size it wrote it.
    #[test]
    fn frames_grow_for_pictures_larger_than_their_slots() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        writer.write(4, 2, |pixels| pixels.fill(1));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![1; 32]))
        );
        assert!(!writer.fits(8, 4), "eight by four is larger than a slot");

        writer.write(8, 4, |pixels| pixels.fill(2));
        assert!(writer.fits(8, 4), "the slots grew with the frame");
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((8, 4, vec![2; 128]))
        );

        // A frame smaller than the grown slots still shows at its own size.
        writer.write(2, 1, |pixels| pixels.fill(3));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((2, 1, vec![3; 8]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A file that grew for a picture shown large gives the room back once the
    /// frames have needed a fraction of it for long enough: a share that went
    /// full-screen and came back to a tile holds slots the size of the screen
    /// for the stream's life otherwise. The frame whose slot moved with the
    /// room is not read at the old stride, and the frame written after it is
    /// read where it was written.
    #[test]
    fn frames_give_back_the_room_a_large_picture_took() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        assert!(writer.fits(64, 48), "the slots grew for the picture");
        assert_eq!(
            frames.take(|width, height, _| (width, height)),
            Some((64, 48))
        );
        let grown = writer.file.metadata().unwrap().len();
        assert_eq!(grown, HEADER as u64 + SLOTS as u64 * 64 * 48 * 4);

        // A frame much smaller than the slots, and the wait the room goes back
        // after: a test's wait is none, and a stream's is ten seconds. The
        // reader kept its view from the take before, so the worker's first ask
        // finds the file held, and the room waits for the reader.
        writer.write(4, 2, |pixels| pixels.fill(2));
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert_eq!(
            writer.file.metadata().unwrap().len(),
            grown,
            "a view kept between takes keeps the room"
        );
        // The reader's next take reads at the stride its view has, and lets the
        // view go after it, having seen the ask.
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![2; 32]))
        );
        writer.write(4, 2, |pixels| pixels.fill(4));
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        let held = writer.file.metadata().unwrap().len();
        assert_eq!(held, HEADER as u64 + SLOTS as u64 * 4 * 2 * 4);
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "the frame whose slot moved went with the room"
        );

        // The frame written now is read at the stride the slots have.
        writer.write(4, 2, |pixels| pixels.fill(3));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![3; 32]))
        );
        assert_eq!(
            writer.file.metadata().unwrap().len(),
            held,
            "a frame of the size the file settled at doesn't grow it again"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A reader keeps its view from take to take, so a frame costs no mapping of
    /// the file and no page faults over it: the second take reads through the
    /// view the first one made.
    #[test]
    fn a_reader_keeps_its_view_between_takes() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        let view = |frames: &Frames| {
            frames
                .front
                .lock()
                .unwrap()
                .kept
                .as_ref()
                .map(|kept| kept.map.as_ptr())
        };
        writer.write(4, 2, |pixels| pixels.fill(1));
        assert!(frames.take(|_, _, _| ()).is_some());
        let first = view(&frames);
        assert!(first.is_some(), "the view outlives the take");
        writer.write(4, 2, |pixels| pixels.fill(2));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![2; 32])
        );
        assert_eq!(view(&frames), first, "the second take used the same view");
        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// Measures a take of a 4K frame, the cost a stream shown full-screen pays a
    /// frame on the window's thread.
    #[test]
    #[ignore = "measures; run with --ignored --nocapture"]
    fn measures_a_take_of_a_4k_frame() {
        let frames = Frames::create(3840, 2160).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        let mut spent = Duration::ZERO;
        let mut sum = 0u64;
        for frame in 0..200u32 {
            writer.write(3840, 2160, |pixels| pixels.fill(frame as u8));
            let started = Instant::now();
            sum += frames
                .take(|_, _, pixels| {
                    pixels
                        .iter()
                        .step_by(4096)
                        .map(|&b| u64::from(b))
                        .sum::<u64>()
                })
                .unwrap();
            spent += started.elapsed();
        }
        eprintln!("a 4K take: {:?} on average ({sum})", spent / 200);
    }

    /// The worker asks for the file, and the reader, which kept its view from
    /// its last take, lets it go at the end of the next one: what a stream's
    /// frames do over two of them before the room can go back.
    fn reader_lets_go(writer: &mut Writer, frames: &Frames) {
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        let _ = frames.take(|_, _, _| ());
        assert!(
            frames.front.lock().unwrap().kept.is_none(),
            "the reader let its view go once asked"
        );
    }

    /// A picture the worker drew on the GPU, as it hands one to
    /// [`Writer::publish_into`]: handles 512 and 1024 are no real handles, but
    /// they say which texture a test published.
    fn drawn(texture: u64, width: u32, height: u32) -> Drawn {
        drawn_in(1, texture, width, height)
    }

    /// The same, drawn in the set of textures and fence numbered `set`.
    fn drawn_in(set: u64, texture: u64, width: u32, height: u32) -> Drawn {
        Drawn {
            luid: 0x1488a,
            fence: 512,
            texture: SharedTexture {
                handle: texture,
                set,
            },
            fence_value: texture * 2,
            width,
            height,
        }
    }

    /// Publishes a picture the way the worker does: into the slot the reader
    /// has moved on from, which is what [`Writer::claim`] answers.
    fn publish(writer: &mut Writer, drawn: &Drawn) -> bool {
        if writer.claim().is_none() {
            return false;
        }
        writer.publish_into(drawn)
    }

    /// The picture a slot describes, as a reader reads it, and nothing when
    /// it holds pixels.
    fn picture(frames: &Frames) -> Option<Picture> {
        frames
            .take_frame(|frame| match frame {
                Frame::Gpu(picture) => Some(picture),
                Frame::Pixels { width, height, .. } => {
                    assert!(width > 0 && height > 0, "a frame says its size");
                    None
                }
                // A DMA-BUF is the Linux picture path, and a test that looks
                // for a shared texture's handle never publishes one.
                #[cfg(target_os = "linux")]
                Frame::Dmabuf(_) => None,
            })
            .flatten()
    }

    /// A slot holds pixels or a picture the worker drew on the GPU, and a
    /// reader is told which: the picture's description comes back whole, the
    /// pixel path after it is the frame the worker wrote rather than the
    /// picture read as bytes, and a reader that only shows pixels passes the
    /// picture over instead of taking its slot for one.
    #[test]
    fn a_slot_holds_pixels_or_a_picture_and_says_which() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        assert!(
            publish(&mut writer, &drawn(9, 4, 2)),
            "nothing said against it"
        );
        assert_eq!(
            picture(&frames),
            Some(Picture {
                luid: 0x1488a,
                fence: 512,
                texture: 9,
                fence_value: 18,
                generation: 1,
                set: 1,
                slot: 0,
                width: 4,
                height: 2,
            })
        );
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "a picture is not pixels"
        );

        publish(&mut writer, &drawn(11, 4, 2));
        assert_eq!(picture(&frames).map(|picture| picture.texture), Some(11));

        writer.write(4, 2, |pixels| pixels.fill(4));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![4; 32])),
            "the pixels after a picture are pixels"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The UI decides once per stream whether it can show the worker's
    /// pictures, and says so in the header: a worker drawing for an adapter
    /// the UI doesn't draw on, or for a UI that refused them, writes the
    /// frame's pixels rather than losing the frame.
    #[test]
    fn a_picture_the_ui_cannot_open_comes_out_as_pixels() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // The UI draws on the worker's adapter, and says so.
        assert!(frames.accept_gpu(0x1488a));
        assert!(!frames.accept_gpu(0x1488a), "said once");
        assert!(publish(&mut writer, &drawn(9, 4, 2)), "the same adapter");
        assert_eq!(picture(&frames).map(|picture| picture.luid), Some(0x1488a));

        // It draws on another one, and the worker stops drawing for it.
        assert!(frames.accept_gpu(0x2079f));
        assert!(
            !publish(&mut writer, &drawn(11, 4, 2)),
            "another adapter is refused before anything is opened"
        );
        writer.write(4, 2, |pixels| pixels.fill(5));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![5; 32])
        );

        // And once it has refused them outright, no picture is published.
        assert!(frames.refuse_gpu());
        assert!(!frames.refuse_gpu(), "refused once, logged once");
        assert!(
            !publish(&mut writer, &drawn(13, 4, 2)),
            "the UI can't show it"
        );
        writer.write(4, 2, |pixels| pixels.fill(6));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![6; 32])
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The UI's answer on DMA-BUF pictures is the word it writes for a shared
    /// texture, because it is the same answer, and the worker asks it before it
    /// exports anything at all: a window that hasn't looked says nothing, one
    /// that can import a picture names the device it shows it on, and one that
    /// refused them is not drawn for.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_window_says_whether_it_can_show_dmabuf_pictures() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = socketed(&frames, &path);
        frames.unlink();

        assert_eq!(writer.dmabuf_showable(), None, "the window hasn't looked");
        assert!(
            frames.accept_dmabuf(0x55aa55aa),
            "it can show them, on its own device"
        );
        assert_eq!(writer.dmabuf_showable(), Some(0x55aa55aa));
        assert!(!frames.accept_dmabuf(0x55aa55aa), "said once");
        assert_eq!(
            writer.presented_on(),
            Some(0x55aa55aa),
            "the same answer a share reads"
        );

        assert!(frames.refuse_gpu());
        assert_eq!(
            writer.dmabuf_showable(),
            None,
            "a window that refused them isn't asked for them"
        );
        let name = "yapfast-test-frames-dmabuf-refused";
        let memory = memory(name);
        let fds = [memory.as_raw_fd()];
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 16,
        }];
        let drawn = described((4, 2), &planes, &fds);
        assert_eq!(writer.claim(), Some(0), "an empty slot");
        assert!(
            !writer.publish_dmabuf(&drawn),
            "a refused picture isn't sent"
        );
        assert_eq!(naming(name), 1, "and its descriptors stay the worker's");
        drop(memory);

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A GPU slot is drawn into again only once the reader says it has
    /// finished with the picture that is there, slot by slot and generation by
    /// generation. Taking a picture is not finishing with it: the window shows
    /// it until the next one replaces it, and the GPU may still be sampling
    /// its texture after that, so the reader's own word is what frees the slot.
    /// A window that falls behind costs preview frames and never a torn
    /// picture.
    #[test]
    fn a_slot_is_drawn_into_again_only_once_the_reader_finished_with_it() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // The file has three slots, and none has held a picture yet: the worker
        // takes its own slot back each time, and the reader is handed the newest.
        for texture in 9..12 {
            let slot = writer.claim().expect("a slot that never held a picture");
            assert_eq!(slot, writer.back);
            assert!(writer.publish_into(&drawn(texture, 4, 2)));
        }
        // The pictures the reader never took are ones it will never show, so the
        // worker's own slot is still its own. The picture the reader did take is
        // not, until the reader says its queue has finished with it.
        let newest = picture(&frames).expect("the newest picture");
        assert_eq!((newest.slot, newest.generation), (0, 3));
        let slot = writer.claim().expect("a slot the reader never took");
        assert_ne!(slot, newest.slot, "not the picture on the screen");
        assert!(writer.publish_into(&drawn(12, 4, 2)));

        // Finishing with a generation a slot doesn't hold says nothing about
        // the picture that is there, and a slot finished with twice frees
        // nothing more.
        assert!(
            !frames.retire(newest.slot, newest.generation - 1),
            "not the picture the slot holds"
        );
        assert!(frames.retire(newest.slot, newest.generation));
        assert!(!frames.retire(newest.slot, newest.generation), "once");

        // The worker draws its next picture somewhere of its own, and that is
        // the one the reader is handed.
        let _ = writer.claim().expect("a slot the writer may draw in");
        assert!(writer.publish_into(&drawn(13, 4, 2)));
        let newest = picture(&frames).expect("the picture just published");
        assert!(newest.generation > 3);
        assert!(frames.retire(newest.slot, newest.generation));

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture says which set of the worker's textures and fence it was drawn
    /// in, and that number only grows, slot by slot. A handle's value can be
    /// handed out again once the handle that had it is closed, so a reader that
    /// keys what it opened by the handle alone can't tell the set it has from the
    /// one after it; the number is what tells them apart.
    #[test]
    fn a_published_set_number_only_grows() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        publish(&mut writer, &drawn_in(7, 9, 4, 2));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((7, 9))
        );
        publish(&mut writer, &drawn_in(7, 11, 4, 2));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((7, 11)),
            "the same set, another of its three textures"
        );

        // A set the worker built after that one, and the handle of a texture of
        // the set before it: only the number says the two are different.
        publish(&mut writer, &drawn_in(8, 9, 4, 2));
        let next = picture(&frames).expect("the picture of the next set");
        assert_eq!((next.set, next.texture), (8, 9));

        // A set numbered before the one the picture holds is a worker that
        // went backwards, and nothing of it is published: the reader would take
        // its handle for one of the set it has, and a picture is not written
        // where the number it is known by would go back.
        assert!(frames.retire(0, 1), "the first picture is off the screen");
        let slot = writer.claim().expect("a slot the reader has finished with");
        assert!(
            !writer.publish_into(&drawn_in(6, 13, 4, 2)),
            "an older set was published"
        );
        assert_eq!(writer.claim(), Some(slot), "nothing was written there");
        assert!(writer.publish_into(&drawn_in(9, 13, 4, 2)));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((9, 13)),
            "the slot holds the newer set's picture"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The worker is never held up by the one slot it would have drawn in next:
    /// a window showing the newest picture keeps that slot, and the slot the
    /// reader never took is free to draw in instead. A rotation the worker
    /// couldn't turn would freeze the preview for the rest of the share.
    #[test]
    fn a_slot_the_reader_keeps_does_not_hold_up_the_worker() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        publish(&mut writer, &drawn(9, 4, 2));
        publish(&mut writer, &drawn(10, 4, 2));
        // The reader takes the newest and goes on showing it: the slot the
        // worker would draw in next is one it hasn't finished with.
        let newest = picture(&frames).expect("the newest picture");
        assert_eq!((newest.slot, newest.generation), (1, 2));
        let slot = writer
            .claim()
            .expect("the slot the reader never took is free");
        assert_ne!(slot, newest.slot);
        assert!(writer.publish_into(&drawn(11, 4, 2)));

        // The reader finishing with what it showed frees that slot in turn.
        assert!(frames.retire(newest.slot, newest.generation));
        assert!(writer.claim().is_some(), "a slot is free again");
    }

    /// A picture carries the size it was drawn at and a number that only
    /// grows, so a reader that shows it notices the worker drawing at another
    /// size and knows which picture it has. A stream that draws on the GPU and
    /// then writes pixels changes kind and size in one frame, and the reader
    /// is told that too.
    #[test]
    fn a_picture_says_its_size_and_number() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        publish(&mut writer, &drawn(9, 640, 360));
        assert_eq!(
            picture(&frames).map(|picture| (picture.width, picture.height, picture.generation)),
            Some((640, 360, 1))
        );
        publish(&mut writer, &drawn(9, 1280, 720));
        assert_eq!(
            picture(&frames).map(|picture| (picture.width, picture.height, picture.generation)),
            Some((1280, 720, 2)),
            "the same texture, drawn at another size"
        );

        writer.write(2, 1, |pixels| pixels.fill(7));
        assert_eq!(
            frames
                .take_frame(|frame| match frame {
                    Frame::Pixels {
                        width,
                        height,
                        pixels,
                    } => Some((width, height, pixels.to_vec())),
                    Frame::Gpu(_) => None,
                    #[cfg(target_os = "linux")]
                    Frame::Dmabuf(_) => None,
                })
                .flatten(),
            Some((2, 1, vec![7; 8])),
            "pixels again, at the pixel size"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The three slots' roles as the two sides hold them: the writer's back,
    /// the middle the reader is handed, and the reader's front.
    fn roles(writer: &Writer, frames: &Frames) -> (u32, u32, u32) {
        let middle = writer.mapping.header().middle.load(Ordering::Acquire) & !FRESH;
        let front = frames
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .slot;
        (writer.back, middle, front)
    }

    /// Back, middle and front are three different slots: the writer fills one,
    /// the reader is handed another, and reads a third. Two roles in one slot is
    /// the worker painting over the frame that is being read, or the ring
    /// collapsing into a double buffer.
    fn assert_three(writer: &Writer, frames: &Frames, when: &str) {
        let (back, middle, front) = roles(writer, frames);
        assert!(
            back != middle && middle != front && back != front,
            "{when}: the back is {back}, the middle is {middle} and the front is {front}"
        );
    }

    /// Publishing more pictures than the reader takes must never leave two roles
    /// in one slot, and it must not collapse the ring into two slots: three
    /// publishes before the first take used to put the middle in the reader's
    /// slot, after which the worker filled the frame the reader was reading.
    #[test]
    fn the_three_slots_are_never_two() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        assert_three(&writer, &frames, "a fresh file");

        // More pictures than there are slots, and not a take among them: the
        // worker takes its own slot back each time.
        for texture in 9..13 {
            writer.claim().expect("a slot the writer may draw in");
            assert!(writer.publish_into(&drawn(texture, 4, 2)));
            assert_three(&writer, &frames, "after a picture before a take");
        }

        // Then claims, takes and publishes in the order that used to tear: a
        // claim, the reader's take, the publish into the claimed slot.
        let newest = picture(&frames).expect("the newest picture");
        assert_three(&writer, &frames, "after the first take");
        // A picture for the middle, so the reader has one to take between the
        // claim and the publish.
        let _ = writer.claim().expect("a slot the writer may draw in");
        assert!(writer.publish_into(&drawn(13, 4, 2)));
        let slot = writer
            .claim()
            .expect("the next slot the writer may draw in");
        assert_ne!(slot, newest.slot, "not the picture on the screen");
        let taken = picture(&frames).expect("the picture the middle held");
        assert_ne!(slot, taken.slot, "the middle moved away from the slot");
        assert_three(&writer, &frames, "after a take between claim and publish");
        assert!(writer.publish_into(&drawn(14, 4, 2)));
        assert_three(&writer, &frames, "after the publish");
        // The picture just published is the one the reader is handed next, in
        // the slot the writer drew it in.
        let next = picture(&frames).expect("the picture just published");
        assert_eq!((next.slot, next.texture), (slot, 14));

        // The pixel path rotates the same three slots the same way.
        for value in 1..4 {
            writer.write(4, 2, |pixels| pixels.fill(value));
            assert_three(&writer, &frames, "after pixels");
            assert!(frames.take(|_, _, _| ()).is_some(), "the pixels are read");
            assert_three(&writer, &frames, "after the pixels are read");
        }

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture the reader took stays the worker's only once the reader says its
    /// rendering queue has passed it. A newer take of another slot does not say
    /// that: the window may still be sampling the older picture's texture, and
    /// drawing there would put a picture nobody finished with behind the one on
    /// the screen.
    #[test]
    fn a_picture_the_reader_took_is_not_freed_by_a_newer_take() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // Two pictures taken one after the other: the first is on the screen
        // until the window's queue has drawn the second.
        publish(&mut writer, &drawn(9, 4, 2));
        let first = picture(&frames).expect("the first picture");
        publish(&mut writer, &drawn(10, 4, 2));
        let second = picture(&frames).expect("the second picture");
        assert_eq!((first.slot, first.generation), (0, 1));
        assert_eq!((second.slot, second.generation), (1, 2));

        // The writer's own slot comes round to the picture it took first, which
        // nothing has retired: it is the window's until the reader says so, and
        // the newer take of the other slot says nothing about it.
        publish(&mut writer, &drawn(11, 4, 2));
        assert_eq!(
            writer.claim(),
            None,
            "the picture the window took is still the window's"
        );
        assert!(frames.retire(first.slot, first.generation));
        assert_eq!(writer.claim(), Some(first.slot), "finished with, so free");

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A file whose room the worker couldn't give back because the shortened file
    /// wouldn't map keeps its room and its slots: no room given back rather than
    /// no frames, and never a view that isn't there to write through.
    #[test]
    fn a_shortening_that_cannot_be_mapped_again_keeps_its_slots() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        let grown = writer.file.metadata().unwrap().len();
        assert_eq!(grown, HEADER as u64 + SLOTS as u64 * 64 * 48 * 4);
        assert!(frames.take(|_, _, _| ()).is_some());

        // The shortened file can't be mapped again once: the room goes back on
        // the file and it is mapped where it was.
        writer.mapping.fail_remap = 1;
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert_eq!(
            writer.file.metadata().unwrap().len(),
            grown,
            "the room stayed"
        );
        assert!(writer.fits(64, 48), "the slots are still there");

        // The frame after it is written and read where it was written.
        writer.write(8, 4, |pixels| pixels.fill(2));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((8, 4, vec![2; 128]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A file that can't be mapped again even at the room it had is mapped by the
    /// frame after it: the writer skips a frame rather than panicking, and the
    /// slots it gets are the ones the file really holds.
    #[test]
    fn a_file_that_cannot_be_mapped_is_mapped_by_the_frame_after() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        assert!(frames.take(|_, _, _| ()).is_some());

        // Neither the shortened file nor the file it was lengthened back to can
        // be mapped: no view is left, and the frame after asks for one.
        reader_lets_go(&mut writer, &frames);
        writer.mapping.fail_remap = 2;
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert!(writer.mapping.map.is_none(), "no view is left");

        writer.write(4, 2, |pixels| pixels.fill(3));
        assert!(writer.mapping.map.is_some(), "the frame after mapped it");
        assert!(frames.take(|_, _, pixels| pixels.len()).is_some());

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The card path reads the header before a frame is written, so a file a
    /// shortening left without a view is one it can't read: a claim and a
    /// publish there answer rather than panic, and a file that can be mapped
    /// again has a slot from the claim after it. A remap the file system refuses
    /// costs pictures, never the stream's thread.
    #[test]
    fn a_card_picture_survives_a_file_that_cannot_be_mapped() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        assert!(frames.take(|_, _, _| ()).is_some());

        // Neither the shortened file nor the file put back can be mapped, so no
        // view is left — and the card path is what runs before the pixel one.
        reader_lets_go(&mut writer, &frames);
        writer.mapping.fail_remap = 2;
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert!(writer.mapping.map.is_none(), "no view is left");

        // The claim answers "no slot", which is what the worker skips a picture
        // for, and the rest of the card path refuses the same way. Each step
        // asks for a view of its own, so each is given a refusal of its own.
        for _ in 0..2 {
            writer.mapping.fail_remap = 1;
            assert_eq!(writer.claim(), None, "no file to draw in");
            assert!(!writer.given_up(), "a file, not a window, is the reason");
        }
        writer.mapping.fail_remap = 1;
        assert!(!writer.publish_into(&drawn(9, 4, 2)), "nowhere to publish");
        writer.mapping.fail_remap = 1;
        assert_eq!(writer.presented_on(), None, "nothing to say about a slot");
        writer.mapping.fail_remap = 1;
        assert_eq!(writer.ui_process(), 0, "no file, no process to ask");

        // The claim after it maps the file again, and the card path carries on.
        assert!(writer.claim().is_some(), "mapped by the claim after it");
        assert!(writer.publish_into(&drawn(9, 4, 2)));
        assert_eq!(picture(&frames).map(|picture| picture.texture), Some(9));

        // And the pixel path after a picture is read where it was written.
        writer.write(4, 2, |pixels| pixels.fill(3));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![3; 32]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A slot the reader took and never retired is the window's, and a worker
    /// that answered "no slot" for it forever would stop the ring turning: the
    /// preview would freeze on the last picture it took. Past the bound the
    /// writer gives the picture path up instead, and the frames after it are
    /// pixels, which touch no texture and reach the window as ever.
    #[test]
    fn a_slot_nothing_lets_go_comes_back_as_pixels() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        // Nothing held is given a moment in a test; a stream gives it a second.
        writer.held_for = Duration::ZERO;

        // Three pictures, each taken and never retired: the window shows them
        // and never says it is done with them.
        for texture in 9..12 {
            publish(&mut writer, &drawn(texture, 4, 2));
            assert!(picture(&frames).is_some(), "taken, {texture}");
        }
        // The slot the writer would draw in next is one of them: it says so
        // until the bound is up, and then it stops asking.
        assert_eq!(writer.claim(), None, "the picture is still the window's");
        assert!(writer.given_up(), "and nothing is letting it go");
        assert_eq!(writer.claim(), None, "the picture path is given up");

        // The pixel path doesn't wait for the window: the ring turns and the
        // frame is read, so a share whose window stopped talking goes on
        // showing the screen.
        writer.write(4, 2, |pixels| pixels.fill(7));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![7; 32]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A descriptor of this process's own naming `name`, which is the "picture"
    /// these tests share: nothing about it is a card's, and its name is what
    /// says how many of this process's descriptors name it.
    #[cfg(target_os = "linux")]
    use std::os::fd::AsRawFd;

    #[cfg(target_os = "linux")]
    fn memory(name: &str) -> std::os::fd::OwnedFd {
        use std::os::fd::{FromRawFd, OwnedFd};
        let name = std::ffi::CString::new(name).unwrap();
        // SAFETY: the name is this test's own string, and nothing is asked of
        // the descriptor but that it be a fresh anonymous one.
        let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
        assert!(fd >= 0, "a memfd for the test");
        // SAFETY: the call made the descriptor, and nothing else holds it.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    /// How many of this process's descriptors name the memfd `name`: one more
    /// for each end a picture has crossed to, and one fewer for each end that
    /// has closed it. It is what says a descriptor was handed over rather than
    /// copied, and closed rather than leaked.
    #[cfg(target_os = "linux")]
    fn naming(name: &str) -> usize {
        let Ok(entries) = std::fs::read_dir("/proc/self/fd") else {
            panic!("the open descriptors are listed");
        };
        entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                std::fs::read_link(PathBuf::from("/proc/self/fd").join(entry.file_name()))
                    .is_ok_and(|target| target.to_string_lossy().contains(name))
            })
            .count()
    }

    /// A stream whose two ends are both in this test, which a spawn makes two
    /// processes: the window's end in the frames, the worker's in the writer.
    #[cfg(target_os = "linux")]
    fn socketed(frames: &Frames, path: &Path) -> Writer {
        let (reader, sender) = dmabuf::Reader::paired();
        let mut front = frames
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        front.slot = 2;
        front.pictures = reader;
        drop(front);
        let mut writer = Writer::open(path).unwrap();
        writer.pictures = sender;
        writer
    }

    /// A picture on the Linux path, at the size it is named with.
    #[cfg(target_os = "linux")]
    fn described<'a>(
        (width, height): (u32, u32),
        planes: &'a [dmabuf::Plane],
        fds: &'a [std::os::fd::RawFd],
    ) -> dmabuf::Descriptor<'a> {
        dmabuf::Descriptor {
            width,
            height,
            fourcc: u32::from_le_bytes(*b"XR24"),
            modifier: 0,
            planes,
            colour: dmabuf::Colour::SDR,
            fds,
        }
    }

    /// A picture's descriptor is what crosses on Linux, not a handle in the file:
    /// it goes over the stream's socket, the window takes it with the picture it
    /// names, and it is closed exactly once on each side — the window's as it
    /// retires the slot, the worker's when it lets go of its own.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_picture_crosses_as_descriptors_and_is_closed_once_on_each_side() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = socketed(&frames, &path);
        frames.unlink();

        let name = "yapfast-test-frames-picture";
        let memory = memory(name);
        assert_eq!(naming(name), 1, "the worker's own");
        let fds = [memory.as_raw_fd()];
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 16,
        }];
        let drawn = described((4, 2), &planes, &fds);
        assert_eq!(writer.claim(), Some(0), "a slot that never held a picture");
        assert!(writer.publish_dmabuf(&drawn), "one message, one picture");
        assert_eq!(naming(name), 1, "the sender keeps its own descriptor");

        let picture = frames
            .take_frame(|frame| match frame {
                Frame::Dmabuf(picture) => Some(picture),
                Frame::Pixels { .. } | Frame::Gpu(_) => None,
            })
            .flatten()
            .expect("the picture the worker published");
        assert_eq!((picture.width(), picture.height()), (4, 2));
        assert_eq!(
            (picture.fourcc(), picture.modifier()),
            (u32::from_le_bytes(*b"XR24"), 0)
        );
        let held: Vec<_> = picture.planes().collect();
        assert_eq!(held.len(), 1, "one plane, one descriptor");
        assert_eq!(
            held[0].0,
            dmabuf::Plane {
                offset: 0,
                stride: 16
            }
        );
        assert_eq!(naming(name), 2, "the window has its own now");
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "a picture is not pixels"
        );

        // Retiring the slot and letting the picture go is one close, and the
        // worker's own descriptor is still the worker's.
        let (slot, generation) = (picture.slot(), picture.generation());
        assert!(frames.retire(slot, generation), "off the screen");
        drop(picture);
        assert_eq!(naming(name), 1, "the window closed its own");
        drop(memory);
        assert_eq!(naming(name), 0, "and the worker its own");

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The slot protocol is the same as a shared texture's: a picture the window
    /// took is the worker's again only once the window says it has finished with
    /// it, and then the slot is published into again — the same slot, a newer
    /// picture of it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_retired_picture_slot_is_published_into_again() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = socketed(&frames, &path);
        frames.unlink();
        let take = |frames: &Frames| {
            frames
                .take_frame(|frame| match frame {
                    Frame::Dmabuf(picture) => Some(picture),
                    Frame::Pixels { .. } | Frame::Gpu(_) => None,
                })
                .flatten()
                .expect("the picture published")
        };

        let name = "yapfast-test-frames-slot";
        let memory = memory(name);
        let fds = [memory.as_raw_fd()];
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 16,
        }];
        let drawn = described((4, 2), &planes, &fds);

        // One picture a slot, each taken and kept: the window holds one memory
        // beside the worker's own.
        assert_eq!(writer.claim(), Some(0), "a slot that never held a picture");
        assert!(writer.publish_dmabuf(&drawn));
        let first = take(&frames);
        assert_eq!(writer.claim(), Some(1), "the next slot");
        assert!(writer.publish_dmabuf(&drawn));
        let second = take(&frames);
        assert_eq!(writer.claim(), Some(2), "the third slot");
        assert!(writer.publish_dmabuf(&drawn));
        let third = take(&frames);
        assert_eq!(
            (first.slot(), second.slot(), third.slot()),
            (0, 1, 2),
            "a picture a slot"
        );
        assert_eq!(naming(name), 4, "the worker's own and the window's three");

        // The writer's back has come round to the picture the window took first,
        // which the window hasn't finished with: nothing is drawn there, and the
        // frame's pixels are what the caller publishes instead.
        assert_eq!(
            writer.claim(),
            None,
            "the window may still be sampling that picture"
        );

        // Finished with, the slot is the writer's again, and the picture after
        // it goes into the slot it came from.
        assert!(
            frames.retire(first.slot(), first.generation()),
            "the window is done with the first picture"
        );
        assert_eq!(writer.claim(), Some(first.slot()), "the slot came back");
        assert!(writer.publish_dmabuf(&drawn));
        let again = take(&frames);
        assert_eq!(again.slot(), first.slot(), "the same slot");
        assert!(
            again.generation() > first.generation(),
            "a newer picture of it"
        );

        // Letting go of each picture closes the window's own descriptor of it,
        // and the worker's is the worker's until the worker lets go.
        drop((first, second, third, again));
        assert_eq!(naming(name), 1);
        drop(memory);
        assert_eq!(naming(name), 0);

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A socket whose window's end has closed is no channel at all: the picture
    /// isn't published, the frame goes out as pixels, and the stream carries on.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_picture_a_dead_socket_cannot_carry_comes_out_as_pixels() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = socketed(&frames, &path);
        frames.unlink();
        // The window that made the socket has gone, which is the socket closed
        // at its end.
        writer.pictures = {
            let (reader, sender) = dmabuf::Reader::paired();
            drop(reader);
            sender
        };

        let name = "yapfast-test-frames-dead-socket";
        let memory = memory(name);
        let fds = [memory.as_raw_fd()];
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 16,
        }];
        let drawn = described((4, 2), &planes, &fds);
        assert_eq!(writer.claim(), Some(0));
        assert!(!writer.publish_dmabuf(&drawn), "nowhere to send it");
        assert!(
            !writer.publish_dmabuf(&drawn),
            "and it isn't tried again at frame rate"
        );
        assert_eq!(naming(name), 1, "a send that failed takes no reference");

        // The frame after it is the frame's pixels, which need no socket.
        writer.write(4, 2, |pixels| pixels.fill(9));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![9; 32]))
        );
        drop(memory);
        assert_eq!(naming(name), 0);

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A frames file says which version of the header it was made with, and a
    /// reader of another version refuses it rather than reading its fields at
    /// another meaning; a file that isn't one of these at all is refused by its
    /// magic, as ever.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_of_another_version_or_magic_is_refused() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer
            .mapping
            .header()
            .version
            .store(VERSION + 1, Ordering::Release);
        assert!(
            Writer::open(&path).is_err(),
            "another version of the header"
        );
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "the window refuses it too"
        );

        writer
            .mapping
            .header()
            .version
            .store(VERSION, Ordering::Release);
        // SAFETY: the header is the first bytes of a view this test holds, page
        // aligned, and its magic is a plain number at the front of it.
        unsafe {
            writer
                .mapping
                .view()
                .as_ptr()
                .cast_mut()
                .cast::<u32>()
                .write(u32::from_le_bytes(*b"XXXX"));
        }
        assert!(Writer::open(&path).is_err(), "not a video frames file");
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "and not for the window"
        );

        drop((writer, frames));
    }
}

mod layout;
mod ring;
mod slots;
mod writer;

pub(crate) use layout::*;
pub(crate) use ring::*;
pub(crate) use slots::*;
pub(crate) use writer::*;

pub use ring::Frames;
pub use slots::Drawn;
pub use slots::Frame;
pub use slots::Picture;
pub use slots::SharedTexture;
pub use writer::Writer;
