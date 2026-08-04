"""
Deterministic tests for the atomic pipe reassembler.

Two independent gates:

  test 1 (split invariance) - the parser must produce identical messages no
      matter how the byte stream is sliced across read() calls. This is the
      property that all the alignment bugs violate.

  test 2 (size sweep) - chunk() must never emit a frame larger than PIPE_BUF,
      and its frames must round-trip back to the original payload.

Run: python3 test_atomicv2.py
"""

import os
import time

from atomicv2 import Pipe, chunk

# The atomicity budget the design assumes. Linux is 4096; other platforms
# differ (macOS is 512), which is why this is named rather than inlined.
LIMIT = 4096

ID_MASK = (1 << 22) - 1


class StreamExhausted(Exception):
    """The parser asked for more bytes than the test stream contains."""


# ---------------------------------------------------------------------------
# frame oracle - deliberately a second, independent implementation of the
# header format, so a bug in the parser cannot mask itself in the test.
# ---------------------------------------------------------------------------


def frame_is_last(frame: bytes) -> bool:
    return bool((frame[0] >> 6) & 1)


def frame_id(frame: bytes) -> int:
    return int.from_bytes(frame[0:3], "big") & ID_MASK


def frame_payload(frame: bytes) -> bytes:
    if frame_is_last(frame):
        size = int.from_bytes(frame[3:5], "big")
        return frame[5 : 5 + size]
    return frame[3:]


# ---------------------------------------------------------------------------
# harness
# ---------------------------------------------------------------------------


def make_fake_read(stream: bytes, chunk_size: int):
    """A stand-in for os.read that hands out `stream` in fixed-size slices.

    Raises instead of returning b"" so an under-producing parser fails loudly
    rather than spinning on the EOF path.
    """
    pos = 0

    def fake_read(_fd, n):
        nonlocal pos
        if pos >= len(stream):
            raise StreamExhausted(
                f"parser consumed all {len(stream)} bytes without "
                f"producing the expected messages"
            )
        end = min(pos + min(chunk_size, n), len(stream))
        out = stream[pos:end]
        pos = end
        return out

    return fake_read


def build_stream(specs):
    """specs: [(writer_id, payload_bytes), ...]

    Frames from every writer are interleaved round-robin, which is what the
    kernel would hand you under real concurrency. Returns the byte stream plus
    the messages in the order they should complete - a message completes when
    its terminal frame lands, so completion order follows the stream, not the
    order the writers were listed.
    """
    per_writer = [(wid, payload, chunk(wid, payload)) for wid, payload in specs]

    stream = bytearray()
    expected = []
    idx = 0
    while any(idx < len(frames) for _, _, frames in per_writer):
        for wid, payload, frames in per_writer:
            if idx < len(frames):
                stream.extend(frames[idx])
                if idx == len(frames) - 1:
                    expected.append((wid, payload))
        idx += 1

    return bytes(stream), expected


def drain(stream: bytes, count: int, chunk_size: int):
    """Feed `stream` to a Pipe in `chunk_size` slices, pull `count` messages."""
    p = Pipe()
    real_read = os.read
    os.read = make_fake_read(stream, chunk_size)
    try:
        got = []
        for _ in range(count):
            wid, payload = p.read()
            # the accumulator holds lists of ints; normalise for comparison
            got.append((wid, bytes(payload)))
        return got
    finally:
        os.read = real_read
        p.close_read()
        p.close_write()


# ---------------------------------------------------------------------------
# test 1 - split invariance
# ---------------------------------------------------------------------------

SPECS = [
    (101, b"A" * 1),  # single terminal frame, minimum size
    (102, b"B" * 4092),  # terminal frame at the payload boundary
    (103, b"C" * 8200),  # two continuations + a small terminal
    (104, b"D" * 4093),  # one continuation + a 1-byte terminal
]

SPLIT_SIZES = [1, 2, 3, 4, 5, 7, 13, 64, 511, 1000, 4091, 4092, 4095, 4096, 8192]


def test_split_invariance():
    stream, expected = build_stream(SPECS)
    print(f"test 1: split invariance ({len(stream)} bytes, {len(expected)} messages)")

    failures = []
    for cs in SPLIT_SIZES:
        started = time.monotonic()
        try:
            got = drain(stream, len(expected), cs)
        except StreamExhausted as e:
            failures.append((cs, f"under-produced: {e}"))
            print(f"  chunk={cs:<5} FAIL  under-produced")
            continue
        except Exception as e:
            failures.append((cs, f"{type(e).__name__}: {e}"))
            print(f"  chunk={cs:<5} FAIL  {type(e).__name__}: {e}")
            continue

        elapsed = time.monotonic() - started
        if got != expected:
            detail = describe_mismatch(expected, got)
            failures.append((cs, detail))
            print(f"  chunk={cs:<5} FAIL  {detail}")
        else:
            print(f"  chunk={cs:<5} ok    ({elapsed:.2f}s)")

    return failures


def describe_mismatch(expected, got):
    if len(expected) != len(got):
        return f"expected {len(expected)} messages, got {len(got)}"
    for i, (exp, act) in enumerate(zip(expected, got)):
        if exp[0] != act[0]:
            return f"message {i}: expected writer {exp[0]}, got {act[0]}"
        if exp[1] != act[1]:
            return (
                f"message {i} (writer {exp[0]}): expected {len(exp[1])} bytes, "
                f"got {len(act[1])}"
            )
    return "unknown mismatch"


# ---------------------------------------------------------------------------
# test 2 - size sweep through chunk()
# ---------------------------------------------------------------------------

SWEEP_ID = 4242

SWEEP_SIZES = (
    [1, 2, 3, 7, 100]
    + list(range(4088, 4101))  # first chunk boundary
    + list(range(8180, 8193))  # second chunk boundary
    + [10000, 12275, 12276, 12277]
)


def test_size_sweep():
    print(f"\ntest 2: size sweep through chunk() (limit={LIMIT})")

    failures = []
    for n in SWEEP_SIZES:
        payload = bytes([(n % 26) + 65]) * n
        frames = chunk(SWEEP_ID, payload)
        problems = []

        # a) every frame must fit the atomicity budget, or the kernel is free
        #    to interleave another writer's bytes inside it
        oversize = [(i, len(f)) for i, f in enumerate(frames) if len(f) > LIMIT]
        if oversize:
            problems.append(
                "frames over PIPE_BUF: " + ", ".join(f"#{i}={ln}" for i, ln in oversize)
            )

        # b) exactly one terminal frame, and it must be last
        last_flags = [frame_is_last(f) for f in frames]
        if last_flags.count(True) != 1:
            problems.append(f"{last_flags.count(True)} terminal frames, expected 1")
        elif not last_flags[-1]:
            problems.append("terminal frame is not the final frame")

        # c) every frame carries the writer id
        bad_ids = {frame_id(f) for f in frames} - {SWEEP_ID}
        if bad_ids:
            problems.append(f"unexpected ids in frames: {sorted(bad_ids)}")

        # d) frames must round-trip back to the original payload
        joined = b"".join(frame_payload(f) for f in frames)
        if joined != payload:
            problems.append(f"payload round-trip: {len(joined)} bytes, want {n}")

        # e) and the parser must agree
        if not problems:
            try:
                got = drain(b"".join(frames), 1, LIMIT)
                if got != [(SWEEP_ID, payload)]:
                    problems.append("parser disagreed with the oracle")
            except Exception as e:
                problems.append(f"parser raised {type(e).__name__}: {e}")

        if problems:
            failures.append((n, problems))
            print(f"  n={n:<6} FAIL  {'; '.join(problems)}")

    if not failures:
        print(f"  all {len(SWEEP_SIZES)} sizes ok")

    return failures


# ---------------------------------------------------------------------------


def platform_pipe_buf():
    r, w = os.pipe()
    try:
        return os.fpathconf(r, "PC_PIPE_BUF")
    except (OSError, ValueError, AttributeError):
        return None
    finally:
        os.close(r)
        os.close(w)


def main():
    actual = platform_pipe_buf()
    if actual is not None and actual != LIMIT:
        print(
            f"warning: this platform reports PIPE_BUF={actual}, but the tests "
            f"assume {LIMIT}. Writes above {actual} are not atomic here.\n"
        )

    f1 = test_split_invariance()
    f2 = test_size_sweep()

    print()
    if not f1 and not f2:
        print("PASS - both gates green")
        return 0

    print(f"FAIL - {len(f1)} split failures, {len(f2)} sweep failures")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
