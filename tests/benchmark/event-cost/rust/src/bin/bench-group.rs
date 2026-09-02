//! Emit one libside event carrying a single 32-bit integer, in a loop,
//! from an event in a group, which is what a provider is.
//!
//! This program depends on libside only: `run-benchmark` preloads the
//! tracer, which subscribes to the event and records it, so what is
//! timed is the whole path from the instrumentation site to the
//! committed record.
//!
//! `bench-macro.rs` is the same program with the event on its own, and
//! `bench-side.c` the same again through the C API, which is what these
//! numbers are compared to.

use libside::*;
use std::time::Instant;

#[libside::events]
mod trace {
    use super::*;

    define_event!(
        bench_event,
        provider: "side_benchmark",
        event: "u32",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            v: u32,
        ),
    );
}

/// Never inlined, so that the loop is not folded into the timing code
/// and so that it is a symbol of its own in a profile.
#[inline(never)]
fn emit(nr: u64) {
    for i in 0..nr {
        side_event!(trace::bench_event, i as u32);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let arg = |args: &mut dyn Iterator<Item = String>, default: u64| {
        args.next().and_then(|a| a.parse().ok()).unwrap_or(default)
    };
    let iters = arg(&mut args, 5_000_000);
    let warmup = arg(&mut args, 1_000_000);
    let reps = arg(&mut args, 7);

    emit(warmup);

    for _ in 0..reps {
        let begin = Instant::now();
        emit(iters);
        let elapsed = begin.elapsed();
        /* Nanoseconds per event, one line per repetition. */
        println!("{:.2}", elapsed.as_nanos() as f64 / iters as f64);
    }
}
