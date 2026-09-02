use libside::{define_event, SideGather, SIDE_LOGLEVEL_INFO};

// Make these Rust structs usable by libside.
#[derive(SideGather)]
struct ThreadInfo {
    id: u32,
}

#[derive(SideGather)]
struct ProcessInfo {
    pid: u32,
    threads: Vec<ThreadInfo>,
}

// Declare the event.
define_event!(
    hello_world_event,
    provider: "rust",
    event: "hello_world",
    level: SIDE_LOGLEVEL_INFO,
    fields: (
        id: u32,
        message: &str,
        process: &ProcessInfo,
    ),
);

fn main() {
    let threads = vec![ThreadInfo { id: 42 }, ThreadInfo { id: 43 }];
    let process = ProcessInfo { pid: 42, threads };

    // Named arguments may be passed in any order.
    hello_world_event!(
        4,
        process: &process,
        message: "hello from rust",
    );
}
