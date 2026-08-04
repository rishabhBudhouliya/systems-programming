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
