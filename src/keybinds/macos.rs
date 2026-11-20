//! Global keybinds on macOS: a listen-only Quartz event tap on a thread of
//! its own, with its own run loop. It sees every key going down or up and
//! every modifier changing, in every app, and passes them on untouched.
//!
//! A listen-only tap needs the Input Monitoring permission. Without it the
//! thread asks macOS once, then looks again every [`RETRY_EVERY`], so the
//! keybinds start working once the user has allowed Yapfast, and the
//! Keybinds page says in the meantime where to allow it.
//!
//! Key codes are macOS's virtual key codes, which is also what winit gives
//! as a key's scan code on macOS, so the keys settings recorded are the
//! keys the tap hears. [`key_change`] maps an event to such a code.

use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes,
    kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGPreflightListenEventAccess, CGRequestListenEventAccess,
};

use super::{Matcher, Reach, Report, Sink, Status};

/// Shown while Input Monitoring isn't allowed, which the page's note goes
/// on from.
const NOT_ALLOWED: &str = "macOS hasn't allowed Yapfast to watch keys in other apps: allow it under System Settings → Privacy & Security → Input Monitoring";
/// Shown when macOS allows it but still gives no tap.
const NO_TAP: &str = "macOS didn't let Yapfast watch keys in other apps";

/// How often a thread without a tap looks again, in case the user has
/// allowed Yapfast since.
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// How often a waiting thread looks whether it should end.
const STOP_POLL: Duration = Duration::from_millis(200);
/// How long the run loop runs before the thread checks the tap and whether
/// it should end, in seconds. A stop that comes first wakes it sooner.
const RUN_SLICE_SECONDS: f64 = 1.0;
/// The order of the tap's run loop source among the loop's others.
const SOURCE_ORDER: isize = 0;

/// What the tap hears: key down, key up, and modifiers changing, as the
/// bits `CGEventMaskBit` sets for those event types.
const EVENT_MASK: u64 = (1 << CGEventType::KeyDown.0)
    | (1 << CGEventType::KeyUp.0)
    | (1 << CGEventType::FlagsChanged.0);

/// The highest virtual key code macOS has; anything above isn't a key.
const HIGHEST_KEY_CODE: i64 = 0xFF;

/// The event flags macOS keeps for each modifier: its own bit, and the bits
/// of each side that IOKit adds in the low bits, as the keyboard sets them.
const SHIFT_FLAG: u64 = 0x0002_0000;
const CONTROL_FLAG: u64 = 0x0004_0000;
const ALTERNATE_FLAG: u64 = 0x0008_0000;
const COMMAND_FLAG: u64 = 0x0010_0000;
const CAPS_LOCK_FLAG: u64 = 0x0001_0000;
const FUNCTION_FLAG: u64 = 0x0080_0000;
const LEFT_CONTROL_BIT: u64 = 0x0000_0001;
const LEFT_SHIFT_BIT: u64 = 0x0000_0002;
const RIGHT_SHIFT_BIT: u64 = 0x0000_0004;
const LEFT_COMMAND_BIT: u64 = 0x0000_0008;
const RIGHT_COMMAND_BIT: u64 = 0x0000_0010;
const LEFT_ALTERNATE_BIT: u64 = 0x0000_0020;
const RIGHT_ALTERNATE_BIT: u64 = 0x0000_0040;
const RIGHT_CONTROL_BIT: u64 = 0x0000_2000;

/// A modifier key: its virtual key code, the bit that says this side is
/// down, and the flag and both sides' bits of its kind.
struct ModifierKey {
    code: u32,
    side_bit: u64,
    flag: u64,
    both_sides: u64,
}

/// Every key that changes the event flags, as macOS's `kVK_` codes name
/// them. Caps Lock and Fn have no sides: their flag is their bit.
const MODIFIER_KEYS: [ModifierKey; 10] = [
    ModifierKey {
        code: 0x37,
        side_bit: LEFT_COMMAND_BIT,
        flag: COMMAND_FLAG,
        both_sides: LEFT_COMMAND_BIT | RIGHT_COMMAND_BIT,
    },
    ModifierKey {
        code: 0x36,
        side_bit: RIGHT_COMMAND_BIT,
        flag: COMMAND_FLAG,
        both_sides: LEFT_COMMAND_BIT | RIGHT_COMMAND_BIT,
    },
    ModifierKey {
        code: 0x38,
        side_bit: LEFT_SHIFT_BIT,
        flag: SHIFT_FLAG,
        both_sides: LEFT_SHIFT_BIT | RIGHT_SHIFT_BIT,
    },
    ModifierKey {
        code: 0x3C,
        side_bit: RIGHT_SHIFT_BIT,
        flag: SHIFT_FLAG,
        both_sides: LEFT_SHIFT_BIT | RIGHT_SHIFT_BIT,
    },
    ModifierKey {
        code: 0x3A,
        side_bit: LEFT_ALTERNATE_BIT,
        flag: ALTERNATE_FLAG,
        both_sides: LEFT_ALTERNATE_BIT | RIGHT_ALTERNATE_BIT,
    },
    ModifierKey {
        code: 0x3D,
        side_bit: RIGHT_ALTERNATE_BIT,
        flag: ALTERNATE_FLAG,
        both_sides: LEFT_ALTERNATE_BIT | RIGHT_ALTERNATE_BIT,
    },
    ModifierKey {
        code: 0x3B,
        side_bit: LEFT_CONTROL_BIT,
        flag: CONTROL_FLAG,
        both_sides: LEFT_CONTROL_BIT | RIGHT_CONTROL_BIT,
    },
    ModifierKey {
        code: 0x3E,
        side_bit: RIGHT_CONTROL_BIT,
        flag: CONTROL_FLAG,
        both_sides: LEFT_CONTROL_BIT | RIGHT_CONTROL_BIT,
    },
    ModifierKey {
        code: 0x39,
        side_bit: CAPS_LOCK_FLAG,
        flag: CAPS_LOCK_FLAG,
        both_sides: CAPS_LOCK_FLAG,
    },
    ModifierKey {
        code: 0x3F,
        side_bit: FUNCTION_FLAG,
        flag: FUNCTION_FLAG,
        both_sides: FUNCTION_FLAG,
    },
];

/// What kind of key event the tap heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyEvent {
    Down,
    Up,
    /// A modifier went down or came up; the flags say which.
    FlagsChanged,
}

/// The key events among Quartz's event types.
fn key_event(event_type: CGEventType) -> Option<KeyEvent> {
    if event_type == CGEventType::KeyDown {
        Some(KeyEvent::Down)
    } else if event_type == CGEventType::KeyUp {
        Some(KeyEvent::Up)
    } else if event_type == CGEventType::FlagsChanged {
        Some(KeyEvent::FlagsChanged)
    } else {
        None
    }
}

fn modifier_key(code: u32) -> Option<&'static ModifierKey> {
    MODIFIER_KEYS.iter().find(|key| key.code == code)
}

/// Whether the modifier `key` is down by the event `flags`. The side's own
/// bit says so; an event without any side bits, as some remappers send, has
/// only the flag of its kind to go by.
fn modifier_down(key: &ModifierKey, flags: u64) -> bool {
    if flags & key.side_bit != 0 {
        return true;
    }
    flags & key.flag != 0 && flags & key.both_sides == 0
}

/// The key a tap event is about, and whether it went down: its virtual key
/// code, as winit gives it as a scan code on macOS. A modifier's change
/// carries the key's code and the flags, which tell whether it went down or
/// came up. None for an event that is no key, or a key code that can't be
/// one.
fn key_change(event: KeyEvent, key_code: i64, flags: u64) -> Option<(u32, bool)> {
    if !(0..=HIGHEST_KEY_CODE).contains(&key_code) {
        return None;
    }
    let code = key_code as u32;
    match event {
        KeyEvent::Down => Some((code, true)),
        KeyEvent::Up => Some((code, false)),
        KeyEvent::FlagsChanged => modifier_key(code).map(|key| (code, modifier_down(key, flags))),
    }
}

/// Whether the event `flags` still have `code` down, for a key that isn't a
/// modifier, which they say nothing of.
fn still_down(code: u32, flags: u64) -> bool {
    modifier_key(code).is_none_or(|key| modifier_down(key, flags))
}

/// What the tap's callback works on, on the tap thread only.
struct TapState {
    matcher: Matcher,
    sink: Sink,
    /// The tap, to switch it on again when macOS switches it off.
    tap: Option<CFRetained<CFMachPort>>,
    stop: Arc<AtomicBool>,
}

impl TapState {
    /// A key event reached the keybinds.
    fn heard(&mut self, event: KeyEvent, key_code: i64, flags: u64) {
        let sink = &self.sink;
        let out = &mut |action, down| sink.act(action, down);
        if event == KeyEvent::FlagsChanged {
            // A modifier's key-up the tap missed shows in the flags.
            self.matcher
                .resync(None, |code| still_down(code, flags), out);
        }
        if let Some((code, down)) = key_change(event, key_code, flags) {
            self.matcher.key(code, down, out);
        }
    }

    /// Switches the tap on, and lets every key go: the keys that came up
    /// while it was off weren't heard.
    fn recover(&mut self) {
        if let Some(tap) = &self.tap {
            CGEvent::tap_enable(tap, true);
        }
        let sink = &self.sink;
        self.matcher
            .release_all(&mut |action, down| sink.act(action, down));
    }
}

/// Quartz calls this for each event the tap hears, on the tap thread.
///
/// # Safety
/// `user_info` is the `RefCell<TapState>` [`tap_thread`] keeps alive for as
/// long as the tap exists, and only this thread runs the callback.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: objc2_core_graphics::CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let heard = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: see above.
        let state = unsafe { &*(user_info as *const RefCell<TapState>) };
        // SAFETY: Quartz hands the callback a live event.
        let event = unsafe { event.as_ref() };
        handle_event(state, event_type, event);
    }));
    if heard.is_err() {
        tracing::error!("a keybind event handler panicked");
    }
    event.as_ptr()
}

fn handle_event(state: &RefCell<TapState>, event_type: CGEventType, event: &CGEvent) {
    let Ok(mut state) = state.try_borrow_mut() else {
        tracing::error!("a keybind event arrived inside another");
        return;
    };
    if state.stop.load(Ordering::Relaxed) {
        return;
    }
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput
    {
        tracing::warn!(
            timeout = event_type == CGEventType::TapDisabledByTimeout,
            "macOS switched the keybind tap off; switching it on"
        );
        state.recover();
        return;
    }
    let Some(kind) = key_event(event_type) else {
        return;
    };
    let key_code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    let flags = CGEvent::flags(Some(event)).bits();
    state.heard(kind, key_code, flags);
}

/// Why there is no tap yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Missing {
    NotAllowed,
    NoTap,
}

impl Missing {
    fn why(self) -> &'static str {
        match self {
            Missing::NotAllowed => NOT_ALLOWED,
            Missing::NoTap => NO_TAP,
        }
    }
}

/// A tap on the keyboard, and the run loop source that brings its events.
struct Installed {
    tap: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
}

/// Makes the tap if macOS allows it, and adds it to this thread's run loop.
fn install(state: &Rc<RefCell<TapState>>) -> Result<Installed, Missing> {
    if !CGPreflightListenEventAccess() {
        return Err(Missing::NotAllowed);
    }
    let user_info = Rc::as_ptr(state) as *mut c_void;
    // SAFETY: the callback matches Quartz's type, and `user_info` outlives
    // the tap: `tap_thread` drops its state only after invalidating it.
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            EVENT_MASK,
            Some(tap_callback),
            user_info,
        )
    }
    .ok_or(Missing::NoTap)?;
    let source =
        CFMachPort::new_run_loop_source(None, Some(&tap), SOURCE_ORDER).ok_or(Missing::NoTap)?;
    let run_loop = CFRunLoop::current().ok_or(Missing::NoTap)?;
    // SAFETY: reading Core Foundation's constant.
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&tap, true);
    state.borrow_mut().tap = Some(tap.clone());
    Ok(Installed { tap, source })
}

/// Sleeps `total`, or less if `stop` comes first; true if it did.
fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) -> bool {
    let mut slept = Duration::ZERO;
    while slept < total {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        std::thread::sleep(STOP_POLL);
        slept += STOP_POLL;
    }
    stop.load(Ordering::Relaxed)
}

/// Tries to install the tap until it works or `stop` comes, telling the page
/// when the reason there is none changes.
fn install_when_allowed(
    state: &Rc<RefCell<TapState>>,
    report: &Report,
    stop: &AtomicBool,
) -> Option<Installed> {
    let mut last = None;
    loop {
        match install(state) {
            Ok(installed) => return Some(installed),
            Err(missing) => {
                if last != Some(missing) {
                    tracing::warn!(?missing, "no global keybinds yet");
                    report(Status {
                        reach: Reach::Focused(missing.why()),
                        given: HashMap::new(),
                    });
                    last = Some(missing);
                }
            }
        }
        if sleep_unless_stopped(stop, RETRY_EVERY) {
            return None;
        }
    }
}

/// The tap thread's run loop, for the thread that drops [`Tap`] to wake.
struct SharedRunLoop(CFRetained<CFRunLoop>);

// SAFETY: CFRunLoopStop, the only call made through a shared one, may be
// made from any thread.
unsafe impl Send for SharedRunLoop {}

/// A listen-only event tap on its own thread; ends when dropped.
pub struct Tap {
    stop: Arc<AtomicBool>,
    run_loop: Arc<Mutex<Option<SharedRunLoop>>>,
}

impl Tap {
    pub(super) fn start(matcher: Matcher, sink: Sink, report: Report) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let run_loop = Arc::new(Mutex::new(None));
        let thread = (stop.clone(), run_loop.clone());
        let spawned = std::thread::Builder::new()
            .name("yapfast-keybinds".into())
            .spawn(move || tap_thread(matcher, sink, report, thread.0, thread.1));
        if let Err(error) = spawned {
            tracing::warn!(%error, "no thread for the keybind tap");
            return None;
        }
        Some(Self { stop, run_loop })
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let run_loop = self
            .run_loop
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(SharedRunLoop(run_loop)) = run_loop.as_ref() {
            run_loop.stop();
        }
    }
}

/// Asks macOS for Input Monitoring once, if Yapfast hasn't got it.
fn ask_for_input_monitoring() {
    if CGPreflightListenEventAccess() {
        return;
    }
    let granted = CGRequestListenEventAccess();
    tracing::info!(
        granted,
        "asked macOS to let Yapfast watch keys in other apps"
    );
}

/// The tap thread: waits for the permission, installs the tap, and runs its
/// run loop in slices until told to stop.
fn tap_thread(
    matcher: Matcher,
    sink: Sink,
    report: Report,
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Option<SharedRunLoop>>>,
) {
    ask_for_input_monitoring();
    let state = Rc::new(RefCell::new(TapState {
        matcher,
        sink,
        tap: None,
        stop: stop.clone(),
    }));
    let Some(installed) = install_when_allowed(&state, &report, &stop) else {
        return;
    };
    match CFRunLoop::current() {
        Some(run_loop) => {
            *shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(SharedRunLoop(run_loop));
        }
        None => tracing::warn!("no run loop to stop the keybind tap's; it ends within a second"),
    }
    tracing::info!("keybind event tap installed");
    report(Status {
        reach: Reach::Everywhere,
        given: HashMap::new(),
    });
    run_until_stopped(&installed, &stop);
    shut_down(&state, installed);
}

fn run_until_stopped(installed: &Installed, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: reading Core Foundation's constant.
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, RUN_SLICE_SECONDS, false);
        if !CGEvent::tap_is_enabled(&installed.tap) {
            tracing::warn!("the keybind tap was off; switching it on");
            CGEvent::tap_enable(&installed.tap, true);
        }
    }
}

/// Takes the tap off and lets every key go, so nothing stays held.
fn shut_down(state: &Rc<RefCell<TapState>>, installed: Installed) {
    installed.tap.invalidate();
    if let Some(run_loop) = CFRunLoop::current() {
        // SAFETY: reading Core Foundation's constant.
        run_loop.remove_source(Some(&installed.source), unsafe { kCFRunLoopCommonModes });
    }
    let mut state = state.borrow_mut();
    state.tap = None;
    let sink = state.sink.clone();
    state
        .matcher
        .release_all(&mut |action, down| sink.act(action, down));
    tracing::info!("keybind event tap removed");
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_V: i64 = 0x09;
    const LEFT_SHIFT: i64 = 0x38;
    const RIGHT_SHIFT: i64 = 0x3C;
    const LEFT_COMMAND: i64 = 0x37;
    const RIGHT_COMMAND: i64 = 0x36;
    const LEFT_OPTION: i64 = 0x3A;
    const RIGHT_OPTION: i64 = 0x3D;
    const LEFT_CONTROL: i64 = 0x3B;
    const RIGHT_CONTROL: i64 = 0x3E;
    const CAPS_LOCK: i64 = 0x39;
    const FUNCTION: i64 = 0x3F;

    #[test]
    fn a_key_is_its_virtual_key_code_going_down_or_up() {
        assert_eq!(key_change(KeyEvent::Down, KEY_V, 0), Some((0x09, true)));
        assert_eq!(key_change(KeyEvent::Up, KEY_V, 0), Some((0x09, false)));
        assert_eq!(key_change(KeyEvent::Down, -1, 0), None);
        assert_eq!(key_change(KeyEvent::Down, 0x100, 0), None);
    }

    #[test]
    fn a_modifier_goes_down_by_its_own_side_and_up_without_it() {
        let both_shifts = SHIFT_FLAG | LEFT_SHIFT_BIT | RIGHT_SHIFT_BIT;
        let change = |code, flags| key_change(KeyEvent::FlagsChanged, code, flags);
        assert_eq!(
            change(LEFT_SHIFT, SHIFT_FLAG | LEFT_SHIFT_BIT),
            Some((0x38, true))
        );
        // Right Shift joins: Left Shift's own bit is still set.
        assert_eq!(change(RIGHT_SHIFT, both_shifts), Some((0x3C, true)));
        // Left Shift comes up while Right Shift holds the flag.
        assert_eq!(
            change(LEFT_SHIFT, SHIFT_FLAG | RIGHT_SHIFT_BIT),
            Some((0x38, false))
        );
        assert_eq!(change(RIGHT_SHIFT, 0), Some((0x3C, false)));
    }

    #[test]
    fn every_modifier_tells_its_sides_apart() {
        let sides = [
            (
                LEFT_COMMAND,
                RIGHT_COMMAND,
                COMMAND_FLAG,
                LEFT_COMMAND_BIT,
                RIGHT_COMMAND_BIT,
            ),
            (
                LEFT_OPTION,
                RIGHT_OPTION,
                ALTERNATE_FLAG,
                LEFT_ALTERNATE_BIT,
                RIGHT_ALTERNATE_BIT,
            ),
            (
                LEFT_CONTROL,
                RIGHT_CONTROL,
                CONTROL_FLAG,
                LEFT_CONTROL_BIT,
                RIGHT_CONTROL_BIT,
            ),
        ];
        for (left, right, flag, left_bit, right_bit) in sides {
            let change = |code, flags| key_change(KeyEvent::FlagsChanged, code, flags);
            assert_eq!(change(left, flag | left_bit), Some((left as u32, true)));
            assert_eq!(change(right, flag | right_bit), Some((right as u32, true)));
            assert_eq!(change(left, flag | right_bit), Some((left as u32, false)));
            assert_eq!(change(right, flag | left_bit), Some((right as u32, false)));
        }
    }

    #[test]
    fn an_event_without_side_bits_goes_by_the_flag() {
        let change = |code, flags| key_change(KeyEvent::FlagsChanged, code, flags);
        assert_eq!(change(LEFT_COMMAND, COMMAND_FLAG), Some((0x37, true)));
        assert_eq!(change(LEFT_COMMAND, 0), Some((0x37, false)));
    }

    #[test]
    fn caps_lock_and_function_follow_their_flags() {
        let change = |code, flags| key_change(KeyEvent::FlagsChanged, code, flags);
        assert_eq!(change(CAPS_LOCK, CAPS_LOCK_FLAG), Some((0x39, true)));
        assert_eq!(change(CAPS_LOCK, 0), Some((0x39, false)));
        assert_eq!(change(FUNCTION, FUNCTION_FLAG), Some((0x3F, true)));
        assert_eq!(change(FUNCTION, 0), Some((0x3F, false)));
    }

    #[test]
    fn a_flags_change_of_no_modifier_is_no_key() {
        assert_eq!(key_change(KeyEvent::FlagsChanged, KEY_V, SHIFT_FLAG), None);
    }

    #[test]
    fn the_flags_tell_which_keys_are_still_down() {
        assert!(
            still_down(KEY_V as u32, 0),
            "flags say nothing of other keys"
        );
        assert!(still_down(
            LEFT_CONTROL as u32,
            CONTROL_FLAG | LEFT_CONTROL_BIT
        ));
        assert!(!still_down(
            LEFT_CONTROL as u32,
            CONTROL_FLAG | RIGHT_CONTROL_BIT
        ));
        assert!(!still_down(RIGHT_SHIFT as u32, 0));
    }

    #[test]
    fn the_modifiers_are_the_ones_keybinds_know() {
        for (left, right, _) in [
            (0x3B, 0x3E, "CTRL"),
            (0x38, 0x3C, "SHIFT"),
            (0x3A, 0x3D, "ALT"),
            (0x37, 0x36, "LOGO"),
        ] {
            assert!(modifier_key(left).is_some() && modifier_key(right).is_some());
            assert_eq!(
                yapfast_core::keybinds::modifier(left).map(|(l, r, _)| (l, r)),
                Some((left, right))
            );
        }
    }

    #[test]
    fn the_tap_hears_exactly_keys_and_modifiers() {
        assert_eq!(EVENT_MASK, (1 << 10) | (1 << 11) | (1 << 12));
        assert_eq!(key_event(CGEventType::MouseMoved), None);
        assert_eq!(key_event(CGEventType::KeyDown), Some(KeyEvent::Down));
    }
}
