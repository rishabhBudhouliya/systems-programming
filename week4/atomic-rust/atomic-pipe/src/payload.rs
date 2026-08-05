//! Demo payloads that can be verified *per writer*.
//!
//! This is the crux of the aggregation demo. Interleaving reorders bytes but never
//! loses them, so any whole-stream aggregate (a total byte sum, a total length) comes
//! out identical whether or not writes were torn -- a raw pipe would pass such a check
//! and the demo would show nothing.
//!
//! So each payload carries its writer's index in the first 4 bytes and is filled with
//! bytes derived from that index. The reader recovers the index from the payload,
//! recomputes what the payload should have been, and compares. That check can only
//! pass if this writer's bytes arrived contiguous and unmixed.

/// Writer index is stored big-endian in the first 4 bytes of every payload.
pub const INDEX_LEN: usize = 4;

/// Deterministic byte stream, seeded so two different writers never agree.
struct Xorshift(u64);

impl Xorshift {
    fn new(index: u32) -> Self {
        // Multiply into the whole word so adjacent indices produce unrelated streams,
        // and force it odd so the state can never be zero (xorshift's fixed point).
        Xorshift((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Build the payload writer `index` is supposed to send: 4-byte index, then filler.
///
/// `len` is the whole payload including the index prefix, and must be at least
/// `INDEX_LEN`.
pub fn build(index: u32, len: usize) -> Vec<u8> {
    assert!(
        len >= INDEX_LEN,
        "payload of {len} bytes cannot hold the {INDEX_LEN}-byte writer index"
    );

    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&index.to_be_bytes());

    let mut rng = Xorshift::new(index);
    while out.len() < len {
        let take = (len - out.len()).min(8);
        out.extend_from_slice(&rng.next_u64().to_be_bytes()[..take]);
    }
    out
}

/// Recover the writer index a payload claims to come from.
pub fn index_of(data: &[u8]) -> Option<u32> {
    let head: [u8; INDEX_LEN] = data.get(..INDEX_LEN)?.try_into().ok()?;
    Some(u32::from_be_bytes(head))
}

/// FNV-1a, 64-bit. Order-sensitive, which is the entire point -- a permuted payload
/// hashes differently, where a sum would not.
pub fn checksum(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Outcome of checking one received payload against what its writer should have sent.
pub enum Verdict {
    Ok { index: u32 },
    NoIndex,
    WrongLength { index: u32, want: usize, got: usize },
    Corrupt { index: u32 },
}

impl Verdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Verdict::Ok { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Verdict::Ok { index } => format!("writer {index}: intact"),
            Verdict::NoIndex => "payload too short to contain a writer index".to_string(),
            Verdict::WrongLength { index, want, got } => {
                format!("writer {index}: length {got}, expected {want}")
            }
            Verdict::Corrupt { index } => {
                format!("writer {index}: checksum mismatch -- bytes are not this writer's")
            }
        }
    }
}

/// Check a received payload: does it match what the writer it claims to be would send?
pub fn verify(data: &[u8], expect_len: usize) -> Verdict {
    let Some(index) = index_of(data) else {
        return Verdict::NoIndex;
    };
    if data.len() != expect_len {
        return Verdict::WrongLength {
            index,
            want: expect_len,
            got: data.len(),
        };
    }
    if checksum(data) != checksum(&build(index, expect_len)) {
        return Verdict::Corrupt { index };
    }
    Verdict::Ok { index }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let p = build(3, 4096);
        assert_eq!(4096, p.len());
        assert_eq!(Some(3), index_of(&p));
        assert!(verify(&p, 4096).is_ok());
    }

    #[test]
    fn writers_differ() {
        assert_ne!(build(1, 1024), build(2, 1024));
    }

    // The property the demo depends on: a swapped-in run of another writer's bytes is
    // detected. A total-byte-sum check would not necessarily catch this.
    #[test]
    fn detects_spliced_bytes() {
        let mut a = build(1, 8192);
        let b = build(2, 8192);
        a[4096..6000].copy_from_slice(&b[4096..6000]);
        assert!(!verify(&a, 8192).is_ok());
    }

    // Interleaving permutes bytes without losing them. Confirm an order-insensitive
    // aggregate really is blind to that, which is why checksum() is order-sensitive.
    #[test]
    fn sum_is_blind_to_reordering_but_checksum_is_not() {
        let p = build(7, 4096);
        let mut shuffled = p.clone();
        shuffled.reverse();

        let sum = |d: &[u8]| d.iter().map(|&b| b as u64).sum::<u64>();
        assert_eq!(sum(&p), sum(&shuffled), "a byte sum cannot see reordering");
        assert_ne!(checksum(&p), checksum(&shuffled), "a checksum can");
    }
}
