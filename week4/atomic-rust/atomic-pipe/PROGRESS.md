# Atomic pipe — Rust port progress

Session: 2026-08-04 → 2026-08-05. Talk: Thursday 2026-08-06, 16:00.
Reference implementation: `week4/atomic-pipe/atomicv2.py`. Prior notes: `week4/atomic-pipe/RETRO.md`.

## Status: demo-ready

6/6 tests green, both demos run clean.

```
test_message              test_chunk_small        test_chunk_large
test_roundtrip_misaligned test_split_invariance   test_interleaved_writers
```

## Achieved

- **Framing encode** — `Message` + `convert()`. 3-byte header (22-bit id + last-flag),
  terminal frames carry an extra 2-byte length. Wire format pinned by a byte-exact test
  computed by hand, not derived from `convert()`.
- **`chunk()`** — splits a payload into continuation frames plus exactly one terminal frame.
- **Reassembler** — `Pipe::read()` with `tail_buffer` / `accumulator` / `ready_buffer`,
  ported from the Python state split.
- **Real pipe I/O** — `std::io::pipe` (stable since 1.87), so `rustix` was never needed and
  the dependency is gone. `libc` is in only for `fork`/`waitpid`/`getpid`/`_exit`.
- **`close_write`** via `Option<PipeWriter>` — "closed" is a type-level state, not a
  convention. `Drop` closes the fd; there is no `close()`.
- **Constants derived from one number** — `PIPE_BUF` → `PAYLOAD_MAX = PIPE_BUF - HEADER - SIZE_FIELD`.
- **Test evidence**
  - split-invariance at 15 read sizes including **1 byte per read** and every frame boundary ±1
  - two writers' frames hand-interleaved frame-by-frame, read 7 bytes at a time
- **Demos** — 4 forked processes × 400 KB each. Framed: all 4 intact, all 4 split mid-stream
  by other writers (3156 frames). Raw control with no framing: 6 runs where 4 were written.

## The finding

**macOS `PIPE_BUF` is 512, not 4096.** 4096 is the Linux value; the design assumed it.

Frames were 4094 bytes, so on this machine *none of them were atomic*. Single-writer tests
passed (nothing to interleave with) and a 10 KB concurrent run passed (payloads fit the pipe
buffer, so no child ever blocked and they ran effectively serially). Raising N to 400 KB
forced children to block mid-write, and a message arrived with 17219 bytes instead of 400000
— 4 full payloads plus 855, i.e. the parser had found a "terminal frame" mid-stream.

Confirmed with `getconf PIPE_BUF /tmp` → 512 and `sys/syslimits.h:100`.

Found by a test, not by reading a header. Fixing it meant editing six hardcoded literals.

## What went well

- **Types drove better designs than the Python had.** `payload: &'a [u8]` removed a copy per
  chunk. `Option<PipeWriter>` made write-after-close unrepresentable instead of a runtime
  error on a stale fd. A diverging `_exit` arm in the fork loop deleted the "which process am
  I" branch that Python needed in two places.
- **Deferring the cursor commit until a frame is confirmed** — carried over from the Python
  retro, held up under every read size.
- **Writing the byte-exact wire-format test before the parser existed.** Header byte order was
  never in question afterward, so parser bugs stayed diagnosable as parser bugs.
- **Empiricism beat reading.** The `PIPE_BUF` assumption survived two days of reasoning and
  died to one test run.

## Mistakes

- **Sized array vs unsized slice** — `[T; N]` vs `[T]` tripped things up four separate times
  (3-byte header, payload, `u32::from_be_bytes`, `u16::from_be_bytes`). Rule learned: range
  indexing always yields `[T]` with no length; reaching `[T; N]` needs an array literal or a
  fallible `try_into()`.
- **`Vec::with_capacity(4096)` for the read buffer.** `read` fills up to `len()`, not capacity,
  so it silently read 0 bytes forever. Capacity is reserved room; length is elements that exist.
- **Assumed `read()` returns whole frames because writes are atomic.** `PIPE_BUF` constrains
  writes only; a pipe has no framing. This is *why* `tail_buffer` exists — if reads returned
  frames, none of the reassembly code would be needed.
- **Parent's `close_write()` inside the fork loop.** Ran on iteration 0, so children 1–3
  inherited `w: None` and only child 0 could write. It also serialized the demo — fork, drain,
  reap, repeat — so nothing could interleave.
- **Terminal frame in an `else` branch.** Any payload over `PAYLOAD_MAX` emitted continuation
  frames and no terminal frame, so the reader would accumulate forever.
- **Whole-buffer `len()` where remaining-from-`ptx` was meant** — bug #3 from the Python retro,
  reintroduced once. Could have panicked on an out-of-bounds index.
- **`drain(..ptx)` inside the parse loop**, leaving `ptx` stale against a shifted buffer —
  bug #2 from the retro, also reintroduced.
- **Magic numbers in six places.** Turned a one-line platform fix into a multi-site edit. The
  retro predicted this exact shape and it still happened.
- **Serial-vs-interleaved conflated.** A first attempt at measuring interleaving counted id
  switches; 4 strictly serial writers produce 3 switches. The real test is whether a writer's
  frames are *contiguous* in arrival order.

## Next

Correctness gaps, in priority order:

1. **Truncation detection** — accumulator non-empty at EOF currently `panic!`s. Should be a
   typed error the caller can handle.
2. **Error handling** — `read()` uses `.expect()`; signature should be
   `io::Result<Option<(u32, Vec<u8>)>>`. Same for `write()`, which currently asserts on the
   write count instead of reporting a partial write.
3. **`size == 0`** means "give up" rather than "empty message". Relatedly, `chunk()` emits zero
   frames for an empty payload, so `write(id, b"")` silently vanishes.
4. **id validation** — only 22 bits survive encoding. On Linux, `pid_max` defaults to exactly
   2^22, so a pid at the boundary would corrupt the last-flag. Validate or assign ids directly.
5. **Runtime `PIPE_BUF`** via `fpathconf(fd, _PC_PIPE_BUF)`. The constant belongs to the pipe,
   not the program — so these become fields, not `const`s.
6. **Extract `Reassembler` from `Pipe`** — a pure state machine with no fds. The `read_size`
   field is the cheap stand-in that made the sweep testable; the clean split is better.
7. **Run the demo on Linux** to exercise the 4096 path and confirm the constants are the only
   platform coupling.

Cleanup: dead `if self.tail_buffer.len() < 1` branch; stale header comment in `pipe.rs`
referencing the old 4191/4194 numbers.

## Process note

Ground rules: `systems-programming/policy.md` (guide, don't fix; no implementation code).
Waived narrowly and explicitly for test harnesses and instrumentation under deadline —
`frame_order`, `read_size`, `test_split_invariance`, `test_interleaved_writers`, and the demo
harness in `main.rs`. Framing, chunking, and the reassembly loop are own work.

Schedule held this time: the Wednesday 14:00 → 18:30 code box was met, unlike the Python phase
where a 3-hour Sunday estimate ran into Tuesday.
