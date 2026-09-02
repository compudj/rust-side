use libside::*;

#[derive(SideGather)]
struct ThreadInfo {
    id: u32,
}

static ROOT_THREAD: ThreadInfo = ThreadInfo { id: 1 };

#[derive(SideGather)]
struct ProcessInfo {
    pid: u32,
    priority: i32,
    running: bool,
    thread: ThreadInfo,
    parent_thread: &'static ThreadInfo,
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

fn main() {
    let process = ProcessInfo {
        pid: 1234,
        priority: -5,
        running: true,
        thread: ThreadInfo { id: 1234 },
        parent_thread: &ROOT_THREAD,
    };

    process_event!(&process);
}
