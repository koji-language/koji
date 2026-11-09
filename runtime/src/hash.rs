//! The hash of records (the same as that of the library written in Koji: `mixed` in
//! std/koji/text.kj): fast (a multiply and a rotation for each 8 bytes), since a map hashes its key
//! at each access. Its low bits are not mixed: a map takes the high bits of its hash times a large
//! odd number (see `startOf` in std/koji/map.kj), which depend on all of them.

use std::hash::Hasher;

const K: u64 = 0x517c_c1b7_2722_0a95;

#[derive(Default)]
pub(crate) struct Fast(u64);

impl Fast {
    pub(crate) fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for Fast {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}
