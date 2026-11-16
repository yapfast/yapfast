//! What a change in who speaks costs on its way to the window: the call's
//! loop publishes its state, the chat worker takes it from the watch and
//! the application keeps it in place of the last, as `discord_voice` and
//! `chat` do each time someone starts or stops speaking. That cost mustn't
//! grow with the people in the call or the system's devices, which the
//! state shares rather than copies. See the numbers with
//!
//! ```sh
//! cargo test --profile fast -p yapfast-core --test voice_snapshot -- --nocapture
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

use tokio::sync::watch;
use yapfast_core::voice::{Device, Participant, Phase, State};

struct Counting;

// Counted for each thread, so that only the test's own allocations count.
thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    let _ = BYTES.try_with(|total| total.set(total.get() + bytes as u64));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations and bytes asked for on this thread while `f` runs.
fn counted(f: impl FnOnce()) -> (u64, u64) {
    let before = (ALLOCATIONS.get(), BYTES.get());
    f();
    (ALLOCATIONS.get() - before.0, BYTES.get() - before.1)
}

/// Someone in a Discord call, as `discord_voice` lists them.
fn person(index: usize) -> Participant {
    let id = (1_200_000_000_000_000_000u64 + index as u64 * 4_194_304).to_string();
    Participant {
        user: format!("Participant {index}"),
        person: id.clone(),
        local: index == 0,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming: false,
        video: None,
        avatar: Some(format!(
            "https://cdn.discordapp.com/avatars/{id}/a_0123456789abcdef0123456789abcdef.webp?size=128"
        )),
        id,
    }
}

/// A device as LiveKit lists PulseAudio's: by name.
fn device(kind: &str, index: usize) -> Device {
    let name = format!("{kind} {index} Analog Stereo, Family 17h/19h HD Audio Controller");
    Device {
        id: format!("name:{name}"),
        name,
    }
}

/// A connected call with `people` in it, on a system with `devices`
/// microphones and speakers.
fn call(people: usize, devices: usize) -> State {
    let participants: Vec<Participant> = (0..people).map(person).collect();
    let microphones: Vec<Device> = (0..devices).map(|n| device("Microphone", n)).collect();
    let speakers: Vec<Device> = (0..devices).map(|n| device("Speakers", n)).collect();
    let cameras: Vec<Device> = (0..2).map(|n| device("Camera", n)).collect();
    State {
        phase: Phase::Connected,
        room: Some("discord:1200000000000000000:1200000000004194304".into()),
        name: "General".into(),
        muted: false,
        participants: participants.into(),
        microphone: microphones.first().map(|device| device.id.clone()),
        speaker: speakers.first().map(|device| device.id.clone()),
        microphones: microphones.into(),
        speakers: speakers.into(),
        cameras: cameras.into(),
        ..State::default()
    }
}

/// What one change in who speaks costs, on average: allocations, bytes and
/// time, from the call's new list of who's there until the application
/// holds the state with it.
fn speaking(people: usize, devices: usize) -> (u64, u64, Duration) {
    const UPDATES: u32 = 2_000;
    let mut state = call(people, devices);
    let (snapshots, mut states) = watch::channel(state.clone());
    let mut shown = state.clone();
    let (mut allocations, mut bytes, mut time) = (0, 0, Duration::ZERO);
    for update in 0..UPDATES {
        // Who's there, as the call's loop makes it on each turn, with the
        // next one speaking, or no one; made outside what's measured.
        let speaker = update as usize % (people + 1);
        let participants: Vec<Participant> = (0..people)
            .map(|index| Participant {
                speaking: index == speaker,
                ..person(index)
            })
            .collect();
        let started = Instant::now();
        let (count, size) = counted(|| {
            // discord_voice: publishes the call's state when who's there changed.
            if participants[..] != state.participants[..] {
                state.participants = participants.into();
                snapshots.send_replace(state.clone());
            }
            // chat: hands the application the state the watch holds.
            let received = states.borrow_and_update().clone();
            // application: keeps it in place of the one it showed.
            drop(std::mem::replace(&mut shown, received));
        });
        time += started.elapsed();
        allocations += count;
        bytes += size;
    }
    // The application holds the call's latest state.
    let speaker = (UPDATES - 1) as usize % (people + 1);
    let speaking = |state: &State| -> Vec<bool> {
        state
            .participants
            .iter()
            .map(|person| person.speaking)
            .collect()
    };
    let last: Vec<bool> = (0..people).map(|index| index == speaker).collect();
    assert_eq!(speaking(&shown), last);
    assert_eq!(speaking(&state), last);
    (
        allocations / u64::from(UPDATES),
        bytes / u64::from(UPDATES),
        time / UPDATES,
    )
}

#[test]
fn a_change_in_who_speaks_copies_neither_the_people_nor_the_devices() {
    let alone = speaking(1, 1);
    let typical = speaking(5, 8);
    let crowded = speaking(25, 12);
    for ((people, devices), (allocations, bytes, time)) in
        [((1, 1), alone), ((5, 8), typical), ((25, 12), crowded)]
    {
        println!(
            "{people:>2} in the call, {devices:>2} microphones and as many speakers: \
             {allocations} allocations, {bytes} bytes, {time:?} per change"
        );
    }
    assert_eq!(
        alone.0, crowded.0,
        "a change in who speaks allocates more with more people and devices"
    );
    assert_eq!(alone.0, typical.0);
}
