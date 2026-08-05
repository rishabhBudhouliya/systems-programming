//! Act 2: what does atomicity actually cost?
//!
//! Four rows, and the third and fourth are the point:
//!
//!   raw, 64 KiB writes   -- unconstrained baseline; what a pipe can do
//!   raw, PIPE_BUF writes -- same bytes, same absence of framing, but forced into
//!                           small writes. Isolates the cost of the *constraint*.
//!   atomic, read 512     -- the framed pipe as pipe.rs shipped it
//!   atomic, read 64 KiB  -- the framed pipe with a sane read size
//!
//! Row 2 vs row 4 is the honest measure of what the framing and reassembly code costs,
//! because both do one write(2) per PIPE_BUF-ish chunk. Comparing row 1 to row 4
//! instead would blame the parser for the syscall floor that PIPE_BUF imposes on any
//! correct implementation.
//!
//! Rows 3 and 4 differ only in read_size. Nothing about PIPE_BUF constrains reads --
//! only writes -- and test_split_invariance already proves reassembly is independent of
//! where read(2) cuts the stream, so the gap between them is free throughput.

use atomic_pipe::pipe::{Pipe, chunk};
use atomic_pipe::probe;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

const USAGE: &str = "usage: bench [--total-bytes N] [--message-bytes N] [--reps N]";

struct Row {
    label: &'static str,
    elapsed: Duration,
    bytes: usize,
    writes: usize,
    reads: usize,
    overhead_pct: f64,
}

fn main() {
    let mut total_bytes = 256 << 20; // 256 MiB
    let mut message_bytes = 1 << 20; // 1 MiB per framed message
    let mut reps = 5usize;
    let mut sweep = false;
    let mut only_atomic = false;
    let mut read_size = 65536usize;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--sweep-read-size" {
            sweep = true;
            i += 1;
            continue;
        }
        // Machine-readable single number, for the side-by-side with Perl.
        if args[i] == "--only-atomic" {
            only_atomic = true;
            i += 1;
            continue;
        }
        let Some(v) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) else {
            eprintln!("{}: expected a number\n{USAGE}", args[i]);
            std::process::exit(2);
        };
        match args[i].as_str() {
            "--total-bytes" => total_bytes = v,
            "--message-bytes" => message_bytes = v,
            "--reps" => reps = v,
            "--read-size" => read_size = v,
            other => {
                eprintln!("unexpected argument {other:?}\n{USAGE}");
                std::process::exit(2);
            }
        }
        i += 2;
    }

    if cfg!(debug_assertions) {
        eprintln!("warning: debug build. Run with --release or the numbers are meaningless.\n");
    }

    if only_atomic {
        let d = median(reps, || run_atomic(total_bytes, message_bytes, read_size));
        println!("{:.1}", total_bytes as f64 / d.as_secs_f64() / 1e6);
        return;
    }

    let (probe_r, _probe_w) = std::io::pipe().expect("pipe");
    let pipe_buf = probe::runtime_pipe_buf(&probe_r).unwrap_or(512);
    let compiled = probe::compiled_pipe_buf();

    println!("atomic-pipe throughput");
    println!("  {}", probe::report(&probe_r));
    println!(
        "  total {} MiB, framed message {} KiB, {} reps (median reported)",
        total_bytes >> 20,
        message_bytes >> 10,
        reps
    );
    println!();

    // Exact frame count for one framed message, so the syscall column is measured
    // from the real chunker rather than estimated.
    let frames_per_message = chunk(1, &vec![0u8; message_bytes]).len();
    let messages = total_bytes / message_bytes;
    let framed_writes = frames_per_message * messages;
    let framed_wire = (total_bytes as f64 / (compiled - 2) as f64) * 3.0;

    let big = 64 << 10;

    if sweep {
        // read_size only affects how read(2) cuts the stream, which
        // test_split_invariance proves reassembly is immune to. So this column should
        // rise monotonically as syscalls fall -- and where it does not, the reader is
        // paying a per-call cost proportional to read_size rather than to bytes moved.
        println!("  atomic pipe, read_size sweep");
        println!("  {:<14} {:>9} {:>12}", "read_size", "MB/s", "read(2)");
        for rs in [512usize, 1024, 4096, 16384, 65536, 262144] {
            let d = median(reps, || run_atomic(total_bytes, message_bytes, rs));
            println!(
                "  {:<14} {:>9.0} {:>12}",
                rs,
                total_bytes as f64 / d.as_secs_f64() / 1e6,
                total_bytes.div_ceil(rs)
            );
        }
        println!();
        return;
    }

    let rows = vec![
        Row {
            label: "raw pipe, 64 KiB writes",
            elapsed: median(reps, || run_raw(total_bytes, big, big)),
            bytes: total_bytes,
            writes: total_bytes.div_ceil(big),
            reads: total_bytes.div_ceil(big),
            overhead_pct: 0.0,
        },
        Row {
            label: "raw pipe, PIPE_BUF writes",
            elapsed: median(reps, || run_raw(total_bytes, pipe_buf, big)),
            bytes: total_bytes,
            writes: total_bytes.div_ceil(pipe_buf),
            reads: total_bytes.div_ceil(big),
            overhead_pct: 0.0,
        },
        Row {
            label: "atomic pipe, read 512",
            elapsed: median(reps, || run_atomic(total_bytes, message_bytes, 512)),
            bytes: total_bytes,
            writes: framed_writes,
            reads: total_bytes.div_ceil(512),
            overhead_pct: framed_wire / total_bytes as f64 * 100.0,
        },
        Row {
            label: "atomic pipe, read 64 KiB",
            elapsed: median(reps, || run_atomic(total_bytes, message_bytes, big)),
            bytes: total_bytes,
            writes: framed_writes,
            reads: total_bytes.div_ceil(big),
            overhead_pct: framed_wire / total_bytes as f64 * 100.0,
        },
    ];

    println!(
        "  {:<28} {:>9} {:>12} {:>12} {:>9}",
        "", "MB/s", "write(2)", "read(2)", "wire"
    );
    for row in &rows {
        let mbps = row.bytes as f64 / row.elapsed.as_secs_f64() / 1e6;
        println!(
            "  {:<28} {:>9.0} {:>12} {:>12} {:>8.2}%",
            row.label, mbps, row.writes, row.reads, row.overhead_pct
        );
    }

    let baseline = rate(&rows[0]);
    let constrained = rate(&rows[1]);
    let framed = rate(&rows[3]);
    println!();
    println!(
        "  cost of the PIPE_BUF constraint alone: {:.2}x  ({:.0} -> {:.0} MB/s)",
        baseline / constrained,
        baseline,
        constrained
    );
    println!(
        "  cost of framing + reassembly on top:   {:.2}x  ({:.0} -> {:.0} MB/s)",
        constrained / framed,
        constrained,
        framed
    );
    println!(
        "  read_size 512 -> 64 KiB is worth:      {:.2}x  ({:.0} -> {:.0} MB/s)",
        framed / rate(&rows[2]),
        rate(&rows[2]),
        framed
    );
}

fn rate(r: &Row) -> f64 {
    r.bytes as f64 / r.elapsed.as_secs_f64() / 1e6
}

/// One warmup, then `reps` timed runs, median. Pipe throughput is scheduler-sensitive
/// enough that a mean over few runs is misleading.
fn median(reps: usize, mut f: impl FnMut() -> Duration) -> Duration {
    f();
    let mut runs: Vec<Duration> = (0..reps.max(1)).map(|_| f()).collect();
    runs.sort();
    runs[runs.len() / 2]
}

fn reap() {
    let mut status = 0;
    unsafe { libc::waitpid(-1, &mut status, 0) };
}

/// Unframed bytes through a plain pipe, written `chunk_size` at a time.
fn run_raw(total: usize, chunk_size: usize, read_size: usize) -> Duration {
    let (mut r, w) = std::io::pipe().expect("pipe");
    let mut w = Some(w);

    let start = Instant::now();
    if unsafe { libc::fork() } == 0 {
        let buf = vec![0x5au8; chunk_size];
        let mut left = total;
        let out = w.as_mut().unwrap();
        while left > 0 {
            let n = left.min(chunk_size);
            if out.write_all(&buf[..n]).is_err() {
                unsafe { libc::_exit(1) }
            }
            left -= n;
        }
        unsafe { libc::_exit(0) }
    }
    drop(w.take());

    let mut buf = vec![0u8; read_size];
    let mut got = 0usize;
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => panic!("read failed: {e}"),
        }
    }
    let elapsed = start.elapsed();
    reap();
    assert_eq!(total, got, "raw: short read");
    elapsed
}

/// The same bytes as framed messages through Pipe.
fn run_atomic(total: usize, message_bytes: usize, read_size: usize) -> Duration {
    let messages = total / message_bytes;
    let mut p = Pipe::new().expect("Pipe::new");
    p.read_size = read_size;

    let start = Instant::now();
    if unsafe { libc::fork() } == 0 {
        let buf = vec![0x5au8; message_bytes];
        let pid = std::process::id();
        for _ in 0..messages {
            p.write(pid, &buf);
        }
        p.close_write();
        unsafe { libc::_exit(0) }
    }
    p.close_write();

    let mut got = 0usize;
    while let Some((_id, msg)) = p.read() {
        got += msg.len();
    }
    let elapsed = start.elapsed();
    reap();
    assert_eq!(messages * message_bytes, got, "atomic: short read");
    elapsed
}
