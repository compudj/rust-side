# libside-rust

Experimental Rust frontend for `libside`.

This crate is intentionally separate from the `libside` source tree. It provides:

- a small Rust FFI layer for the `side_call` ABI,
- a `define_event!` declaration macro that emits libside metadata and a Rust inline backend,
- Cargo-based linking against an installed or in-tree `libside`.

## Toolchain

Rust 1.83 or newer. The description of an event is laid out by the const
evaluator, which needs mutable references in `const fn` (1.83) and
`offset_of!` (1.77). Debian bookworm's 1.63 is too old; install a
toolchain with `rustup`.

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

## Providers: grouping events which share types

A description is laid out into one object and every distance within it
is between two bytes of that object, so two events are two objects and a
type used by both is described twice. `#[libside::events]` on a module
lays its events out together, which is the boundary a translation unit
gives the C API and the one a tracepoint provider has always had:

```rust
#[libside::events]
mod trace {
    use super::*;

    define_event!(
        process_started,
        provider: "rust",
        event: "process_started",
        level: SIDE_LOGLEVEL_INFO,
        fields: (seq: u32, process: &ProcessInfo),
    );

    define_event!(
        process_exited,
        provider: "rust",
        event: "process_exited",
        level: SIDE_LOGLEVEL_INFO,
        fields: (status: i32, process: &ProcessInfo),
    );
}

trace::process_started(0, &process);
```

The events are written exactly as they are on their own; the module is
the only new thing. `examples/provider.rs` is three events sharing two
structures: **2640 bytes of description ungrouped, 1552 grouped**, the
same behaviour and no relocation either way.
`tests/benchmark/description-sharing` weighs it at a thousand. What is shared is the
description of a structure, which is the same bytes wherever it is used;
the 64 byte type at each point of use still carries its own offset and
access mode, exactly as `side_static_define_struct` and
`side_field_gather_struct` divide the work in C.

Two structures which describe the same way *are* the same description,
so what is compared is the shape rather than which Rust type it came
from. Merging them is right, not a coincidence to be avoided.

A group calls the events as functions rather than through a macro, and
registers all of them with one call:

```rust
trace::process_started(0, &process);            // asks whether it is enabled first

if trace::thread_switched_enabled() {           // only where the arguments cost something
    trace::thread_switched(&expensive(), &other());
}
```

That mirrors `tracepoint()` and `tracepoint_enabled()`. A standalone
`define_event!` keeps its macro and its named arguments; grouping is
additive, and nothing changes for an event which does not want it.

## Crossing a group: `define_type!` and `side_extern()`

A distance is between two bytes of one object, so a group can share
nothing with another group, or with another crate. What crosses that
boundary is an address, which is exactly what the C API does with
`side_define_struct()` and `side_extern()`:

```rust
libside::define_type!(PROCESS_INFO, ProcessInfo);   // described on its own

#[libside::events]
mod lifecycle {
    use super::*;
    define_event!(
        started,
        provider: "rust",
        event: "started",
        level: SIDE_LOGLEVEL_INFO,
        fields: (seq: u32, process: side_extern(PROCESS_INFO)),
    );
}
```

The address cannot be written at build time -- the const evaluator
refuses to turn a pointer into an integer, whatever it points at -- so
the group's constructor writes it before registering. That is the work a
loader does for a relocation, and it costs the same: one store, dirtying
the one page the relocation would have dirtied.

**Which is why it is worth measuring before reaching for it.** Two
groups of twenty events over the same structure:

| | description | pages dirtied |
|---|---|---|
| `side_extern(PROCESS_INFO)` | 10240 bytes | 3 of 3 |
| each group describing it | 11056 bytes | 0 of 3 |

816 bytes saved, and every page of the descriptions private to the
process instead of shared between them. Within one group it is worse
still: the group already shares a structure two of its events describe
the same way, so `side_extern()` there buys nothing at all and only
costs the pages.

Reach for it for the two things duplication cannot do:

- **crossing a crate**, where there is no other way to share at all;
- **identity** -- a tracer sees one object at one address, so it can
  tell that two events use *the same* type rather than two which happen
  to describe the same way. That is what a CTF2 field class alias needs,
  and no amount of duplication provides it.

`examples/extern-type.rs` is two groups over one structure.

## How a description is built

Everything a libside description points at, it points at by the distance
from the member holding the pointer to what it points at, rather than by
an address. That is what keeps a description free of load-time
relocations, so the pages it lives on stay clean and shared between
processes instead of being copied into every one of them.

C cannot write such a pointer: the difference of two addresses is not a
constant expression, so libside names the distance as an absolute
assembler symbol and takes its address. Rust cannot write one either --
a pointer cannot be cast to an integer in a constant -- but it does not
need the same trick, because it can compute the whole thing:

`define_event!` measures the description with `side::event_size()` and
then lays it out with `side::build_event()`, a `const fn` which writes
the description, the provider and event names, the field array and every
type they reach into **one** `[u8; N]`. Every distance is then between
two bytes of one object, which the const evaluator knows, so nothing is
left for the assembler or the loader to work out:

```
side_event_state_ptr     size     16  relocations 2
side_event_description   size   1088  relocations 0
side_event_state         size     64  relocations 4
```

The two relocations an event does cost are in `side_event_state`, a
section a tracer writes to anyway when it enables the event, and one
more in `side_event_state_ptr`, which is how an event is reached. The
description itself costs none. This holds under `--release`, under fat
LTO and under `--gc-sections`.

One consequence is worth knowing when reading the code: a type
description is no longer something to write once and point at from
several places, since its bytes depend on where it sits. What
`#[derive(SideGather)]` produces is therefore a *shape*
(`side::Layout`), not a description, and each event lays out its own
copy of whatever its fields reach.

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

To see the descriptions and the arguments without a session daemon, run an
example under libside's console tracer instead:

```sh
LD_PRELOAD=/usr/local/lib/libside-console-tracer.so ./target/debug/examples/hello-world
```
