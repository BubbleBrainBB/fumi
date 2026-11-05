//! Timers. All of this runs on the main thread; atomics only keep the statics
//! simple.
//!
//! - `busy_ns`: main-thread time, from a CFRunLoop observer: every stretch from
//!   waking up to going back to sleep. It covers event delivery, GPUI's update,
//!   layout, paint and the Metal encode, which `draw` below cannot see.
//! - `frames`: draw time from our root view's `render` to the last `paint`.
//! - `handler`: the time inside `Editor::edit`.

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

static BUSY_TOTAL: AtomicU64 = AtomicU64::new(0);
static SLICE_START: AtomicU64 = AtomicU64::new(0);
static IN_SLICE: AtomicBool = AtomicBool::new(false);
static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static FRAME_T0: AtomicU64 = AtomicU64::new(0);
static LAST_DRAW: AtomicU64 = AtomicU64::new(0);
static LAST_FRAME_END: AtomicU64 = AtomicU64::new(0);
static LAST_HANDLER: AtomicU64 = AtomicU64::new(0);
static LOAD_ROPE: AtomicU64 = AtomicU64::new(0);
static LOAD_SCAN: AtomicU64 = AtomicU64::new(0);
static RECORD: AtomicBool = AtomicBool::new(false);
static FRAMES: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
static START: Mutex<Option<Instant>> = Mutex::new(None);

pub fn now() -> u64 {
    let mut s = START.lock().unwrap();
    s.get_or_insert_with(Instant::now).elapsed().as_nanos() as u64
}

pub fn frame_begin() {
    FRAME_T0.store(now(), Relaxed);
}

pub fn frame_end() {
    let t = now();
    let draw = t.saturating_sub(FRAME_T0.load(Relaxed));
    LAST_DRAW.store(draw, Relaxed);
    LAST_FRAME_END.store(t, Relaxed);
    FRAME_COUNT.fetch_add(1, Relaxed);
    if RECORD.load(Relaxed) {
        FRAMES.lock().unwrap().push((t, draw));
    }
}

pub fn frames() -> u64 {
    FRAME_COUNT.load(Relaxed)
}
pub fn last_draw() -> u64 {
    LAST_DRAW.load(Relaxed)
}
pub fn last_frame_end() -> u64 {
    LAST_FRAME_END.load(Relaxed)
}
pub fn last_handler() -> u64 {
    LAST_HANDLER.load(Relaxed)
}
pub fn note_handler(d: Duration) {
    LAST_HANDLER.store(d.as_nanos() as u64, Relaxed);
}
pub fn note_load(rope: Duration, scan: Duration) {
    LOAD_ROPE.store(rope.as_nanos() as u64, Relaxed);
    LOAD_SCAN.store(scan.as_nanos() as u64, Relaxed);
}
pub fn load_times() -> (u64, u64) {
    (LOAD_ROPE.load(Relaxed), LOAD_SCAN.load(Relaxed))
}
pub fn record_frames(on: bool) {
    RECORD.store(on, Relaxed);
}
pub fn take_frames() -> Vec<(u64, u64)> {
    std::mem::take(&mut FRAMES.lock().unwrap())
}

/// Main-thread busy time so far, counting the stretch in progress.
pub fn busy_ns() -> u64 {
    let total = BUSY_TOTAL.load(Relaxed);
    if IN_SLICE.load(Relaxed) {
        total + now().saturating_sub(SLICE_START.load(Relaxed))
    } else {
        total
    }
}

type CFIndex = isize;
const BEFORE_WAITING: usize = 1 << 5;
const AFTER_WAITING: usize = 1 << 6;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopObserverCreate(
        allocator: *const c_void,
        activities: usize,
        repeats: u8,
        order: CFIndex,
        callout: extern "C" fn(*mut c_void, usize, *mut c_void),
        context: *const c_void,
    ) -> *mut c_void;
    fn CFRunLoopAddObserver(rl: *mut c_void, observer: *mut c_void, mode: *const c_void);
}

extern "C" fn on_activity(_: *mut c_void, activity: usize, _: *mut c_void) {
    if activity == AFTER_WAITING {
        SLICE_START.store(now(), Relaxed);
        IN_SLICE.store(true, Relaxed);
    } else if activity == BEFORE_WAITING && IN_SLICE.swap(false, Relaxed) {
        let d = now().saturating_sub(SLICE_START.load(Relaxed));
        BUSY_TOTAL.fetch_add(d, Relaxed);
    }
}

pub fn install_runloop_observer() {
    unsafe {
        let obs = CFRunLoopObserverCreate(
            std::ptr::null(),
            BEFORE_WAITING | AFTER_WAITING,
            1,
            isize::MIN,
            on_activity,
            std::ptr::null(),
        );
        CFRunLoopAddObserver(CFRunLoopGetMain(), obs, kCFRunLoopCommonModes);
    }
}

/// Current and lifetime-peak physical footprint, the number Activity Monitor
/// shows as Memory.
pub fn footprint_mb() -> (f64, f64) {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V4,
            &mut info as *mut _ as *mut *mut c_void,
        )
    };
    if rc != 0 {
        return (0., 0.);
    }
    let mb = |b: u64| b as f64 / 1_048_576.;
    (mb(info.ri_phys_footprint), mb(info.ri_lifetime_max_phys_footprint))
}
