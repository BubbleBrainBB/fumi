//! Watching a folder for changes, with FSEvents. Paths arrive on a channel;
//! the stream stops when the `Watcher` is dropped.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

type CFRef = *const c_void;

#[repr(C)]
struct StreamContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

const UTF8: u32 = 0x0800_0100;
const NOW: u64 = u64::MAX;
const NO_DEFER: u32 = 0x2;
const FILE_EVENTS: u32 = 0x10;

#[link(name = "CoreServices", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: CFRef;
    fn CFStringCreateWithCString(alloc: CFRef, s: *const c_char, encoding: u32) -> CFRef;
    fn CFArrayCreate(alloc: CFRef, values: *const CFRef, n: isize, callbacks: *const c_void) -> CFRef;
    fn CFRunLoopGetCurrent() -> CFRef;
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: CFRef);
    fn FSEventStreamCreate(
        alloc: CFRef,
        callback: extern "C" fn(CFRef, *mut c_void, usize, *mut c_void, *const u32, *const u64),
        context: *const StreamContext,
        paths: CFRef,
        since: u64,
        latency: f64,
        flags: u32,
    ) -> CFRef;
    fn FSEventStreamScheduleWithRunLoop(stream: CFRef, run_loop: CFRef, mode: CFRef);
    fn FSEventStreamStart(stream: CFRef) -> bool;
    fn FSEventStreamStop(stream: CFRef);
    fn FSEventStreamInvalidate(stream: CFRef);
    fn FSEventStreamRelease(stream: CFRef);
}

unsafe extern "C" {
    static kCFTypeArrayCallBacks: c_void;
}

struct Sink {
    events: Sender<Vec<String>>,
    wake: Option<futures::channel::mpsc::UnboundedSender<()>>,
}

extern "C" fn on_events(
    _: CFRef,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    _: *const u32,
    _: *const u64,
) {
    let sink = unsafe { &*(info as *const Sink) };
    let paths = paths as *const *const c_char;
    let list = (0..count)
        .map(|i| unsafe { CStr::from_ptr(*paths.add(i)) }.to_string_lossy().into_owned())
        .collect();
    let _ = sink.events.send(list);
    if let Some(wake) = &sink.wake {
        let _ = wake.unbounded_send(());
    }
}

#[derive(Clone, Copy)]
struct SendRef(CFRef);
unsafe impl Send for SendRef {}

pub struct Watcher {
    run_loop: SendRef,
    pub events: Receiver<Vec<String>>,
}

impl Watcher {
    pub fn new(path: &Path) -> Option<Watcher> {
        Self::with_wake(path, None)
    }

    /// Also sends `()` on `wake` after each batch, so a task can sleep until then
    /// instead of polling.
    pub fn with_wake(path: &Path, wake: Option<futures::channel::mpsc::UnboundedSender<()>>) -> Option<Watcher> {
        let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let (tx, rx) = channel();
        let (loop_tx, loop_rx) = channel();
        std::thread::Builder::new()
            .name("fsevents".into())
            .spawn(move || unsafe {
                let sender = Box::into_raw(Box::new(Sink { events: tx, wake }));
                let context = StreamContext {
                    version: 0,
                    info: sender as *mut c_void,
                    retain: std::ptr::null(),
                    release: std::ptr::null(),
                    copy_description: std::ptr::null(),
                };
                let cf_path = CFStringCreateWithCString(std::ptr::null(), c_path.as_ptr(), UTF8);
                let values = [cf_path];
                let paths = CFArrayCreate(
                    std::ptr::null(),
                    values.as_ptr(),
                    1,
                    &kCFTypeArrayCallBacks as *const c_void,
                );
                let stream = FSEventStreamCreate(
                    std::ptr::null(),
                    on_events,
                    &context,
                    paths,
                    NOW,
                    0.2,
                    FILE_EVENTS | NO_DEFER,
                );
                let run_loop = CFRunLoopGetCurrent();
                FSEventStreamScheduleWithRunLoop(stream, run_loop, kCFRunLoopDefaultMode);
                FSEventStreamStart(stream);
                let _ = loop_tx.send(SendRef(run_loop));
                CFRunLoopRun();
                FSEventStreamStop(stream);
                FSEventStreamInvalidate(stream);
                FSEventStreamRelease(stream);
                drop(Box::from_raw(sender));
            })
            .ok()?;
        let run_loop = loop_rx.recv().ok()?;
        Some(Watcher { run_loop, events: rx })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        unsafe { CFRunLoopStop(self.run_loop.0) };
    }
}
