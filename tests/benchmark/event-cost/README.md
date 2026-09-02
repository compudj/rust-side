# What one event costs

What emitting an event from Rust costs, next to the same event emitted
through libside's C API. The event carries a single 32-bit integer,
which is the smallest payload that still has to be described, filtered
and serialized, and the loop around it is the same in all three
programs, so what is left between the columns is what each frontend puts
between the call site and `side_call()`.

    lttng-sessiond --daemonize
    ./run-benchmark

## The three programs

| | how the event is written | how it is called |
|---|---|---|
| `bench-side.c` | `side_static_event()` | a macro |
| `bench-macro` | `define_event!` | a macro |
| `bench-group` | `define_event!` in `#[libside::events]` | a function |

The two Rust programs are there because grouping changes the call site:
an event on its own gets a macro which tests whether it is enabled where
it is written, and one in a group gets a function which tests inside
itself. Both are `#[inline(always)]`, and the question is whether that
is enough.

None of them links the tracer. `run-benchmark` preloads it, which is how
libside instrumentation is used.

## Four cases

| case | how |
|---|---|
| disabled | no session at all, so the event has no subscriber |
| recorded | `enable-event`, no filter |
| filter `1 == 0` | a filter which rejects without reading the payload |
| filter on the field | `v == 4000000000`, which the loop never emits, so the filter reads the field and rejects every instance |

The two filter cases separate the cost of *reaching* the event from the
cost of *recording* it, and `1 == 0` against a filter which reads the
field separates the interpreter's field load from the rest.

## What it says

Nanoseconds per event, median of seven repetitions, one machine, one
run:

| case | C | Rust macro | Rust group |
|---|---:|---:|---:|
| disabled | 0.23 | 0.24 | 0.23 |
| recorded to ring buffer | 132.27 | 133.57 | 132.86 |
| filter `1 == 0` | 17.30 | 17.78 | 17.77 |
| filter on the payload field | 22.94 | 23.47 | 23.56 |

Over two runs every difference falls between −2.4 and +1.3 ns, and the
only one which keeps its sign is the filtered path, where Rust is half a
nanosecond to a nanosecond behind. Nothing separates the two Rust
programs: the function a group generates costs what the macro costs, so
grouping is free at the call site as well as cheaper in memory.

## How it is measured

**The programs are interleaved.** For each case, one repetition of each
program in turn, over and over, rather than all the repetitions of one
program before the next. This matters more than it sounds: measured the
other way, one run had C fastest in every case and the next had it
slowest in every case, by 30%. A machine which drifts -- and one which
scales its frequency always does -- moves every column of a repetition
together, which leaves the comparison between them standing; running one
program to completion first puts the drift *between* the columns, where
it cannot be told apart from what is being measured.

**Each case is checked before anything is timed.** A short run goes into
a session which writes to disk, and the events which came out are
counted: the unfiltered case must record every one, and both filtered
cases none. An event which is silently not enabled measures as a very
fast disabled path, and counting is the only way to tell the two apart.
`--no-verify` skips it.

The rest is the usual: a snapshot session, so the sub-buffers are
overwrite buffers and nothing is written to disk while the loop runs; a
warmup loop to prime the ring buffer pages and settle the caches and the
branch predictors; `taskset` to pin; and the median of the repetitions,
with the range they spanned beside it.

## Environment

| variable | default | |
|---|---|---|
| `BENCH_ITERS` | 5000000 | events in a timed repetition |
| `BENCH_WARMUP` | 1000000 | events in the warmup loop |
| `BENCH_REPS` | 7 | repetitions, of which the median is reported |
| `BENCH_CPU` | 3 | CPU to pin to |
| `BENCH_VERIFY_ITERS` | 2000 | events in the run which is counted |
| `UST_LIB` | what `pkg-config` finds | tracer to preload |

Compare the columns of one run rather than one column across runs.

## Note

What the absolute figures are made of is mostly not what is being
measured here. The cost of a filter fetching fields, and the cost of
recording to the ring buffer, are deliberately unoptimized in the side
path: the bytecode specializer leaves field loads on the generic dynamic
`LOAD_FIELD`, and serialization walks the description with a two-pass
visitor. Those are the same for every column, which is what makes the
columns comparable; they are not a difference between the frontends and
this benchmark says nothing about them.

`tests/benchmark/side-instrumentation` in the LTTng-UST tree is where
libside is measured against a tracepoint, which is the other question.
