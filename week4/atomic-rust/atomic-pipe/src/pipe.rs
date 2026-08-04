/*
 * Steps:
 * 1) add the rustix dependency
 * 2) start writing the pipe code
 * 3) add a test file that tests the pipe code
 */

struct Message<'a> {
    // process id, u32 should fit 2^22 ids
    id: u32,
    last: u8,
    payload: &'a [u8],
    limit: usize,
}

// returned Message is tired to this payload
impl<'a> Message<'a> {
    pub fn new(id: u32, last: u8, payload: &'a [u8]) -> Message<'a> {
        let limit = 4096;
        Message {
            id,
            last,
            payload,
            limit,
        }
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
            let mut buffer = Vec::with_capacity(4096);
            // we need to slice the first byte to keep 3 bytes
            buffer.extend(&word[1..]);
            buffer.extend(self.payload);
            return buffer;
        }
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
    while remaining > 4091 {
        let m = Message::new(pid, 0, &data[..4091]);
        messages.push(m.convert());
        data = &data[4091..];
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
        let payload = vec![65u8; 10000];
        let frames = chunk(id, &payload);
        assert_eq!(3, frames.len());
        assert_eq!(4094, frames[0].len());
        assert_eq!(4094, frames[1].len());
        assert_eq!(1823, frames[2].len());

        // The guarantee the whole project exists for: no single write() exceeds PIPE_BUF.
        for f in &frames {
            assert!(f.len() <= 4096, "frame of {} bytes exceeds PIPE_BUF", f.len());
        }

        // Exactly one terminal frame, and it is the last one.
        // The last-bit is bit 22 of the 3-byte header == bit 6 of header byte 0.
        let last_bits: Vec<u8> = frames.iter().map(|f| (f[0] >> 6) & 1).collect();
        assert_eq!(vec![0, 0, 1], last_bits);
    }
}
