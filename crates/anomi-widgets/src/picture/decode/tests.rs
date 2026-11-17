//! The runs over a moving picture's frames: ported from the tests of
//! `src/ui/pictures.rs` (`animations_decode_the_frame_showing_and_let_it_go_when_idle`,
//! `a_frame_on_its_way_leaves_the_last_one_showing`).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use iced::widget::image;

use super::*;
use crate::picture::moving::Frames;

/// `count` frames of 2 by 2 pixels, each a grey of its own.
fn greys(count: u8) -> Arc<dyn Moving> {
    let frames = (0..count).map(|index| vec![index * 5; 16]).collect();
    Arc::new(Frames::new(2, 2, frames, vec![50; count.into()], 0))
}

fn ten() -> Arc<dyn Moving> {
    greys(10)
}

fn plain() -> Look {
    Look {
        width: 2,
        height: 2,
        corners: [0; 4],
        cover: false,
    }
}

fn grey(frame: &image::Handle) -> u8 {
    match frame {
        image::Handle::Rgba { pixels, .. } => pixels[0],
        _ => panic!("frames are pixels"),
    }
}

/// An animation of [`ten`] as [`plain`] draws it, and its frames.
fn animated() -> (Animation, Arc<dyn Moving>) {
    let source = ten();
    (Animation::new(source.as_ref(), plain()), source)
}

/// [`animated`], streamed as a large sticker or GIF is: nothing kept.
fn streamed() -> (Animation, Arc<dyn Moving>) {
    let (mut animation, source) = animated();
    animation.kept.clear();
    (animation, source)
}

/// Asks for `row` until its frame comes, as the window asks at each frame.
fn show(animation: &mut Animation, source: &Arc<dyn Moving>, row: usize) {
    let started = Instant::now();
    loop {
        if animation
            .frame(row, Instant::now(), source)
            .is_some_and(|frame| grey(&frame) == row as u8 * 5)
        {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "frame {row}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Waits until the run has its next frame ready.
fn settle(animation: &Animation) {
    let started = Instant::now();
    while !animation
        .runs
        .iter()
        .any(|run| run.hand.lock().frame.is_some())
    {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The grey of frame `row` when it's due, its run having had the time.
fn step(animation: &mut Animation, source: &Arc<dyn Moving>, row: usize) -> u8 {
    settle(animation);
    grey(
        &animation
            .frame(row, Instant::now(), source)
            .expect("a frame"),
    )
}

#[test]
fn one_run_plays_round_and_round_on_time_and_another_place_gets_its_own() {
    let (mut animation, source) = streamed();
    assert!(
        animation.frame(0, Instant::now(), &source).is_none(),
        "the still"
    );
    for row in (1..10).chain(0..10) {
        assert_eq!(
            step(&mut animation, &source, row),
            row as u8 * 5,
            "frame {row}"
        );
    }
    assert_eq!(animation.runs(), 1);
    // Another place showing it at its own frame gets a run of its own.
    show(&mut animation, &source, 5);
    assert_eq!(animation.runs(), 2);
    assert!(
        animation.frame(10, Instant::now(), &source).is_none(),
        "no such frame"
    );
}

#[test]
fn while_the_still_shows_the_second_frame_is_made_ready() {
    let (mut animation, source) = animated();
    assert!(
        animation.frame(0, Instant::now(), &source).is_none(),
        "the still"
    );
    settle(&animation);
    // Due, it's there at once.
    let second = animation.frame(1, Instant::now(), &source).expect("ready");
    assert_eq!(grey(&second), 5);
    // And the third is made while the second shows.
    assert_eq!(step(&mut animation, &source, 2), 10);
}

#[test]
fn a_small_animation_is_kept_whole_after_one_round() {
    let (mut animation, source) = animated();
    assert!(
        animation.frame(0, Instant::now(), &source).is_none(),
        "the still"
    );
    for row in (1..10).chain([0]) {
        assert_eq!(
            step(&mut animation, &source, row),
            row as u8 * 5,
            "frame {row}"
        );
    }
    assert_eq!(animation.runs(), 0, "every frame kept: nothing decodes");
    for row in (0..10).chain(0..10) {
        let frame = animation.frame(row, Instant::now(), &source);
        assert_eq!(frame.map(|frame| grey(&frame)), Some(row as u8 * 5));
    }
    assert_eq!(animation.runs(), 0);
    assert_eq!(animation.bytes(), 10 * 16, "each frame counted once");
    // A large one streams: 10 frames of 256 pixels a side are 2.5 MiB.
    let large = Look {
        width: 256,
        height: 256,
        ..plain()
    };
    assert!(Animation::new(source.as_ref(), large).kept.is_empty());
}

#[test]
fn a_run_that_fell_behind_catches_up() {
    let (mut animation, source) = streamed();
    show(&mut animation, &source, 3);
    settle(&animation);
    // Frame 4 was ready, but 5 is due: 4 shows late once, then 6 is on
    // time, 5 passed over.
    assert_eq!(step(&mut animation, &source, 5), 20);
    assert_eq!(step(&mut animation, &source, 6), 30);
}

#[test]
fn idle_runs_end_and_the_last_frame_shows_until_the_next_comes() {
    let (mut animation, source) = streamed();
    animation.idle = Duration::from_millis(30);
    show(&mut animation, &source, 5);
    // Held still a while, the run lets go of its frames and ends.
    let started = Instant::now();
    while animation.runs() > 0 {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        animation.bytes(),
        16,
        "the frame last shown, nothing on its way"
    );
    // The frame held still is given again without a run.
    let held = animation.frame(5, Instant::now(), &source).expect("held");
    assert_eq!(grey(&held), 25);
    assert_eq!(animation.runs(), 0);
    // Going on starts another, and frame 5 shows until 6 comes, not the still.
    let next = animation
        .frame(6, Instant::now(), &source)
        .expect("the last frame");
    assert!([25, 30].contains(&grey(&next)));
    show(&mut animation, &source, 6);
}

#[test]
fn at_most_four_places_have_a_run() {
    let source = greys(40);
    let mut animation = Animation::new(source.as_ref(), plain());
    // Places far apart, each at its own frame: the fifth takes the place
    // of the run used longest ago.
    for row in [1, 10, 20, 30, 38] {
        animation.frame(row, Instant::now(), &source);
    }
    assert_eq!(animation.runs(), RUNS);
}

#[test]
fn letting_go_ends_the_runs() {
    let (mut animation, source) = animated();
    show(&mut animation, &source, 1);
    let hand = animation.runs[0].hand.clone();
    drop(animation);
    let started = Instant::now();
    while !hand.lock().ended {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(hand.lock().frame.is_none(), "no frame left behind");
}

#[test]
fn frames_come_at_the_size_and_shape_drawn() {
    let look = Look {
        width: 1,
        height: 1,
        corners: [0; 4],
        cover: false,
    };
    let source = ten();
    let mut animation = Animation::new(source.as_ref(), look);
    let started = Instant::now();
    let frame = loop {
        if let Some(frame) = animation.frame(2, Instant::now(), &source) {
            break frame;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    };
    match frame {
        image::Handle::Rgba { width, height, .. } => assert_eq!((width, height), (1, 1)),
        _ => panic!("frames are pixels"),
    }
}

/// Ten delays of which `frames()` gives the first `gives`, as a file that
/// won't open or a GIF corrupt part way, counting its calls.
struct Short {
    delays: Vec<u32>,
    gives: usize,
    calls: AtomicUsize,
}

impl Short {
    fn new(gives: usize) -> Arc<Self> {
        Arc::new(Self {
            delays: vec![50; 10],
            gives,
            calls: AtomicUsize::new(0),
        })
    }
}

impl Moving for Short {
    fn size(&self) -> (u32, u32) {
        (2, 2)
    }

    fn delays(&self) -> &[u32] {
        &self.delays
    }

    fn plays(&self) -> u32 {
        0
    }

    fn frames(&self) -> Box<dyn Iterator<Item = Vec<u8>>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::new((0..self.gives).map(|index| vec![index as u8 * 5; 16]))
    }
}

/// Waits until each run has its next frame ready or has ended, as for a
/// window whose frames come slower than they decode.
fn quiet(animation: &Animation) {
    let started = Instant::now();
    while !animation.runs.iter().all(|run| {
        let state = run.hand.lock();
        state.frame.is_some() || state.ended
    }) {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The grey of each of twenty frames `source` plays, kept or streamed;
/// none where the still shows.
fn twenty(source: &Arc<dyn Moving>, keep: bool) -> Vec<Option<u8>> {
    let mut animation = Animation::new(source.as_ref(), plain());
    if !keep {
        animation.kept.clear();
    }
    (0..20)
        .map(|at| {
            let frame = animation.frame(at % 10, Instant::now(), source);
            quiet(&animation);
            frame.map(|frame| grey(&frame))
        })
        .collect()
}

#[test]
fn a_source_that_runs_short_is_read_once_and_holds_its_last_frame() {
    for keep in [true, false] {
        // A file that won't open: the still throughout.
        let none = Short::new(0);
        let source: Arc<dyn Moving> = none.clone();
        assert_eq!(twenty(&source, keep), [None; 20], "kept: {keep}");
        assert_eq!(none.calls.load(Ordering::Relaxed), 1, "kept: {keep}");
        // Corrupt at its fourth frame: the still, two frames, and the
        // third held for the seven it lacks.
        let three = Short::new(3);
        let source: Arc<dyn Moving> = three.clone();
        let round = [None, Some(5), Some(10), Some(10), Some(10)]
            .into_iter()
            .chain([Some(10); 5]);
        let rounds: Vec<_> = round.clone().chain(round).collect();
        assert_eq!(twenty(&source, keep), rounds, "kept: {keep}");
        // Kept, it's read once; streamed, once a round, as a whole one is.
        let calls = three.calls.load(Ordering::Relaxed);
        assert_eq!(calls, if keep { 1 } else { 2 }, "kept: {keep}");
    }
}

#[test]
fn half_the_way_round_is_behind() {
    assert!(not_past(3, 3, 10));
    assert!(not_past(1, 3, 10));
    assert!(!not_past(4, 3, 10), "after it");
    assert!(not_past(9, 1, 10), "round the end");
    assert!(!not_past(0, 0, 0), "no frames");
}
