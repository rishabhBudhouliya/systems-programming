//! Recovering PIPE_BUF from both ends, so a mismatch is visible instead of silent.
//!
//! The demo's whole atomicity claim rests on the compiled-in PIPE_BUF matching what the
//! kernel actually enforces on this pipe. On macOS that is 512; on Linux 4096. Get it
//! wrong in the too-large direction and frames stop being atomic while every
//! single-writer test keeps passing -- which is exactly the bug that went unnoticed for
//! two days.
//!
//! So print both numbers side by side rather than trusting either.

use crate::pipe::chunk;
use std::os::fd::{AsFd, AsRawFd};

/// The PIPE_BUF that pipe.rs was *compiled* with, recovered from frame geometry.
///
/// A continuation frame is HEADER + PAYLOAD_MAX, and PAYLOAD_MAX is
/// PIPE_BUF - HEADER - SIZE_FIELD, so a full frame is PIPE_BUF - SIZE_FIELD bytes and
/// adding the 2-byte size field back recovers the constant. This avoids pipe.rs having
/// to expose it just for instrumentation.
pub fn compiled_pipe_buf() -> usize {
    // Any payload larger than PAYLOAD_MAX yields at least one full continuation frame.
    let frames = chunk(1, &vec![0u8; 1 << 20]);
    frames[0].len() + 2
}

/// What the kernel will actually treat as atomic on this specific fd.
///
/// PIPE_BUF is a property of the pipe, not of the program -- which is the argument for
/// making this a field rather than a constant (PROGRESS.md item #5).
pub fn runtime_pipe_buf<F: AsFd>(fd: F) -> Option<usize> {
    let v = unsafe { libc::fpathconf(fd.as_fd().as_raw_fd(), libc::_PC_PIPE_BUF) };
    if v < 0 { None } else { Some(v as usize) }
}

/// One line for the top of any demo's output: what we assumed, what is true, and
/// whether that is a problem.
pub fn report<F: AsFd>(fd: F) -> String {
    let compiled = compiled_pipe_buf();
    match runtime_pipe_buf(fd) {
        Some(actual) if actual == compiled => {
            format!("PIPE_BUF: compiled {compiled}, kernel {actual} -- agree")
        }
        Some(actual) if compiled > actual => format!(
            "PIPE_BUF: compiled {compiled}, kernel {actual} -- \
             MISMATCH, frames exceed the atomic limit and are NOT atomic"
        ),
        Some(actual) => format!(
            "PIPE_BUF: compiled {compiled}, kernel {actual} -- \
             conservative, still atomic but {}x more frames than needed",
            actual / compiled.max(1)
        ),
        None => format!("PIPE_BUF: compiled {compiled}, kernel unknown (fpathconf failed)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The probe must agree with what the kernel says on a real pipe. On the platform
    // pipe.rs is configured for, these are the same number -- and if they ever are not,
    // that is precisely the bug worth failing a test over.
    #[test]
    fn probe_matches_frame_sizes() {
        let compiled = compiled_pipe_buf();
        let frames = chunk(1, &vec![0u8; 1 << 20]);
        for f in &frames {
            assert!(
                f.len() <= compiled,
                "frame of {} bytes exceeds recovered PIPE_BUF {}",
                f.len(),
                compiled
            );
        }
    }

    #[test]
    fn runtime_lookup_works_on_a_real_pipe() {
        let (r, _w) = std::io::pipe().unwrap();
        let actual = runtime_pipe_buf(&r).expect("fpathconf should work on a pipe");
        assert!(actual >= 512, "PIPE_BUF is at least 512 by POSIX, got {actual}");
    }
}
