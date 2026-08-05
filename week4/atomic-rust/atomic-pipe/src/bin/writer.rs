//! One writer process for the aggregation demo.
//!
//! Spawned by demo/aggregate.sh as several concurrent processes that all inherit the
//! *same* pipe write end as stdout. Nothing here creates a pipe -- the shell did that.
//!
//!   framed: chunk() the payload and write each frame with its own write(2), every one
//!           at or under PIPE_BUF, so the kernel's all-or-nothing rule applies to each.
//!   raw:    one length-prefixed write_all of the whole payload, which is what you would
//!           write if you had not thought about PIPE_BUF. The kernel splits it and other
//!           writers land in the gaps.
//!
//! Both modes write to a dup of fd 1 as a File, never through std::io::Stdout --
//! Stdout is a LineWriter, and letting it choose flush boundaries would destroy the
//! frame sizing that the whole design depends on.

use atomic_pipe::payload;
use atomic_pipe::pipe::chunk;
use std::io::Write;
use std::os::fd::AsFd;
use std::process::ExitCode;

const USAGE: &str = "usage: writer --index N --bytes N [--mode framed|raw]";

fn main() -> ExitCode {
    let mut index: Option<u32> = None;
    let mut bytes: Option<usize> = None;
    let mut framed = true;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--index" | "--bytes" => {
                let Some(v) = args.get(i + 1) else {
                    eprintln!("{}: missing value\n{USAGE}", args[i]);
                    return ExitCode::from(2);
                };
                let Ok(n) = v.parse::<u64>() else {
                    eprintln!("{}: {v:?} is not a number\n{USAGE}", args[i]);
                    return ExitCode::from(2);
                };
                if args[i] == "--index" {
                    index = Some(n as u32);
                } else {
                    bytes = Some(n as usize);
                }
                i += 2;
            }
            "--mode" => {
                match args.get(i + 1).map(String::as_str) {
                    Some("framed") => framed = true,
                    Some("raw") => framed = false,
                    other => {
                        eprintln!("--mode: expected framed|raw, got {other:?}\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
                i += 2;
            }
            "--framed" => {
                framed = true;
                i += 1;
            }
            "--raw" => {
                framed = false;
                i += 1;
            }
            other => {
                eprintln!("unexpected argument {other:?}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let (Some(index), Some(bytes)) = (index, bytes) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let data = payload::build(index, bytes);

    // A dup of stdout, unbuffered. Dropping this closes our dup, not the real fd 1.
    let fd = match std::io::stdout().as_fd().try_clone_to_owned() {
        Ok(fd) => fd,
        Err(e) => {
            eprintln!("writer {index}: cannot dup stdout: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::fs::File::from(fd);

    let result = if framed {
        // The id is this writer's pid, so the reader reports real process identity and
        // the 22-bit id field is exercised the way it would be in practice.
        let pid = std::process::id();
        if pid >= 1 << 22 {
            eprintln!(
                "writer {index}: pid {pid} does not fit the 22-bit id field \
                 (max 4194303) -- check /proc/sys/kernel/pid_max"
            );
            return ExitCode::FAILURE;
        }
        write_framed(&mut out, pid, &data)
    } else {
        write_raw(&mut out, &data)
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        // EPIPE means the reader gave up and closed the read end -- which is exactly
        // what the raw reader does when it desyncs. That is a consequence of the
        // finding, not a separate failure, and four copies of it racing onto stderr
        // only obscures the real message. Stay quiet and let the reader do the talking.
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("writer {index}: write failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// One write(2) per frame, each frame <= PIPE_BUF, so each is atomic against the
/// other writers sharing this pipe.
fn write_framed(out: &mut impl Write, id: u32, data: &[u8]) -> std::io::Result<()> {
    for frame in chunk(id, data) {
        out.write_all(&frame)?;
    }
    Ok(())
}

/// The naive protocol: 4-byte big-endian length, then the payload, in one call.
/// Correct for a single writer. With concurrent writers the kernel splits it and the
/// reader's next "length" is really a word from the middle of somebody's payload.
fn write_raw(out: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    let mut framed = Vec::with_capacity(4 + data.len());
    framed.extend_from_slice(&(data.len() as u32).to_be_bytes());
    framed.extend_from_slice(data);
    out.write_all(&framed)
}
