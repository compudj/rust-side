//! Dumping the state a tracer missed by starting late.
//!
//! An event says what changed; a tracer which starts afterwards has no
//! way to know what the state was before it did. A state dump closes
//! that gap: the tracer asks, the application walks what it has, and
//! the result goes to the one tracer which asked rather than to every
//! tracer listening.
//!
//! The callback belongs to the group, `#[libside::events(statedump =
//! ...)]`, so that the group's own constructor registers the events
//! first and the callback second. That order matters: registering the
//! callback queues a dump at once, and the events it dumps have to
//! exist by then.
//!
//! Both modes are here. `tasks` is dumped by a thread libside spawns,
//! which asks nothing of the application; `sessions` is dumped by the
//! application, from `run_pending_statedumps()`, which is the bargain a
//! program with an event loop and no wish for another thread makes.

use libside::*;
use std::sync::Mutex;

/// The state, which the dump walks and the events change.
static TASKS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());
static SESSIONS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

#[libside::events(statedump = dump_tasks, mode = agent_thread)]
mod tasks {
    use super::*;

    define_event!(
        task_running,
        provider: "rust",
        event: "task_running",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            id: u32,
            name: &str,
        ),
    );

    define_event!(
        task_started,
        provider: "rust",
        event: "task_started",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            id: u32,
            name: &str,
        ),
    );
}

#[libside::events(statedump = dump_sessions, mode = polling)]
mod sessions {
    use super::*;

    define_event!(
        session_open,
        provider: "rust",
        event: "session_open",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            id: u64,
        ),
    );
}

/*
 * A state dump callback takes the key of the dump under way and nothing
 * else: libside has no pointer of ours to hand back, so it cannot be a
 * closure. The key says which tracer asked, and borrows from the call,
 * so it cannot be kept for later -- there is no later.
 *
 * This one runs on a thread libside spawns, so it takes the lock like
 * any other thread would.
 */
fn dump_tasks(key: StatedumpKey<'_>) {
    for (id, name) in TASKS.lock().unwrap().iter() {
        side_statedump_event!(tasks::task_running, key, *id, name.as_str());
    }
}

/* This one runs on the thread which called run_pending_statedumps(). */
fn dump_sessions(key: StatedumpKey<'_>) {
    for id in SESSIONS.lock().unwrap().iter() {
        side_statedump_event!(sessions::session_open, key, *id);
    }
}

fn main() {
    /*
     * State the program builds as it runs. A tracer which starts after
     * this has no event to tell it any of it happened, which is the
     * whole reason to have a state dump.
     */
    TASKS.lock().unwrap().push((1, "init".into()));
    TASKS.lock().unwrap().push((2, "logger".into()));
    SESSIONS.lock().unwrap().push(4242);

    /* And a change, which an event says outright. */
    let (id, name) = (3, "worker");
    TASKS.lock().unwrap().push((id, name.into()));
    side_event!(tasks::task_started, id, name);
    SESSIONS.lock().unwrap().push(4243);
    side_event!(sessions::session_open, 4243);

    /*
     * A dump also happens when the callback is registered, which is in
     * this group's constructor, before main: it finds no task and no
     * session, because there were none yet. That is not a
     * disappointment, it is the answer to the question -- what the
     * state dump reports is the state at the moment it is taken.
     *
     * So run for a while, and start a session in the meantime:
     *
     *     lttng create; lttng enable-event -u 'rust:*'; lttng start
     *
     * The tracer asks for a dump as it starts, and this time both
     * callbacks find something to say.
     */
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        /*
         * The obligation `mode = polling' takes on: an event loop
         * reaches this every time round, and a tracer which asks waits
         * until it does. `tasks' owes nothing, its agent thread having
         * run its callback already.
         */
        if sessions::statedump_pending() {
            sessions::run_pending_statedumps();
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
