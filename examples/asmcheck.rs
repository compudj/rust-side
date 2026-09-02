//! What an instrumented function compiles to.
//!
//! The emission is the unlikely half of the branch which reaches it, so
//! a function which is not being traced should run a load, a test and a
//! branch it does not take, and everything else should be laid out past
//! its return. This is what to read to check that it still is:
//!
//! ```sh
//! cargo rustc --release --example asmcheck -- \
//!     --emit asm -C llvm-args=--x86-asm-syntax=intel
//! awk '/^handle_request:/,/^\.Lfunc_end/' \
//!     target/release/examples/asmcheck-*.s
//! ```
//!
//! Both forms are here, since they generate different code:
//! `handle_request` carries an event of a group and `handle_standalone`
//! one declared on its own.

use libside::*;

#[derive(SideGather)]
struct Request {
    id: u32,
    len: u32,
}

#[libside::events]
mod trace {
    use super::*;

    define_event!(
        request,
        provider: "rust",
        event: "request",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            id: u32,
            checksum: u32,
            request: &Request,
        ),
    );
}

define_event!(
    standalone,
    provider: "rust",
    event: "standalone",
    level: SIDE_LOGLEVEL_INFO,
    fields: (
        id: u32,
        checksum: u32,
        request: &Request,
    ),
);

fn checksum(buf: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for &b in buf {
        sum = sum.wrapping_add(b as u32).rotate_left(3);
    }
    sum
}

/// A function with something in it, carrying an event of a group.
#[inline(never)]
#[no_mangle]
pub fn handle_request(id: u32, buf: &[u8]) -> u32 {
    let sum = checksum(buf);

    side_event!(
        trace::request,
        id,
        sum,
        &Request { id, len: buf.len() as u32 }
    );

    sum ^ 0x5a5a5a5a
}

/// The same, carrying an event declared on its own.
#[inline(never)]
#[no_mangle]
pub fn handle_standalone(id: u32, buf: &[u8]) -> u32 {
    let sum = checksum(buf);

    standalone!(id, sum, &Request { id, len: buf.len() as u32 });

    sum ^ 0x5a5a5a5a
}

fn main() {
    let buf = std::hint::black_box(vec![1u8, 2, 3, 4]);
    let id = std::hint::black_box(7);

    println!("{} {}", handle_request(id, &buf), handle_standalone(id, &buf));
}
