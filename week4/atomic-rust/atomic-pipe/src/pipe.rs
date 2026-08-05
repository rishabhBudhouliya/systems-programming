/*
 * Steps:
 * 1) add the rustix dependency
 * 2) start writing the pipe code
 * 3) add a test file that tests the pipe code
 */
use std::collections::{HashMap, VecDeque};
use std::io::PipeReader;
use std::io::PipeWriter;
use std::io::Read;
use std::io::Result;
use std::io::Write;
use std::io::pipe;

const PIPE_BUF: usize = 512;
const HEADER: usize = 3;
const SIZE_FIELD: usize = 2;
const PAYLOAD_MAX: usize = PIPE_BUF - HEADER - SIZE_FIELD;

struct Message<'a> {
    // process id, u32 should fit 2^22 ids
    id: u32,
    last: u8,
    payload: &'a [u8],
}

// returned Message is tired to this payload
impl<'a> Message<'a> {
    pub fn new(id: u32, last: u8, payload: &'a [u8]) -> Message<'a> {
        Message { id, last, payload }
    }

    // think of the frame buffer as a Vec<u8> instead of a fixed sized array
    pub fn convert(&self) -> Vec<u8> {
        let word = (self.id | ((self.last as u32 & 1) << 22)).to_be_bytes();
        if self.last & 1 == 1 {
            let size = (self.payload.len() as u16).to_be_bytes();
            let mut buffer = Vec::with_capacity(self.payload.len() + 5);
            // we need to slice the first byte to keep the 3 bytes of header (u32 is 4 bytes)
            buffer.extend(&word[1..]);
            buffer.extend(size);
            buffer.extend(self.payload);
            return buffer;
        } else {
            let mut buffer = Vec::with_capacity(PIPE_BUF);
            // we need to slice the first byte to keep 3 bytes
            buffer.extend(&word[1..]);
            buffer.extend(self.payload);
            return buffer;
        }
    }
}

pub struct Pipe {
    r: Option<PipeReader>,
    w: Option<PipeWriter>,
    tail_buffer: Vec<u8>,
    accumulator: HashMap<u32, Vec<u8>>,
    ready_buffer: VecDeque<(u32, Vec<u8>)>,
    // Instrumentation only: the id of every frame in the order it was parsed off
    // the wire. If writers actually interleaved, this alternates between ids.
    pub frame_order: Vec<u32>,
    // Bytes requested per underlying read(2). Frame boundaries are independent of
    // this, so correctness must not depend on it -- tests sweep it.
    pub read_size: usize,
}

impl Pipe {
    pub fn new() -> Result<Pipe> {
        let (r, w) = pipe().expect("couldn't create an os pipe");
        Ok(Pipe {
            r: Some(r),
            w: Some(w),
            tail_buffer: Vec::new(),
            accumulator: HashMap::new(),
            ready_buffer: VecDeque::new(),
            frame_order: Vec::new(),
            read_size: PIPE_BUF,
        })
    }

    // we need to think about two apis, read and write
    pub fn write(&mut self, id: u32, data: &[u8]) {
        // step 1: chunk the data stream
        let messages = chunk(id, data);
        for message in messages {
            let written_len = self.w.as_mut().unwrap().write(&message);
            assert_eq!(written_len.unwrap(), message.len());
        }
    }

    pub fn read(&mut self) -> Option<(u32, Vec<u8>)> {
        // step 1: read a certain size of buffer into memory and put it in the tail buffer
        while self.ready_buffer.len() == 0 {
            let mut buffer = vec![0u8; self.read_size];
            let n = self
                .r
                .as_mut()
                .unwrap()
                .read(&mut buffer)
                .expect("read failed");
            if n == 0 {
                // EOF
                if self.accumulator.len() != 0 {
                    panic!("EOF before processing accumulator");
                }
                return None;
            }
            self.tail_buffer.extend(&buffer[..n]);
            // step 2: maintain invariant: anything coming out of the tail buffer must be a complete frame, be it
            // continutation or terminal
            let mut ptx = 0;
            while ptx < self.tail_buffer.len() {
                if self.tail_buffer.len() < 1 {
                    // nothing to extract
                    // is EOF as well
                    break;
                }
                let first_byte = self.tail_buffer[ptx];
                let last_bit = first_byte >> 6 == 1;
                if self.tail_buffer[ptx..].len() < 3 {
                    // doesn't have a header worth of data
                    break;
                }
                if !last_bit {
                    // it's a continuation frame
                    // first determine if we have a complete frame worth of data, i.e, 3 + 4191 = 4194 bytes
                    let remaining = self.tail_buffer.len() - ptx;
                    let frame_len = HEADER + PAYLOAD_MAX;
                    if remaining < frame_len {
                        break;
                    }
                    let header = u32::from_be_bytes([
                        0,
                        self.tail_buffer[ptx],
                        self.tail_buffer[ptx + 1],
                        self.tail_buffer[ptx + 2],
                    ]);
                    let id = header & ((1 << 22) - 1);
                    ptx += 3;
                    let payload = &self.tail_buffer[ptx..ptx + PAYLOAD_MAX];
                    self.accumulator.entry(id).or_default().extend(payload);
                    ptx += PAYLOAD_MAX;
                    self.frame_order.push(id);
                } else {
                    let remaining = self.tail_buffer.len() - ptx;
                    if remaining < 5 {
                        // we need a 5 byte worth of header + size data atleast
                        break;
                    }
                    let header = u32::from_be_bytes([
                        0,
                        self.tail_buffer[ptx],
                        self.tail_buffer[ptx + 1],
                        self.tail_buffer[ptx + 2],
                    ]);
                    let id = header & ((1 << 22) - 1);
                    let size = (u16::from_be_bytes([
                        self.tail_buffer[ptx + 3],
                        self.tail_buffer[ptx + 4],
                    ])) as usize;
                    if size == 0 {
                        break;
                    }
                    if remaining < 5 + size {
                        // don't have enough data to read
                        break;
                    }
                    ptx += 5;
                    let payload = &self.tail_buffer[ptx..ptx + size];
                    self.accumulator.entry(id).or_default().extend(payload);
                    ptx += size;
                    self.frame_order.push(id);
                    self.ready_buffer
                        .push_back((id, self.accumulator.remove(&id).unwrap()));
                    // now we need to drain the tail buffer and accumulator
                }
            }
            self.tail_buffer.drain(..ptx);
        }
        return self.ready_buffer.pop_front();
    }

    pub fn close_read(&mut self) {
        drop(self.r.take());
    }

    pub fn close_write(&mut self) {
        drop(self.w.take())
    }
}

/*
 * Should be able to read a stream of bytes (data) and use the given identifier (pid)
 * to construct a collection of frames (messages)
 */
pub fn chunk(pid: u32, mut data: &[u8]) -> Vec<Vec<u8>> {
    // create continuation frames and terminal frames
    let mut messages: Vec<Vec<u8>> = Vec::new();
    let mut remaining = data.len();
    while remaining > PAYLOAD_MAX {
        let m = Message::new(pid, 0, &data[..PAYLOAD_MAX]);
        messages.push(m.convert());
        data = &data[PAYLOAD_MAX..];
        remaining = data.len();
    }
    if remaining != 0 {
        let m = Message::new(pid, 1, &data);
        messages.push(m.convert());
    }
    return messages;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message() {
        let id = 22561;
        let payload = vec![65u8; 10];
        let m = Message::new(id, 0, &payload);
        assert_eq!(22561, m.id)
    }

    #[test]
    fn test_chunk_small() {
        let id = 22561;
        let payload = vec![65u8; 10];
        let frames = chunk(id, &payload);
        assert_eq!(1, frames.len());
        assert_eq!(15, frames[0].len());

        // Wire format, byte for byte. Worked out by hand, not derived from convert():
        //   id     = 22561        = 0x005821
        //   last   = 1            -> bit 22 set  => word = 0x405821
        //   header = 3 bytes big-endian          => 40 58 21
        //   size   = 10           = 2 bytes b.e. => 00 0a
        //   payload= 10 x 'A'                    => 41 ...
        let mut expected: Vec<u8> = vec![0x40, 0x58, 0x21, 0x00, 0x0a];
        expected.extend(vec![0x41u8; 10]);
        assert_eq!(expected, frames[0]);
    }

    #[test]
    fn test_chunk_large() {
        let id = 22561;
        const N: usize = 10000;
        let payload = vec![65u8; N];
        let frames = chunk(id, &payload);

        // Derived from the constants, not hardcoded: every continuation frame is
        // full (it carries no length field), so the terminal frame takes the rest.
        let conts = (N - 1) / PAYLOAD_MAX;
        let tail = N - conts * PAYLOAD_MAX;
        assert_eq!(conts + 1, frames.len());
        for f in &frames[..conts] {
            assert_eq!(HEADER + PAYLOAD_MAX, f.len());
        }
        assert_eq!(HEADER + SIZE_FIELD + tail, frames[conts].len());

        // The guarantee the whole project exists for: no single write() exceeds PIPE_BUF.
        for f in &frames {
            assert!(
                f.len() <= PIPE_BUF,
                "frame of {} bytes exceeds PIPE_BUF {}",
                f.len(),
                PIPE_BUF
            );
        }

        // Exactly one terminal frame, and it is the last one.
        // The last-bit is bit 22 of the 3-byte header == bit 6 of header byte 0.
        let last_bits: Vec<u8> = frames.iter().map(|f| (f[0] >> 6) & 1).collect();
        let mut want = vec![0u8; conts];
        want.push(1);
        assert_eq!(want, last_bits);
    }

    // Varying bytes, so a reordered or dropped chunk is visible.
    fn varying(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    // Continuation frames are 510 bytes, reads are PIPE_BUF (512) -> the reader is
    // out of phase from the very first read. This is what stayed hidden in Python.
    #[test]
    fn test_roundtrip_misaligned() {
        let payload = varying(10000);
        let mut p = Pipe::new().unwrap();
        p.write(7, &payload);
        let (id, got) = p.read().expect("expected one complete message");
        assert_eq!(7, id);
        assert_eq!(payload.len(), got.len());
        assert_eq!(payload, got);
    }

    // The central claim: reassembly is independent of where read(2) happens to cut
    // the stream. Sweep read sizes across and around every frame boundary.
    #[test]
    fn test_split_invariance() {
        const N: usize = 10000;
        let payload = varying(N);
        for &rs in &[
            1,
            2,
            3,
            5,
            7,
            HEADER + SIZE_FIELD - 1,
            PAYLOAD_MAX - 1,
            PAYLOAD_MAX,
            HEADER + PAYLOAD_MAX - 1,
            HEADER + PAYLOAD_MAX,
            HEADER + PAYLOAD_MAX + 1,
            PIPE_BUF,
            PIPE_BUF + 1,
            1023,
            4096,
        ] {
            let mut p = Pipe::new().unwrap();
            p.read_size = rs;
            p.write(9, &payload);
            p.close_write();

            let mut msgs = Vec::new();
            while let Some((id, msg)) = p.read() {
                assert_eq!(9, id, "read_size {}", rs);
                msgs.push(msg);
            }
            assert_eq!(1, msgs.len(), "read_size {}: expected one message", rs);
            assert_eq!(payload, msgs[0], "read_size {}: payload mismatch", rs);
        }
    }

    // Two writers' frames interleaved by hand, so the per-id accumulator is
    // exercised deterministically rather than by scheduler luck.
    #[test]
    fn test_interleaved_writers() {
        const N: usize = 3000;
        let a = varying(N);
        let b: Vec<u8> = varying(N).iter().map(|x| x ^ 0xff).collect();
        let fa = chunk(11, &a);
        let fb = chunk(22, &b);
        assert_eq!(fa.len(), fb.len());

        let mut p = Pipe::new().unwrap();
        p.read_size = 7;
        for (x, y) in fa.iter().zip(fb.iter()) {
            p.w.as_mut().unwrap().write_all(x).unwrap();
            p.w.as_mut().unwrap().write_all(y).unwrap();
        }
        p.close_write();

        let mut got = HashMap::new();
        while let Some((id, msg)) = p.read() {
            assert!(got.insert(id, msg).is_none(), "duplicate id {}", id);
        }
        assert_eq!(2, got.len());
        assert_eq!(&a, got.get(&11).unwrap());
        assert_eq!(&b, got.get(&22).unwrap());
    }
}
