//! The memory of small values that died, kept for the next values of the same size: programs
//! create and drop many small records and strings, and taking a block from here is much cheaper
//! than `malloc` (and giving it back than `free`). One list of free blocks per size in 8-byte
//! words, for blocks of up to `POOLED` words, each holding at most `KEPT` blocks; the others go
//! back to the system.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::UnsafeCell;

const POOLED: usize = 16;
const KEPT: usize = 1 << 16;

struct Pool(UnsafeCell<[Vec<*mut u8>; POOLED + 1]>);

// SAFETY: the runtime of a Koji program runs on one thread.
unsafe impl Sync for Pool {}

static POOL: Pool = Pool(UnsafeCell::new([const { Vec::new() }; POOLED + 1]));

fn layout(words: usize) -> Layout {
    Layout::from_size_align(words.max(1) * 8, 8).expect("the size of a block")
}

/// A block of `words` 8-byte words (not zeroed), aligned to 8 bytes.
pub(crate) fn take(words: usize) -> *mut u8 {
    if words <= POOLED {
        if let Some(block) = unsafe { (*POOL.0.get())[words].pop() } {
            return block;
        }
    }
    let block = unsafe { alloc(layout(words)) };
    if block.is_null() {
        std::alloc::handle_alloc_error(layout(words));
    }
    block
}

/// Gives back a block from `take(words)`.
pub(crate) fn give(block: *mut u8, words: usize) {
    if words <= POOLED {
        let free = unsafe { &mut (*POOL.0.get())[words] };
        if free.len() < KEPT {
            free.push(block);
            return;
        }
    }
    unsafe { dealloc(block, layout(words)) };
}
