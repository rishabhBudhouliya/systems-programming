# What's left, and it's all in `src/pipe.rs`

Everything outside `src/pipe.rs` is done and building. Two of the three acts run today.
One change unblocks the third.

## Blocking — act 1 needs this

### `Pipe::reader_on_stdin()`

`Pipe::new()` always calls `pipe()` internally, so a `Pipe` can only ever wrap a pipe it
created. Act 1 needs one that wraps the read end **the shell** created and handed over as
stdin.

What it needs to do:

- Get an `OwnedFd` for stdin: `std::io::stdin().as_fd().try_clone_to_owned()?`.
  Use `try_clone_to_owned`, **not** `OwnedFd::from_raw_fd(0)` — it dups, so when the
  `Pipe` drops it closes your dup rather than the process's real stdin.
- `impl From<OwnedFd> for io::PipeReader` is stable, so `PipeReader::from(fd)` finishes it.
- `r: Some(...)`, `w: None`. The reader has no write end, and `w: None` already means
  "closed" in this design, so that falls out for free.
- Same state initialisation as `new()` otherwise.

A `writer_on_stdout()` counterpart would be symmetric, but **nothing needs it** —
`src/bin/writer.rs` calls `chunk()` directly and writes frames to a dup of fd 1, so it
already works. Add it only if you want the API to look complete.

Then:

```
cargo build --release --features stdio-pipe
./demo/aggregate.sh framed
```

The `stdio-pipe` feature only gates the framed path in `src/bin/reader.rs`. Once the
constructor exists you can delete the feature from `Cargo.toml` and the two `#[cfg]`
attributes in `reader.rs`, and everything builds unconditionally.

## Required for Linux — one line

### `PIPE_BUF` per platform (`pipe.rs:15`)

Currently hardcoded `512`, the macOS value. On Linux it is 4096. Make it
`cfg!(target_os = "macos")` → 512, else 4096.

Leaving it at 512 on Linux is *safe* — 512 ≤ 4096, so frames stay atomic — but you emit
8× more frames than necessary and the throughput numbers understate the design badly.

Every demo binary prints the compiled constant next to the kernel's runtime
`fpathconf(fd, _PC_PIPE_BUF)`, so a mismatch is visible in the output instead of silent.
`src/probe.rs` recovers the compiled value from `chunk()`'s frame geometry, which is why
`PIPE_BUF` does not need to be `pub`.

## Correction: `read_size` is **not** the win I claimed

The plan said raising `read_size` from 512 to 65536 was "the highest-value line change."
**That was wrong, and the benchmark says so.** Measured on macOS, 64 MiB, median of 5:

```
read_size        MB/s      read(2)
512              1039       131072
1024              907        65536
4096              949        16384
16384             781         4096
65536             913         1024
262144           1044          256
```

Read syscalls vary by **512×** across that sweep and throughput barely moves — it is flat
around 800–1100 MB/s with no trend. The reader is not read-syscall-bound at all. It is
bound by copying: every byte is moved kernel → `buffer` → `tail_buffer` → `accumulator`,
three times, plus a `vec![0u8; read_size]` heap allocation and zeroing **per pump**
(`pipe.rs:95`, inside the loop).

So:

- **Do not** raise `read_size` on my say-so. It changes nothing here and made things
  slightly worse at 16 KiB.
- **Do** re-run the sweep on the Linux box before deciding — `PIPE_BUF` is 4096 there and
  pipe capacity is 64 KiB rather than 16 KiB, so the curve may genuinely differ:
  ```
  ./target/release/bench --sweep-read-size --total-bytes 67108864 --reps 5
  ```
- If you want a real throughput win, the per-pump allocation is the thing to attack —
  hoist that `vec!` to a reusable `Pipe` field. That is worth doing *before* touching
  `read_size`, because a larger read buffer makes the per-pump allocation *more*
  expensive, not less.

This is a better talk beat than the original claim anyway: an obvious optimisation,
measured, and it wasn't one.

## Optional, only if you have time

- **Truncation as an error, not a panic** (`pipe.rs:104`). EOF with a non-empty
  accumulator currently `panic!`s. Act 1's raw mode and any killed writer reach that path,
  and a backtrace on the projector is bad optics.
- **id validation.** Assert `id < (1 << 22)` in `write()`/`chunk()`. `src/bin/writer.rs`
  already checks its own pid and refuses to run if it would overflow, so this is
  belt-and-braces — but it belongs in the library, not the demo.

## Everything else is done

| file | what | status |
|---|---|---|
| `demo/preflight.sh` | toolchain, `PIPE_BUF`, `pid_max`, Perl checks | run it first on the box |
| `src/payload.rs` | per-writer verifiable payloads + 4 tests | green |
| `src/probe.rs` | compiled vs runtime `PIPE_BUF` + 2 tests | green |
| `src/bin/writer.rs` | act 1 writer, framed and raw | works |
| `src/bin/reader.rs` | act 1 aggregator | raw works; framed gated |
| `demo/aggregate.sh` | act 1 driver | raw works; framed blocked |
| `src/bin/bench.rs` | act 2, four rows + read-size sweep | works |
| `demo/bench_perl.pl` / `.sh` | act 3, matched workload | needs `cpanm Atomic::Pipe` |

`cargo test` is 12 green: your 6, plus 6 I added for the new modules.
