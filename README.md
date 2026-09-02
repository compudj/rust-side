# libside-rust

Experimental Rust frontend for `libside`.

This crate is intentionally separate from the `libside` source tree. It provides:

- a small Rust FFI layer for the `side_call` ABI,
- `define_event!`, which declares an event and gives it a macro of its
  own name,
- `#[libside::events]`, which makes a module a provider: its events are
  described together, so a type two of them use is described once, and
  `side_event!()` emits them,
- `#[derive(SideGather)]`, which describes a Rust struct so libside can
  read its fields, and `define_type!`, which describes one on its own so
  that other providers can reach it with `side_extern()`,
- attributes on an event and on the type of a field, which reach the
  CTF2 metadata,
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

There are two ways to declare one, and they are called differently. An
event on its own gets a macro of its own name, and takes named
arguments; an event in a provider -- a module marked
`#[libside::events]`, which is what lets its events share the
descriptions of their types -- is reached through `side_event!()` and
the path which names it. Both ask whether the event is enabled before
they work out the arguments.

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

## Attributes

An attribute is a { key, value } pair carried by an event or by the type
of a field. They are written where C writes them: trailing, after the
thing they belong to. A field's follow its type in brackets, an event's
follow its field list.

```rust
define_event!(
    request,
    provider: "rust",
    event: "request",
    level: SIDE_LOGLEVEL_INFO,
    fields: (
        id: u32,
        code: u32 [side_attr("std.integer.base", side_attr_u8(16))],
    ),
    attributes: [side_attr("std.event.note", side_attr_string("about the event"))],
);
```

The value constructors are the ones C has: `side_attr_bool()`,
`side_attr_u8()` through `_u64()`, `side_attr_s8()` through `_s64()`,
`side_attr_string()` and `side_attr_null()`. Floats and 128-bit
integers are in the ABI and not yet here.

Two things become of an attribute which reaches LTTng-UST. It is carried
into the CTF2 metadata as a user attribute, the key splitting at its
last dot into a namespace and a name, so `std.integer.base` arrives as

```json
"attributes": { "std.integer": { "base": 16 } }
```

which happens whether or not anything understands it. And a few are
understood: `std.integer.base` -- 2, 8, 10 or 16 -- becomes the CTF2
`preferred-display-base`, so `lttng view` prints the field in that base.
`std.blob.media-type` and `lttng.fmt.print-value` are the others, and
want a byte array and an enumeration, neither of which this crate can
describe yet.

A structure keeps its attributes with its definition rather than at a
field which reads through it, as it does in C, so
`side_field_gather_struct()` takes none and neither does a field of a
`#[derive(SideGather)]` type here. `examples/attributes.rs` is the whole
of it.

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

side_event!(trace::process_started, 0, &process);
```

The events are written exactly as they are on their own; the module is
the only new thing. `examples/provider.rs` is three events sharing two
structures: **2640 bytes of description ungrouped, 1552 grouped**, the
same behaviour and no relocation either way.

What is shared is the description of a structure, which is the same
bytes wherever it is used; the 64 byte type at each point of use still
carries its own offset and access mode, exactly as
`side_static_define_struct` and `side_field_gather_struct` divide the
work in C.

The two benchmarks weigh the two halves of it:
`tests/benchmark/description-sharing` what grouping a thousand events
saves, and `tests/benchmark/event-cost` what one event costs to emit.

Two structures which describe the same way *are* the same description,
so what is compared is the shape rather than which Rust type it came
from. Merging them is right, not a coincidence to be avoided.

All the events of a group register with one call, and each becomes a
module of its own name holding `enabled()` and `emit()`. `side_event!()`
is what reaches them:

```rust
side_event!(trace::process_started, 0, &process);
```

It is one macro for every event rather than one per event: the path is
written at the call site and resolves there, which is what lets it name
neither the module the event lives in nor the crate. Any way of writing
that path works -- `trace::process_started`,
`crate::trace::process_started`, or `process_started` where it has been
brought in with `use`.

It asks whether the event is enabled before it works out the arguments,
so one which costs something, or has an effect of its own, is not
reached at all while nothing is listening. That is what `tracepoint()`
and `side_event()` are macros for, and it is why a group generates no
function of the event's own name: there is no shorter spelling which
quietly evaluates its arguments.

It also says that half of the branch is the unlikely one, so the
emission is laid out past the return rather than between the work a
function does and its end. What an instrumented function runs while
nothing is listening is a load, a test, and a branch it does not take:

```
.LBB12_8:
        mov   rcx, qword ptr [rip + ...STATE_0+8]
        test  rcx, rcx
        jne   .LBB12_9                ; the emission, laid out below
.LBB12_10:
        xor   eax, 1515870810         ; the rest of the function
        add   rsp, 48
        pop   rbx
        ret
```

The load is a relaxed atomic one, which is how `side_event_enabled()`
reads it in C: a tracer writes that word from another thread, and it is
read here rather than remembered.

`examples/asmcheck.rs` is where that came from, and carries the command
which regenerates it. It has both forms, since an event of a group and
one declared on its own compile differently.

Where the answer is wanted for something else as well, the two halves
are public and can be used apart:

```rust
if trace::process_started::enabled() {
    trace::process_started::emit(seq, &render(&body));
}
```

A standalone `define_event!` keeps its own macro and its named
arguments; grouping is additive, and nothing changes for an event which
does not want it.

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
the bytes are laid out with a *hole* where each address belongs, and the
object built around them names a pointer at every hole:

```rust
#[repr(C, packed)]
struct GroupDesc {
    run_0: [u8; RUN_0],
    ptr_0: SideRawPtr,      // a pointer written as a pointer
    run_1: [u8; RUN_1],
    ...
}
```

A pointer written as a pointer is a relocation. The loader fills it in
before anything of the program runs, and -- what the group's own
constructor writing the address could never give -- a reader of the
*file* can follow it, so `readside` walks into the fields of a structure
described elsewhere.

**Which is why it is worth measuring before reaching for it.** Two
groups of twenty events over the same structure:

| | description | pages private |
|---|---|---|
| `side_extern(PROCESS_INFO)` | 10226 bytes, 40 relocations | 3 of 3 |
| each group describing it | 11032 bytes, no relocation | 2 of 4 |

806 bytes saved, and every page of the descriptions private to the
process instead of shared between them. (The two on the duplicating side
are its neighbours in the section's end pages, not the descriptions.)
Within one group it is worse still: the group already shares a structure
two of its events describe the same way, so `side_extern()` there buys
nothing at all and only costs the pages.

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
description itself costs none, unless it names a structure described
elsewhere: `side_extern()` is one relocation per reference, and buys
what is said above. This holds under `--release`, under fat LTO and
under `--gc-sections`.

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
