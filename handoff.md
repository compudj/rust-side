# side POC handoff, 2026-09-02

Working note, not part of the build. The tracked history is the record;
this is what is in flight, what was decided, and what was learnt. It
started as a rust-side note and now covers four trees, because the work
moved.

## Where the four trees are

    libside      f3aee21    Tell a tracer when a statedump has been taken
    lttng-ust    c848464d   Answer for the statedump this session asked for
    lttng-tools  f92f1ad28  sessiond: Take the notification
    rust-side    50c2954    Dump the state a tracer which started late has no

NOTHING IS UNCOMMITTED anywhere. Unpushed as of the last check:
libside 2, lttng-ust 4, lttng-tools 1, rust-side 0. Check with
`git rev-list --count @{u}..HEAD` rather than trusting this line.

Build everything with

    export PATH=$HOME/.cargo/bin:$PATH          # rustup is not on PATH
    export PKG_CONFIG_PATH=$HOME/git/libside/src/lib/pkgconfig
    export UST_LIB=$HOME/git/lttng-ust/src/lib/lttng-ust/.libs/liblttng-ust.so.1

WHAT IS INSTALLED UNDER /usr/local IS NOW BEHIND BOTH TREES. The
libside there predates the statedump query and completion commits
(79cf078, f3aee21) and does not export their symbols; the lttng-ust
there is ABI 11, from before the major bump. Preload the build trees,
or stage them (see below).

To run rust-side examples against the libside BUILD tree rather than the
install, `LD_LIBRARY_PATH=$HOME/git/libside/src/lib/libside/.libs`.

## Running a session daemon which speaks the new protocol

/usr/local is not writable and its lttng-tools is ABI 11, so the
end-to-end tests used a throwaway stage plus a worktree build. The
recipe, which took several tries to get right:

    STAGE=<scratch>/ust-stage/usr/local
    (cd ~/git/libside   && make install DESTDIR=<scratch>/ust-stage)
    (cd ~/git/lttng-ust && make install DESTDIR=<scratch>/ust-stage)

THE STAGED .la FILES MUST BE REWRITTEN or libtool ignores the stage:
each carries `libdir='/usr/local/lib'`, and libtool uses THAT, not the
-L path, so the link silently picks the old /usr/local library up. Fix
`libdir=`, and rewrite `/usr/local/lib/<name>.la` ONLY for the names
actually present in the stage -- a blanket rewrite breaks librseq and
liburcu, which are genuinely installed there. Beware when checking the
result: the staged path itself CONTAINS `/usr/local/lib`, so a naive
grep reports false positives; filter on the stage prefix.

Then, lttng-tools in a `git worktree` (the main tree is configured
in-tree, so a VPATH build is refused), configured with UST_CFLAGS /
UST_LIBS / UST_CTL_CFLAGS / UST_CTL_LIBS pointing at the stage, and
built with `make LDFLAGS="-L$STAGE/lib -Wl,-rpath,$STAGE/lib"` so the
stage precedes the `-L/usr/local/lib` which liburcu drags in.

TWO TRAPS WHEN RUNNING IT: the scratch path is too long for a unix
socket (108 chars), so LTTNG_HOME needs a short symlink into it; and
the user's own ABI 11 sessiond keeps running throughout, harmlessly,
because the major bump makes it refuse the new applications outright.

# DONE THIS SESSION: the lttng-ust half of statedump completion

All four lttng-ust commits are verified end to end against a session
daemon built from the tree, with a C application registering a side
statedump callback in each mode.

1. `22c549b2` **ABI major 11 -> 12**, oldest-compatible with it. The
   branch already owed this: `8c5794a9` added `nr_event_attributes`
   without one. A major bump rather than a new command within 11,
   because ust-abi.h only allows extending sessiond->app, where the app
   can reply "unknown command"; there is no symmetric tolerance
   app->sessiond, and an unknown notify command desynchronizes the
   stream rather than erroring.

2. `895599ac` **The query.**
   `LTTNG_UST_ABI_SESSION_STATEDUMP_OUTSTANDING` (0x55) and
   `lttng_ust_ctl_statedump_outstanding()`. No protocol struct: every
   reply already carries a ret_val, which prepare_cmd_reply() fills
   from a non-negative command return. The `statedump_pending ||` half
   is not optional; it closes the window before REGISTER_DONE.

3. `9c63d3ac` **The push notification.**
   `LTTNG_UST_CTL_NOTIFY_CMD_STATEDUMP` carrying the session objd and a
   status, TAKEN or DROPPED. lttng-ust registers a libside completion
   callback which does nothing but `ust_listener_wakeup_all()`; each
   listener loops to the top and re-derives, under ust_lock, which of
   its sessions are no longer outstanding. A per-session obligation
   (`statedump_notify`: NONE / OWED / DROPPED) is what distinguishes
   "quiet because it was taken" from "quiet because none was asked".
   Stop reports DROPPED; destroy says nothing, its objd being released.
   The ust lock is NOT held across the send.

4. `c848464d` **Fix found by testing: the query answered for every
   tracer.** `side_tracer_statedump_request_pending()` reports a
   MATCH_ALL dump -- which registering a callback queues -- as pending
   for EVERY key, by design. So after stop, a polling application which
   never polled left the query saying "outstanding" forever, while the
   notification had already said DROPPED. A per-session
   `side_statedump_issued` bit, set on issue and cleared on cancel,
   gates the question. The two halves now agree.

lttng-tools `f92f1ad28` **sessiond: take the notification.** Without it
`ust_app_recv_notify()` reaches `default: abort()` -- the new notify
command has no "unknown command" reply in that direction. It logs and
replies; NO POLICY, deliberately.

## What was measured

    agent thread     "statedump taken" 13 ms after lttng start
    polling, 8 s     "taken" withheld until the app polled: 5.8 s
    polling, never   "dropped" 0.3 ms after lttng stop, no bogus "taken"
    query after start   outstanding = true
    query after stop    outstanding = false (after the fix above)

# NEXT: ask from the client, do not block in the daemon

DECIDED THIS SESSION, replacing an earlier idea. The daemon does NOT
wait. `lttng start` and `lttng regenerate statedump` return
immediately, as today, and the CLI gets a way to ask whether the
statedump is complete, so waiting -- and any timeout -- is the user's.

WHY NOT A BOUNDED WAIT IN THE SESSIOND, which was tried and backed out:
both `cmd_start_trace()` and `cmd_regenerate_statedump()` run with the
per-session lock held AND with the global session list lock held --
client.cpp takes the list lock across _all_ session commands and says
so at the declaration of `list_lock`. Sleeping there stalls every
session in the daemon, not just this one. Sleeping under the RCU read
lock which the orchestrator loops hold is a second, separate hazard: it
holds off grace periods for the whole wait, including for the thread
which receives the very notifications being waited on.

THE STACK TO BUILD:

    lttng CLI  ->  liblttng-ctl  ->  new sessiond client command
               ->  domain orchestrator  ->  per app,
                   lttng_ust_ctl_statedump_outstanding()

The bottom of it is already proven: a temporary probe in
`_start_app_trace` / `_stop_app_trace` produced the query results in
the table above, then was reverted. The only piece of it kept back is a
ten-line `protocol_guard::is_statedump_outstanding(handle)` wrapper in
`ust-app-command-socket{.hpp,-protocol.cpp}`, which was written,
exercised, and then reverted with the probe -- rewrite it, it is
trivial.

OPEN, and the reason this stopped here: the CLI surface is unchosen.
A subcommand of its own, something folded into `lttng status`, or a
`--wait`/`--timeout` on the client side of `regenerate statedump`. The
public liblttng-ctl API name and its entry in `lttng-ctl.map` are the
same question, and both are one-way doors.

STILL TRUE, and worth keeping in the eventual commit message: the query
is per application, so the daemon aggregates -- one application which
polls rarely must not hold up "the session is done". And never use any
of this to extend the constructor semaphore: in polling mode the
application cannot reach `run_pending_statedumps()` until main() runs,
so waiting there is a guaranteed deadlock.

# DONE in earlier sessions

Newest first.

## libside: the statedump request query and completion callback

`79cf078` and `f3aee21`. The NEXT section above is what consumes them.

`side_tracer_statedump_request_pending(key)` is the LEVEL. It needed one
change to how a request is carried, because "still queued" is not "not
taken yet": `_side_statedump_run_pending_requests()` SPLICES the whole
queue into a stack-local list before running any of it (which is what
makes the running thread sole owner and lets callbacks run lock-free),
so for the whole time a statedump is being taken its request is
reachable from NOBODY, and a query walking the queue would report it
DONE at exactly the moment it is being written. The handle therefore has
a second list and each request is published on it for the time its
callback takes: queue -> in_flight -> run -> remove+free. Batch
semantics and sole ownership are unchanged; the cost is one lock
acquisition per request taken.
A MATCH_ALL dump -- which REGISTERING a callback queues -- reads as
outstanding for EVERY key, correctly: its events reach every tracer.
False also answers a cancelled request, a key never requested, and an
application with no statedump callback at all. It says NOTHING IS
OUTSTANDING, not that a dump happened.
Interrupted by fork(), a statedump is re-queued in the child (only
reachable in polling mode; the agent thread is paused between requests).

`side_tracer_statedump_completion_register(cb, priv)` is the EDGE.
DELIBERATELY A HINT, not a queue: one request fans out to every
registered handle, and MATCH_ALL dumps concern keys nobody named, so it
may fire when nothing a given tracer cares about changed. The query
stays authoritative. A tracer which re-derives cannot be made wrong by
an extra call; one which counted them could be.
WHAT IT GUARANTEES is what makes it usable: the request is off the
in-flight list BEFORE tracers are told, so a notified tracer which asks
is told "not outstanding". It runs on the dumping thread with NO side
lock held -- required, since the statedump locks are leaves below the
tracer control locks. The list is read under `statedump_rcu_gp`.
NOT fired on cancel: the only caller which can cancel is the tracer
which did, and it already knows.

`tests/unit/statedump-request.c`, 17 TAP tests, both modes, wired into
TESTS (suite is 89). TWO THINGS LEARNT WRITING IT:
- the agent-thread in-flight observation needs a TWO-WAY HANDSHAKE
  (callback signals entered, then waits to be released). "Wait for
  entered, then query" races and fails essentially always.
- VERIFY THE TEST IS NOT VACUOUS: removing the in-flight publication
  makes exactly that one assertion report "quiet" while dumping, and
  nothing else fails.

## lttng-ust: cancel the statedump a session will not use

`72bc4be7`. Nothing ever called
`side_tracer_statedump_request_cancel()`. Now `lttng_session_destroy()`
does (after `active = 0`, the counterpart of the key_alloc in
`lttng_session_create()`, covering all three destroy call sites at
once), and so does `lttng_session_disable()`, which is where a session
which merely STOPS arrives and is the one that matters in practice.
Safe because `lttng_session_enable()` calls `lttng_session_statedump()`
on every start, so a restart re-asks -- VERIFIED: stop+start gives
exactly two dumps.
BEST EFFORT, NOT A BARRIER: whoever runs the requests takes the whole
queue before running any, so a dump already picked up finishes; its
events are then discarded by the key filter and by `session->active`.
MEASURED with a C program registering a POLLING handle which does not
poll for 12 s while sessions cycle, counting callback entries: 3
start/stop 4->1 runs, 6 create/destroy 7->1. The remaining 1 is the
MATCH_ALL dump libside queues at REGISTRATION, which belongs to no
session and must not be cancelled.

## rust-side: the application statedump

`50c2954`. `#[libside::events(statedump = dump, mode = agent_thread |
polling)]` registers a group's callback from the SAME constructor which
registers its events, AFTER them -- registering the callback queues a
dump at once and, in agent thread mode, WAITS for it to run before
returning, so the events have to exist by then, and a group's
`.init_array` order against anything else's is undefined. That is the
whole reason the callback hangs off the group. Unregistration is the
reverse.

`side_statedump_event!(path, key, args...)` beside `side_event!()`,
reaching an `emit_statedump()` beside each event's `emit()`. It guards
on `enabled()` for the same reason but does NOT `cold_path()`: a dump
callback runs because a tracer asked, so the emission is the likely
half.

`StatedumpKey<'a>` is the safety: it borrows from the callback's call,
so it cannot be stored, and its raw pointer makes it !Send/!Sync.
Verified both refusals compile-fail. There is nowhere else to get one,
so an event cannot be dumped outside a dump.

MODE IS REQUIRED, no default -- a spawned thread and an obligation to
poll are both too loud to hand somebody quietly. `polling` gives the
group `statedump_pending()` and `run_pending_statedumps()`; agent thread
does not, so the wrong mode is a missing name rather than libside's
runtime `SIDE_ERROR_INVAL`. Both guard a null handle (registration can
fail; libside would deref it).

The group's MODULE NAME is the state name, which shows in
`side:statedump_begin/_end`.

THE FIRST DUMP RUNS IN THE CONSTRUCTOR, BEFORE MAIN, and finds the state
empty. That is correct and is said in the README rather than papered
over. `examples/statedump.rs` therefore runs for 10 s so a session can
be created against it; that run carried 3 tasks and 2 sessions into CTF.

`define_event!` on its own has NO statedump: registration is a group's.

CONSTRUCTOR-TIME HAZARD, documented by libside (`side.c:143-146`) and
not by the rust-side README: statedump provider register/unregister must
not be called holding `side_notification_lock` or a tracer control lock,
because `side_agent_thread_lock` waits on agent-thread progress. In
AGENT_THREAD mode registration spawns the thread and blocks until it has
run the first dump. The group registers from `.init_array` -- fine at
program startup (verified), sharp under `dlopen()` where the loader lock
is held across constructors.

## rust-side: extern references are relocations

`59a7a01`. The constructor which used to write the address of a
structure described elsewhere is gone. The bytes the const evaluator
lays out have a HOLE where each such address belongs, and the object
built around them is

    #[repr(C, packed)]
    struct GroupDesc {
        run_0: [u8; RUN_0],
        ptr_0: SideRawPtr,      // a pointer written as a pointer
        run_1: [u8; RUN_1],
        ...
    }

so the loader fills it in and A READER OF THE FILE CAN FOLLOW IT, which
is the whole point: `readside` now walks into the fields of a foreign
structure instead of a null pointer.

The proc macro does NOT duplicate the layout: it needs only the hole
offsets, which `Built<N, K>.patches` carries, and the count K, which is
the number of `side_extern` fields it already knows. `description_run()`
copies each run out of the blob; `PATCH_WIDTH` is the width of a hole.
`patch()` asserts the holes come out in increasing order and do not
overlap, which is what lets the macro name one pointer per hole in field
order. A `const _: () = assert!(size_of::<GroupDesc>() == SIZE)` catches
any drift between the two.

`#[repr(C, align(16))]` is gone, since packed and align cannot be
combined; byte alignment is what libside's packed structures want
anyway, and it makes descriptions slightly smaller.

`Layout::ExternStruct.target` SURVIVES even though no table is indexed
by it any more: it is the identity in the dedupe key, so two references
which describe the same way but name different structures are not
merged. `put_type()` ignores it (`target: _`).

### The numbers

Two groups of 20 events over one structure:

| | description | pages private |
|---|---|---|
| `side_extern(PROCESS_INFO)` | 10226 bytes, 40 relocations | 3 of 3 |
| each group describing it | 11032 bytes, 0 relocations | 2 of 4 |

MEASURE PRIVATE PAGES, NOT SOFT DIRTY. The loader applies relocations
before any constructor runs, so clearing `/proc/self/clear_refs` from a
constructor -- which is what the earlier measurements did -- happens
after the writes it was meant to catch and reports 0. Read
`/proc/self/pagemap` instead and count pages which are present (bit 63)
and no longer file backed (bit 61 clear). The `2 of 4` on the
duplicating side is neighbours sharing the section's end pages.

The scratch programs which measured this were DELETED with the commit;
the pagemap walk is quoted in the commit message.

Conclusion unchanged: `side_extern` costs relocations and private pages,
and is for crossing a crate and for type identity, not for saving
memory.

# The whole series, in order

1. Ported the crate to libside's self-relative pointers by const
   evaluating each description into one `[u8; N]`.
2. `#[libside::events]`: a module is a provider, its events laid out
   together so a type two of them describe the same way is described
   once.
3. `define_type!` / `side_extern()` for crossing a group or a crate.
4. `side_event!(path, args)`: one macro for every event, so arguments
   are not worked out while the event is disabled.
5. `core::hint::cold_path()` so the emission is the unlikely half.
6. The enabled word is an `AtomicUsize` read `Relaxed`, as C reads it.
7. Attributes on an event and on the type of a field.
8. Two benchmarks under `tests/benchmark/`.
9. libside: named types reachable across translation units again
   (`side_ptr_sel_t`, `side_extern()`, `side_declare_*`).
10. libside: readside applies relocations, and no longer crashes on a
    reference it cannot resolve.
11. rust-side: extern references written as real relocations.
12. rust-side: the application statedump, both modes, with a key which
    cannot outlive the dump.
13. lttng-ust: cancel the statedump a stopped or destroyed session will
    not use.
14. libside: ask whether a statedump is outstanding, and be told when
    one has been taken.
15. lttng-ust: the ABI major bump the branch owed, the outstanding
    query, the completion notification, and scoping the query to the
    session's own request.
16. lttng-tools: the session daemon takes the notification.

# Other ideas, none started

- **Variadic events.** `side_call_variadic` / the dynamic argument
  types are still entirely unreached from Rust; "Current scope" in the
  rust-side README has said "non-variadic events with fixed fields"
  from the start.
- **A type shared between two GROUPS without `side_extern`.** Still
  impossible (const eval will not turn a pointer into an integer);
  `side_extern` is the answer and costs relocations. Nothing to do
  unless the cost is judged wrong.
- **Attributes libside describes but Rust cannot yet reach**: floats
  and 128-bit integer attribute values, `std.blob.media-type` (needs a
  byte array type) and `lttng.fmt.print-value` (needs an enumeration).
- **Enumerations**, which nothing here describes at all.
