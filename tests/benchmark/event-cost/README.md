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
| `bench-group` | `define_event!` in `#[libside::events]` | `side_event!` |

The two Rust programs are there because grouping changes what the call
site expands to. An event on its own gets a macro of its own; one in a
group is reached through `side_event!`, the single macro which serves
every event, and lands on two functions -- `enabled()` and `emit()` --
rather than on one. Both ask before they work out the arguments, and
everything is `#[inline(always)]`; the question is whether that is
enough.

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

What each Rust program costs over the C one, in nanoseconds per event,
as the median of the paired differences over 21 repetitions with the
interval which holds it 95 times out of a hundred:

| case | Rust macro | Rust group |
|---|---|---|
| disabled | −0.01 [−0.03, +0.00] 15/21 | −0.02 [−0.03, +0.01] 14/21 |
| recorded to ring buffer | +0.99 [−0.48, +2.01] 12/21 | +0.79 [−1.32, +2.04] 14/21 |
| filter `1 == 0` | **+0.58 [+0.41, +0.64] 18/21** | **+0.52 [+0.41, +0.66] 18/21** |
| filter on the payload field | **+0.59 [+0.48, +0.70] 19/21** | **+0.58 [+0.47, +0.73] 19/21** |

Two of the four are resolved, and they say the same thing: reaching a
filter from Rust costs **a little over half a nanosecond** more than
reaching it from the C macro. Nothing separates the two Rust programs,
so the function a group generates costs what the macro costs: grouping
is free at the call site as well as cheaper in memory.

The other two are not resolved, and the table says so rather than
letting a number be read as a finding. Disabled is a predicted branch in
all three and there is nothing there to find. Recording is dominated by
the ring buffer, whose spread swamps anything the frontend could
contribute; see below for what it would take.

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

**The difference is taken a repetition at a time.** Repetition *r* of
every program ran under the same conditions, so the pair cancels the
drift they share; the figure reported is the median of those
differences, not the difference of the two medians. It is worth about a
factor of two: for the filter on the field, seven repetitions place the
paired figure to ±0.13 ns and the unpaired one to ±0.26.

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
| `BENCH_REPS` | 21 | repetitions (see below) |
| `BENCH_CPU` | 3 | CPU to pin to |
| `BENCH_VERIFY_ITERS` | 2000 | events in the run which is counted |
| `UST_LIB` | what `pkg-config` finds | tracer to preload |

Compare the columns of one run rather than one column across runs.

## How many repetitions

Enough that the interval does not span zero, which depends entirely on
the case. One repetition of the paired difference has this much spread:

| case | spread of one difference | 21 repetitions place it to |
|---|---:|---:|
| disabled | 0.08 | ±0.03 |
| filter `1 == 0` | 0.56 | ±0.12 |
| filter on the payload field | 0.54 | ±0.12 |
| recorded to ring buffer | 4.53 | ±1.4 |

Seven, which is what this used to default to, leaves three of the four
spanning zero: it resolves the constant filter and nothing else. The
default is 21, which resolves both filter cases, and the two which
remain unresolved there are unresolved for reasons more repetitions do
not fix.

The width falls as 1 / sqrt(n), so bringing the recorded case to ±0.5 ns
would take around 165 repetitions and some seven minutes of that case
alone. It is not worth it: the ring buffer is the same code under all
three columns, the frontends cannot be what makes it vary, and a
difference of that size in a 130 ns path is not what anyone chooses a
frontend on.

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
