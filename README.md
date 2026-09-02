# libside-rust

Experimental Rust frontend for `libside`.

This crate is intentionally separate from the `libside` source tree. It provides:

- a small Rust FFI layer for the `side_call` ABI,
- a `define_event!` declaration macro that emits libside metadata and a Rust inline backend,
- Cargo-based linking against an installed or in-tree `libside`.

## Linking

By default, `build.rs` looks up `libside` with `pkg-config`.

Useful overrides:

- `PKG_CONFIG_PATH=/path/to/libside/src/lib/pkgconfig`
- `LIBSIDE_LIB_DIR=/path/to/libside/src/lib/libside/.libs`
- `LIBSIDE_NO_PKG_CONFIG=1`

When `LIBSIDE_LIB_DIR` is set, the crate links `-lside` from that directory directly.

## Current scope

This first version only covers non-variadic `side_call` events with fixed fields.

Example:

```rust
use libside::{define_event, SIDE_LOGLEVEL_DEBUG};

define_event!(
    my_event,
    provider: "myprovider",
    event: "myevent",
    level: SIDE_LOGLEVEL_DEBUG,
    fields: (
        count: u32,
        name: &str,
    ),
);

my_event!(42, "hello");
```

## Gather structs

`SideGather` describes the actual layout of a Rust struct in the event
metadata. Passing a reference to that struct then emits a single pointer, and
libside reads the declared fields from that pointer.

```rust
use libside::{define_event, SideGather, SIDE_LOGLEVEL_INFO};

#[derive(SideGather)]
struct ProcessInfo {
    pid: u32,
    priority: i32,
    running: bool,
}

define_event!(
    process_event,
    provider: "rust",
    event: "process",
    level: SIDE_LOGLEVEL_INFO,
    fields: (process: &ProcessInfo,),
);

process_event!(&ProcessInfo { pid: 1234, priority: -5, running: true });
```

The current `SideGather` derive supports `bool`, fixed-width signed and unsigned integer
members, nested structs, fixed-size arrays of nested structs, and `Vec<T>`
members. The struct and any vector backing storage must remain valid for the
duration of the event call.

## Running the example

On Linux, `cargo run` now goes through `.cargo/run-with-lttng-ust.sh`, which
resolves `liblttng-ust.so` with `pkg-config` and prepends it to `LD_PRELOAD`.

That means the example can be run directly with Cargo:

```sh
lttng create
lttng enable-event -u 'rust:*'
lttng start
cargo run --example hello-world
lttng destroy
```

If the tracer `.so` is not in the default `pkg-config` library directory, set
`LTTNG_UST_PATH=/absolute/path/to/liblttng-ust.so`.
