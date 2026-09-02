# What grouping events saves

A libside description is laid out into one object and every distance
within it is between two bytes of that object, so two events are two
objects and a type used by both is described twice.
`#[libside::events]` on a module lays its events out together, and a
structure two of them describe the same way is then described once.

This weighs that, for 1000 events described both ways:

    ./measure
    NR_EVENTS=100 ./measure

`measure` writes the programs, builds them, reads their sections, and
runs each one under a probe which reports what the process holds.

## The four programs

|  | fields | grouped |
|---|---|---|
| `shared-grouped` | `seq: u32, process: &ProcessInfo` | yes |
| `shared-ungrouped` | the same | no |
| `scalar-grouped` | `seq: u32, when: u64, delta: i32, ok: bool` | yes |
| `scalar-ungrouped` | the same | no |

`ProcessInfo` holds four scalars, a nested structure and an array of
four of them, which is what there is to share. The scalar pair shares
nothing at all and is there to show the floor: grouping shares type
descriptions and nothing else, so where there is no type to describe
there is nothing for it to do.

The programs contain nothing else. `main()` returns immediately, so
every section is attributable to the instrumentation, and everything
that happens has happened by the time the report is written: the events
are registered by constructors, and a tracer preloaded alongside reads
every description as it is registered.

`gen-instrumentation` writes them into `generated/`, a crate of its own
which depends on this one by path, so that building this crate never
builds them.

## What the numbers mean

**bytes** is what the linker emitted, which is address space and disk.

**pages held** is how many pages of `side_event_description` this
process ever faulted in, out of how many it maps. It is the other
question, and the more interesting one: a description which is mapped
and never read costs a running process nothing at all, because demand
paging never fetches it.

**dirtied** is how many of those the process wrote. A clean page is
backed by the page cache and paid once for the whole system however
many processes map it; a dirty one has been copied and is paid in full
by every process. A description holds no address, so nothing writes to
it and it stays clean: what a tracer makes resident is shared, not
private.

**rss** is the whole process, which is the number to quote and the
number to distrust: it moves with everything else the program does.

## What it says

For 1000 events whose fields reach a structure:

|  | grouped | ungrouped | saved |
|---|---:|---:|---:|
| file | 883,152 | 2,486,104 | 64% |
| `side_event_description` | 231,712 | 1,056,000 | 78% |
| relocations into it | 0 | 0 | |
| pages held, nothing listening | 2 of 57 | 2 of 259 | |
| pages held, tracer reading | 57 of 57 | 259 of 259 | |
| of those, dirtied | 1 | 1 | |
| rss, tracer reading | 2600 kB | 3748 kB | 30% |

and for the same 1000 events of scalar fields:

|  | grouped | ungrouped | saved |
|---|---:|---:|---:|
| `side_event_description` | 380,896 | 384,000 | 0% |
| pages held, tracer reading | 94 of 94 | 94 of 94 | |

Three things are worth reading out of that.

**A description nobody reads costs a process nothing.** Two pages of
either program are ever faulted in while nothing is listening, out of
57 and out of 259. What instrumentation nobody enables costs is a file,
not a resident page, whichever way it is written.

**The saving arrives when a tracer does read it**, and it is the whole
section: 202 pages, 808 kB, and 30% off the process. It is clean and
shared rather than private and dirty, so it is paid once for the system
rather than once per process -- but it is paid, and it is what the
grouping removes.

**Grouping shares type descriptions and nothing else.** The scalar pair
saves nothing in the description and holds the same 94 pages either way.
That it still cuts the file by 43% and the process by 9% is a different
saving, and not this one: a thousand events written on their own are a
thousand constructors and a thousand registration calls, where a group
is one of each.

## How it is measured

Residency comes from `/proc/self/pagemap` and not from `mincore()`,
which for a file backed mapping answers whether the page is in the page
cache -- true of a file just built, whether or not the process ever
touched it.

`pagemap-probe.c` is preloaded rather than linked into the programs,
because those same binaries are what the section sizes are read from:
anything added to them would be counted as instrumentation. It holds
both moments worth reading. A constructor which runs before the
program's own clears the soft dirty bits, so that what is reported as
dirtied is what this process wrote. A destructor reports, which is after
the events are registered and after a tracer has read them.

The tracer is libside's console tracer, found with `pkg-config`, or
named by `SIDE_CONSOLE_TRACER`. Without one the last three rows are left
out, since nothing would ever read a description.
