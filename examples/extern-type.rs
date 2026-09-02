//! A structure described once, referred to from two groups of events.
//!
//! A group lays its events out into one object and reaches everything
//! in it by a distance, which costs nothing. A structure two *groups*
//! share cannot be reached that way, so it is described in an object of
//! its own with `define_type!` and reached by address, which
//! `side_extern()` says at the field. That is the same division of work
//! as `side_define_struct()` and `side_extern()` in the C API.
//!
//! The address cannot be written at build time -- the const evaluator
//! refuses to turn a pointer into an integer -- so the constructor of
//! each group writes it before registering, which is the work a loader
//! does for a relocation and costs the same.

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

/* One description of the structure, for both groups. */
define_type!(PROCESS_INFO, ProcessInfo);

#[libside::events]
mod lifecycle {
    use super::*;

    define_event!(
        started,
        provider: "rust",
        event: "started",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            seq: u32,
            process: side_extern(PROCESS_INFO),
        ),
    );

    define_event!(
        exited,
        provider: "rust",
        event: "exited",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            status: i32,
            process: side_extern(PROCESS_INFO),
        ),
    );
}

#[libside::events]
mod scheduling {
    use super::*;

    define_event!(
        preempted,
        provider: "rust",
        event: "preempted",
        level: SIDE_LOGLEVEL_DEBUG,
        fields: (
            process: side_extern(PROCESS_INFO),
            thread: &ThreadInfo,
        ),
    );
}

fn main() {
    let thread = |id| ThreadInfo {
        id,
        prio: 0,
        running: true,
    };
    let process = ProcessInfo {
        pid: 1234,
        ppid: 1,
        nice: -5,
        alive: true,
        main_thread: thread(1234),
        threads: [thread(1), thread(2), thread(3), thread(4)],
    };

    side_event!(lifecycle::started, 0, &process);
    side_event!(lifecycle::exited, 0, &process);
    side_event!(scheduling::preempted, &process, &thread(7));
}
