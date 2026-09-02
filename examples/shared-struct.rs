//! Two events reaching the same structure, one of them through an array.
//!
//! A type description is no longer something written once and pointed
//! at: every pointer within it is a distance from the member holding
//! it, so each event lays out its own copy of whatever it reaches. This
//! is where that is exercised.

use libside::*;

#[derive(SideGather)]
struct ThreadInfo {
    id: u32,
    running: bool,
}

#[derive(SideGather)]
struct ProcessInfo {
    pid: u32,
    threads: [ThreadInfo; 3],
}

define_event!(
    process_event,
    provider: "rust",
    event: "process",
    level: SIDE_LOGLEVEL_INFO,
    fields: (
        process: &ProcessInfo,
    ),
);

define_event!(
    thread_event,
    provider: "rust",
    event: "thread",
    level: SIDE_LOGLEVEL_DEBUG,
    fields: (
        pid: u32,
        thread: &ThreadInfo,
        note: &str,
    ),
);

fn main() {
    let process = ProcessInfo {
        pid: 7,
        threads: [
            ThreadInfo { id: 70, running: true },
            ThreadInfo { id: 71, running: false },
            ThreadInfo { id: 72, running: true },
        ],
    };

    process_event!(&process);
    thread_event!(process.pid, thread: &process.threads[1], note: "second thread");
}
