//! How much memory the machine has left, and what a stream gives up when it
//! runs short.
//!
//! Two things are sampled, and only where the system says them: the physical
//! memory the system could still give a process, and the budget the active
//! adapter's driver says it has left. Nothing here reads Rust's heap as a
//! stand-in for either: a stream's working set is textures, codec surfaces and
//! driver memory far more than it is allocations, and a number about the
//! processor's heap would say the machine is fine while the card is out.
//!
//! The policy is a state machine with no clock of its own, so it can be driven
//! by a test as well as by a loop: it enters its constrained mode when any known
//! signal is below [`ENTER_BELOW`] per cent, and leaves only once every known
//! signal has been above [`LEAVE_ABOVE`] for [`LEAVE_AFTER`]. A signal the
//! platform doesn't give is unknown, not zero — a machine with no DXGI budget to
//! ask for is not a machine out of video memory — and a signal that goes away
//! while waiting ends the wait.
//!
//! What constrained mode changes is only what a stream can spare: the local
//! preview's size, cached pictures, idle buffers. The stream that goes out keeps
//! its resolution, bitrate and codec, because a person watching would rather
//! have their video than the sharer's preview of it.

use std::time::{Duration, Instant};

/// The headroom, in per cent, below which a stream starts giving memory back.
pub const ENTER_BELOW: u32 = 10;

/// The headroom, in per cent, above which it may stop giving it back.
pub const LEAVE_ABOVE: u32 = 20;

/// How long every signal has to stay above [`LEAVE_ABOVE`] before the stream
/// stops giving memory back: one sample above the line is a page coming free,
/// ten seconds of them is memory that is really free again.
pub const LEAVE_AFTER: Duration = Duration::from_secs(10);

/// How often the machine is sampled: once a second, from the loop that has one,
/// so there is no thread to own or join.
pub const EVERY: Duration = Duration::from_secs(1);

/// A sample of what the machine had left. Every field is `None` when the
/// platform didn't say, and a total of zero is a platform that answered
/// nothing rather than a machine with nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Physical memory the system could give a process, in bytes.
    pub system_free: Option<u64>,
    /// Physical memory the machine has, in bytes.
    pub system_total: Option<u64>,
    /// What the adapter's driver says it can still hand out, in bytes.
    pub video_free: Option<u64>,
    /// What the adapter's driver says it may use, in bytes.
    pub video_budget: Option<u64>,
}

impl Reading {
    /// The system memory left, per cent, or `None` when the platform said
    /// nothing usable.
    pub fn system_headroom(&self) -> Option<u32> {
        headroom(self.system_free?, self.system_total?)
    }

    /// The adapter's budget left, per cent, or `None` when the platform said
    /// nothing usable: a driver that reports a budget of zero is one that
    /// doesn't report one.
    pub fn video_headroom(&self) -> Option<u32> {
        headroom(self.video_free?, self.video_budget?)
    }

    /// Both, whichever the platform knows: what the policy judges the machine
    /// by.
    fn headrooms(&self) -> impl Iterator<Item = (Signal, u32)> {
        [
            (Signal::SystemMemory, self.system_headroom()),
            (Signal::VideoMemory, self.video_headroom()),
        ]
        .into_iter()
        .filter_map(|(signal, headroom)| headroom.map(|headroom| (signal, headroom)))
    }
}

/// `free` as a per cent of `total`, or `None` when there is no total to compare
/// against.
fn headroom(free: u64, total: u64) -> Option<u32> {
    if total == 0 {
        return None;
    }
    Some((free.min(total).saturating_mul(100) / total) as u32)
}

/// Which of the two signals a reading judges the machine by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// The physical memory the system could still give a process.
    SystemMemory,
    /// The active adapter's video memory budget.
    VideoMemory,
}

/// Why a stream is giving memory back, as a report names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The system was short of physical memory.
    SystemMemory,
    /// The adapter was short of its budget.
    VideoMemory,
    /// Both were.
    Both,
}

impl Reason {
    /// What a report calls it.
    pub fn name(self) -> &'static str {
        match self {
            Self::SystemMemory => "the system's memory",
            Self::VideoMemory => "the card's budget",
            Self::Both => "the system's memory and the card's budget",
        }
    }

    /// Why `signals` had the stream give memory back.
    fn of(signals: &[Signal]) -> Self {
        let system = signals.contains(&Signal::SystemMemory);
        let video = signals.contains(&Signal::VideoMemory);
        match (system, video) {
            (true, true) => Self::Both,
            (false, true) => Self::VideoMemory,
            // A machine that answered with neither is not constrained at all.
            _ => Self::SystemMemory,
        }
    }
}

/// What the last sample did to the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Nothing changes.
    Same,
    /// The machine is short: a stream gives back what it can spare.
    Enter(Reason),
    /// The machine has had room for a while: a stream takes its room back.
    Leave,
}

/// Whether a stream is giving memory back, and since when it has had the room
/// to stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Why the stream is giving memory back, or `None` while it isn't.
    reason: Option<Reason>,
    /// Since when every known signal has been above [`LEAVE_ABOVE`], while the
    /// stream is constrained: a wait that one low sample ends.
    clearing: Option<Instant>,
}

impl Policy {
    /// Why the stream is giving memory back, or `None` while it isn't.
    pub fn constrained(&self) -> Option<Reason> {
        self.reason
    }

    /// Reads `reading`, taken at `now`, and says what it changes: entering at
    /// once, leaving only after [`LEAVE_AFTER`] of room. A machine that says
    /// nothing either way keeps the mode it has, since a policy that gave back
    /// memory on a sample it couldn't read would do so on every platform that
    /// can't be asked.
    pub fn update(&mut self, reading: Reading, now: Instant) -> Change {
        let headrooms: Vec<(Signal, u32)> = reading.headrooms().collect();
        if headrooms.is_empty() {
            // A signal that goes away while the wait to leave is running ends
            // it: the time the platform said nothing is not time it said the
            // machine had room, so the wait after it starts over.
            self.clearing = None;
            return Change::Same;
        }
        let short: Vec<Signal> = headrooms
            .iter()
            .filter(|(_, headroom)| *headroom < ENTER_BELOW)
            .map(|(signal, _)| *signal)
            .collect();
        if self.reason.is_some() {
            // Constrained: a low sample is the machine saying it is still
            // short, and the wait to stop starts over.
            if !short.is_empty() {
                self.clearing = None;
                return Change::Same;
            }
            let roomy = headrooms
                .iter()
                .all(|(_, headroom)| *headroom > LEAVE_ABOVE);
            if !roomy {
                self.clearing = None;
                return Change::Same;
            }
            let since = *self.clearing.get_or_insert(now);
            if now.saturating_duration_since(since) < LEAVE_AFTER {
                return Change::Same;
            }
            *self = Self::default();
            return Change::Leave;
        }
        if short.is_empty() {
            return Change::Same;
        }
        let reason = Reason::of(&short);
        self.reason = Some(reason);
        self.clearing = None;
        Change::Enter(reason)
    }
}

/// What the machine has left, sampled from the loop that owns one: a stream
/// makes one of these when it starts and drops it when it ends, so there is
/// nothing to detach or stop.
#[derive(Debug, Default)]
pub struct Sampler {
    /// When the machine was last read, so a loop that runs at 60 Hz reads it
    /// once a second like the policy expects.
    sampled: Option<Instant>,
    last: Reading,
}

impl Sampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The machine's reading, taken again when `now` is a sample later than the
    /// last one. Reading it is a system call and, on Windows, a DXGI question
    /// about the adapter, so it is asked once a second rather than once a frame.
    pub fn read(&mut self, now: Instant) -> Reading {
        if self
            .sampled
            .is_none_or(|sampled| now.saturating_duration_since(sampled) >= EVERY)
        {
            self.sampled = Some(now);
            self.last = platform();
        }
        self.last
    }
}

/// What the platform says about the machine's memory, or nothing it can't be
/// asked.
#[cfg(windows)]
fn platform() -> Reading {
    windows::read()
}

/// A platform that isn't asked keeps its policy where it was: there is no
/// sampler here, and a stream behaves as it did before one existed.
#[cfg(not(windows))]
fn platform() -> Reading {
    Reading::default()
}

#[cfg(windows)]
mod windows {
    use super::Reading;
    use windows::{
        Win32::{
            Graphics::Dxgi::{
                CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
                IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory1,
            },
            System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        },
        core::Interface,
    };

    /// The system's own answer about physical memory, and the active adapter's
    /// about its budget: both are what a stream's memory comes out of.
    pub(super) fn read() -> Reading {
        let system = system_memory();
        let (video_free, video_budget) = video_memory();
        Reading {
            system_free: system.map(|(free, _)| free),
            system_total: system.map(|(_, total)| total),
            video_free,
            video_budget,
        }
    }

    /// Physical memory the system could give a process, and how much it has.
    fn system_memory() -> Option<(u64, u64)> {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: the structure is the size it says it is, and the call only
        // writes into it.
        unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
        Some((status.ullAvailPhys, status.ullTotalPhys))
    }

    /// The adapter the chosen preference picks, which is the one the stream's
    /// device is made on, and what its driver says it has left of its budget.
    /// `None` when the machine has no adapter to ask, or the driver doesn't
    /// answer for one: unknown, not zero.
    fn video_memory() -> (Option<u64>, Option<u64>) {
        let Some(adapter) = chosen() else {
            return (None, None);
        };
        let Ok(budget) = adapter.cast::<IDXGIAdapter3>() else {
            // An adapter that isn't `IDXGIAdapter3` is one before Windows 10's
            // budget: it has no answer to give, which is not a machine with
            // nothing left.
            return (None, None);
        };
        let mut info = Default::default();
        // SAFETY: the segment group is one DXGI defines, and the call writes
        // into the structure it was given.
        if unsafe { budget.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info) }
            .is_err()
        {
            return (None, None);
        }
        let free = info.Budget.saturating_sub(info.CurrentUsage);
        (Some(free), Some(info.Budget))
    }

    /// The adapter [`crate::gpu::choose`] picks under the preference that's set,
    /// which is the one a stream's device is made on.
    fn chosen() -> Option<IDXGIAdapter1> {
        // SAFETY: DXGI is asked for its factory and then for each adapter by
        // index, and every adapter it hands back is released when it goes out
        // of scope.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
        let mut found: Vec<(crate::gpu::Adapter, IDXGIAdapter1)> = Vec::new();
        let mut index = 0;
        // Past the last adapter `EnumAdapters1` answers `DXGI_ERROR_NOT_FOUND`,
        // which is the end of the machine's list rather than a failure.
        while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
            index += 1;
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            found.push((
                crate::gpu::Adapter {
                    luid: crate::gpu::Luid {
                        low: desc.AdapterLuid.LowPart,
                        high: desc.AdapterLuid.HighPart,
                    },
                    vendor: desc.VendorId,
                    dedicated_memory: desc.DedicatedVideoMemory as u64,
                    shared_memory: desc.SharedSystemMemory as u64,
                    software: (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
                },
                adapter,
            ));
        }
        let chosen = crate::gpu::choose(crate::gpu::preference(), found.iter().map(|(a, _)| *a))?;
        found
            .into_iter()
            .find(|(facts, _)| facts.luid == chosen.luid)
            .map(|(_, adapter)| adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    /// A machine that said both of the things it can be asked.
    fn both(system: u32, video: u32) -> Reading {
        Reading {
            system_free: Some(GIB * u64::from(system) / 100),
            system_total: Some(GIB),
            video_free: Some(GIB * u64::from(video) / 100),
            video_budget: Some(GIB),
        }
    }

    fn at(seconds: u64) -> Instant {
        // Any base: the policy only ever compares instants it was given.
        Instant::now() + Duration::from_secs(seconds)
    }

    #[test]
    fn a_machine_with_room_is_left_alone() {
        let mut policy = Policy::default();
        assert_eq!(policy.constrained(), None);
        assert_eq!(policy.update(both(50, 50), at(0)), Change::Same);
        assert_eq!(policy.constrained(), None);
    }

    #[test]
    fn a_signal_below_the_line_constrains_and_says_which() {
        let mut policy = Policy::default();
        assert_eq!(
            policy.update(both(9, 90), at(0)),
            Change::Enter(Reason::SystemMemory)
        );
        assert_eq!(policy.constrained(), Some(Reason::SystemMemory));

        let mut policy = Policy::default();
        assert_eq!(
            policy.update(both(90, 4), at(0)),
            Change::Enter(Reason::VideoMemory)
        );

        let mut policy = Policy::default();
        assert_eq!(
            policy.update(both(9, 9), at(0)),
            Change::Enter(Reason::Both)
        );
    }

    #[test]
    fn one_signal_that_is_unknown_is_not_a_shortage_and_not_a_floor() {
        // A platform that never answers about the adapter: its budget is
        // unknown, and the system's memory is what decides.
        let system_only = Reading {
            system_free: Some(GIB / 2),
            system_total: Some(GIB),
            ..Default::default()
        };
        let mut policy = Policy::default();
        assert_eq!(
            policy.update(system_only, at(0)),
            Change::Same,
            "half the machine free and no budget to ask for"
        );
        let short = Reading {
            system_free: Some(GIB / 20),
            ..system_only
        };
        assert_eq!(
            policy.update(short, at(1)),
            Change::Enter(Reason::SystemMemory)
        );
        // And leaving it only needs the signal that is there to be roomy.
        assert_eq!(policy.update(system_only, at(2)), Change::Same);
        assert_eq!(policy.update(system_only, at(12)), Change::Leave);
    }

    #[test]
    fn a_platform_that_says_nothing_changes_nothing() {
        let mut policy = Policy::default();
        assert_eq!(policy.update(Reading::default(), at(0)), Change::Same);
        assert_eq!(
            policy.update(both(2, 2), at(1)),
            Change::Enter(Reason::Both)
        );
        // A sample it couldn't read neither keeps it constrained nor frees it:
        // the mode stands until a real reading says otherwise.
        assert_eq!(policy.update(Reading::default(), at(2)), Change::Same);
        assert_eq!(policy.update(Reading::default(), at(30)), Change::Same);
        assert_eq!(policy.constrained(), Some(Reason::Both));

        // The wait to leave counts samples that saw room, and a signal that goes
        // away ends it: the silence between them is not room the machine had, so
        // a roomy sample after a long silence starts the ten seconds again
        // rather than leaving at once.
        assert_eq!(policy.update(both(60, 60), at(31)), Change::Same);
        assert_eq!(policy.update(Reading::default(), at(32)), Change::Same);
        assert_eq!(
            policy.update(both(60, 60), at(40)),
            Change::Same,
            "the wait started again at forty"
        );
        assert_eq!(policy.update(both(60, 60), at(49)), Change::Same);
        assert_eq!(
            policy.update(both(60, 60), at(50)),
            Change::Leave,
            "ten seconds of room after the silence"
        );
    }

    #[test]
    fn a_budget_of_zero_is_a_driver_that_does_not_report_one() {
        let reading = Reading {
            system_free: Some(GIB),
            system_total: Some(GIB),
            video_free: Some(0),
            video_budget: Some(0),
        };
        assert_eq!(reading.video_headroom(), None);
        assert_eq!(reading.system_headroom(), Some(100));
        let mut policy = Policy::default();
        assert_eq!(policy.update(reading, at(0)), Change::Same);
    }

    #[test]
    fn leaving_takes_ten_seconds_of_room_without_a_break() {
        let mut policy = Policy::default();
        assert_eq!(
            policy.update(both(5, 5), at(0)),
            Change::Enter(Reason::Both)
        );
        // Above the line, but not above the leave line: the wait never starts.
        assert_eq!(policy.update(both(15, 15), at(1)), Change::Same);
        // Room, and the wait starts.
        assert_eq!(policy.update(both(60, 60), at(2)), Change::Same);
        assert_eq!(policy.update(both(60, 60), at(11)), Change::Same);
        // One sample back under the leave line starts the ten seconds again.
        assert_eq!(policy.update(both(15, 15), at(12)), Change::Same);
        assert_eq!(policy.update(both(60, 60), at(13)), Change::Same);
        assert_eq!(
            policy.update(both(60, 60), at(23)),
            Change::Leave,
            "ten seconds after the break"
        );
        assert_eq!(policy.constrained(), None);
    }

    #[test]
    fn the_sampler_reads_the_machine_once_a_second() {
        let mut sampler = Sampler::new();
        let first = sampler.read(at(0));
        // Whatever the platform said, the same second says it again rather than
        // asking again: a loop at 60 Hz reads the machine where the policy
        // expects it, once a second.
        assert_eq!(
            sampler.read(at(0) + Duration::from_millis(500)),
            first,
            "the same second is not asked twice"
        );
        // The next second is a reading of its own, and it is what the second
        // after it says too.
        let later = sampler.read(at(1));
        assert_eq!(
            sampler.read(at(1) + Duration::from_millis(10)),
            later,
            "a second's reading is kept until the next"
        );
    }
}
