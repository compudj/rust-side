//! A group of events which share the descriptions of their types.
//!
//! The module is the provider: its events are laid out together, so a
//! structure two of them describe the same way is described once. That
//! is the boundary a translation unit gives the C API, and the one a
//! tracepoint provider has always had.

use libside::*;

#[derive(SideGather)]
struct ThreadInfo {
    id: u32,
    prio: i32,
    running: bool,
}

#[derive(SideGather)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    nice: i32,
    alive: bool,
    main_thread: ThreadInfo,
    threads: [ThreadInfo; 4],
}

#[libside::events]
mod trace {
    use super::*;

    define_event!(
        process_started,
        provider: "rust",
        event: "process_started",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            seq: u32,
            process: &ProcessInfo,
        ),
    );

    define_event!(
        process_exited,
        provider: "rust",
        event: "process_exited",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            status: i32,
            process: &ProcessInfo,
        ),
    );

    define_event!(
        thread_switched,
        provider: "rust",
        event: "thread_switched",
        level: SIDE_LOGLEVEL_DEBUG,
        fields: (
            from: &ThreadInfo,
            to: &ThreadInfo,
        ),
    );
}

fn main() {
    let thread = |id| ThreadInfo { id, prio: 0, running: true };
    let process = ProcessInfo {
        pid: 1234,
        ppid: 1,
        nice: -5,
        alive: true,
        main_thread: thread(1234),
        threads: [thread(1), thread(2), thread(3), thread(4)],
    };

    trace::process_started(0, &process);
    trace::process_exited(0, &process);

    /*
     * Ask first only where working out the arguments costs something:
     * the event itself asks before it reads any of them.
     */
    if trace::thread_switched_enabled() {
        trace::thread_switched(&thread(1), &thread(2));
    }
}
