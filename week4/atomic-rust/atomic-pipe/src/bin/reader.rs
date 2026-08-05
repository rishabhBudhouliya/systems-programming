//! The aggregating parent for the demo. Reads whatever the shell piped into stdin.
//!
//! The check that matters is per writer, not over the whole stream. Interleaving
//! reorders bytes but never loses them, so a total byte count or a total byte sum comes
//! out identical whether or not writes were torn -- a raw pipe would pass such a check
//! and prove nothing. Each payload instead carries its writer's index and is filled
//! from a stream derived from that index, so it can only verify if that writer's bytes
//! arrived contiguous and unmixed.
//!
//!   framed: hand stdin to Pipe and take (id, payload) pairs off it.
//!   raw:    the naive protocol -- read a 4-byte length, then that many bytes. Correct
//!           for one writer, and it desyncs the instant a concurrent write is torn,
//!           because the next "length" is really a word from inside someone's payload.

use atomic_pipe::payload::{self, Verdict};
use atomic_pipe::probe;
use std::collections::BTreeMap;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "usage: reader --bytes N --writers N [--mode framed|raw]";

fn main() -> ExitCode {
    let mut bytes: Option<usize> = None;
    let mut writers: Option<usize> = None;
    let mut framed = true;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bytes" | "--writers" => {
                let Some(n) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) else {
                    eprintln!("{}: expected a number\n{USAGE}", args[i]);
                    return ExitCode::from(2);
                };
                if args[i] == "--bytes" {
                    bytes = Some(n);
                } else {
                    writers = Some(n);
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

    let (Some(bytes), Some(writers)) = (bytes, writers) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    println!("  {}", probe::report(std::io::stdin()));

    let results = if framed {
        read_framed(bytes)
    } else {
        read_raw(bytes)
    };

    report(results, writers, bytes)
}

/// One received message: which writer it claims to be from, and whether it holds up.
struct Received {
    id: Option<u32>,
    verdict: Verdict,
}

#[cfg(feature = "stdio-pipe")]
fn read_framed(expect_len: usize) -> Vec<Received> {
    let mut p = match atomic_pipe::pipe::Pipe::reader_on_stdin() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot read stdin as a pipe: {e}");
            std::process::exit(1);
        }
    };

    let mut out = Vec::new();
    while let Some((id, msg)) = p.read() {
        out.push(Received {
            id: Some(id),
            verdict: payload::verify(&msg, expect_len),
        });
    }
    out
}

/// Framed mode needs Pipe to be constructible over an inherited fd. Until
/// Pipe::reader_on_stdin() exists, say so plainly rather than failing to build.
#[cfg(not(feature = "stdio-pipe"))]
fn read_framed(_expect_len: usize) -> Vec<Received> {
    eprintln!(
        "framed mode needs Pipe::reader_on_stdin(), which src/pipe.rs does not have yet.\n\
         See demo/HANDOFF.md. Once it exists:\n\
         \n    cargo build --release --features stdio-pipe\n"
    );
    std::process::exit(1);
}

/// The naive length-prefixed protocol, implemented the obvious way.
fn read_raw(expect_len: usize) -> Vec<Received> {
    let mut stdin = std::io::stdin().lock();
    let mut out = Vec::new();

    // A torn write makes the next "length" a payload-interior word, which is usually
    // enormous. Refuse to trust it rather than trying to allocate gigabytes -- the
    // desync is the finding, and it should be reported, not turned into an OOM.
    let sane_max = expect_len.saturating_mul(4).max(1 << 20);

    loop {
        let mut len_buf = [0u8; 4];
        match stdin.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                eprintln!("  raw: read failed: {e}");
                break;
            }
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > sane_max {
            println!(
                "  raw: desync -- next length reads as {len} bytes (max plausible \
                 {sane_max}). A write was torn and the parser is now misaligned."
            );
            break;
        }

        let mut buf = vec![0u8; len];
        if let Err(e) = stdin.read_exact(&mut buf) {
            println!("  raw: desync -- wanted {len} bytes, stream ended early ({e})");
            break;
        }
        out.push(Received {
            id: None,
            verdict: payload::verify(&buf, expect_len),
        });
    }
    out
}

fn report(results: Vec<Received>, writers: usize, bytes: usize) -> ExitCode {
    let mut seen: BTreeMap<u32, bool> = BTreeMap::new();
    let mut bad = 0usize;

    for r in &results {
        let tag = match r.id {
            Some(id) => format!("id {id:>7}"),
            None => "           ".to_string(),
        };
        let mark = if r.verdict.is_ok() { "ok  " } else { "FAIL" };
        println!("  {mark}  {tag}  {}", r.verdict.describe());

        if let Verdict::Ok { index } = r.verdict {
            seen.insert(index, true);
        } else {
            bad += 1;
        }
    }

    println!();
    println!(
        "  {} message(s), {} verified, {} bad; {} of {} writers accounted for",
        results.len(),
        results.len() - bad,
        bad,
        seen.len(),
        writers
    );

    let complete = seen.len() == writers && bad == 0 && results.len() == writers;
    if complete {
        println!(
            "  PASS: every writer's {bytes} bytes arrived intact and attributable\n"
        );
        ExitCode::SUCCESS
    } else {
        println!("  FAIL: the aggregation is wrong -- messages were torn or lost\n");
        ExitCode::FAILURE
    }
}
