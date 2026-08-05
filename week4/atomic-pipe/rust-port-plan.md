# Bugs
The tests deliberately never exercise EOF (the fake reader raises instead), so:

1. EOF still breaks into popleft() on an empty deque → IndexError at the end of every clean run. Your concurrent test will hit this on the first try.
2. Truncation detection — accumulator non-empty at EOF — doesn't exist.
3. Concurrency has never actually run. Nothing here proves the atomicity guarantee end-to-end; it proves the framing is capable of it.
4. size == 0 still means "give up" rather than "empty message."
5. write() still doesn't check its return value (your TODO).

None of those are parser bugs. They're the API edges, and Rust is the better place to get them right the first time.


# Plan for the two days

Today: frame encode/decode + chunk() as pure functions, then the reassembler as a pure state machine — bytes in, messages out, no file descriptors. Port test 1 as a property test; it's the single highest-value thing you have. Derive every field offset from one frame_start and never mutate a cursor mid-frame.

Wednesday morning: real pipe I/O, threads (not fork), the concurrent stress demo. Handle EINTR explicitly. Blocking mode only.

Wednesday afternoon: presentation. Protect it.



# Designing the frame in Rust
Two types of frame:
1) Continuation frame: header (3 bytes) + payload (4091 bytes)
2) Terminal frame: header (3 bytes) + size (2 bytes) + payload (4091 bytes)

Continuation frame
Total bytes a frame can ever have is 4096 bytes, that means [u8; 4096] at max in other words?
