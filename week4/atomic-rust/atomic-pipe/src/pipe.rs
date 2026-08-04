/*
 * Steps:
 * 1) add the rustix dependency
 * 2) start writing the pipe code
 * 3) add a test file that tests the pipe code
 */

struct Message {
    // process id, u32 should fit 2^22 ids
    id: u32,
    last: u8,
    payload: [u8; 4091],
    limit: usize,
}

impl Message {
    pub fn new(id: u32, last: u8, payload: [u8; 4091]) -> Message {
        Message { id, last, payload, 4096}
    }

    // think of the frame buffer as a Vec<u8> instead of a fixed sized array
    pub fn convert(&self) -> Vec<u8> {
        if self.last & 1 == 1 {
            let word = self.id | ((self.last as u32 & 1) << 22);
            let size = (self.payload.len() as u16).to_be_bytes();
            return (word << 8) + size + self.payload.to_be_bytes();
        } else {
        }
    }
}
