//! The memory of small values (records and strings): programs create and drop many of them, and
//! taking a block from here is much cheaper than `malloc` (and giving it back than `free`). One
//! list of free blocks per size in 8-byte words, for blocks of up to `POOLED` words: each free
//! block holds the next one in its first word. The heads of the lists are `koji_pool`, which the
//! generated code reads and writes inline (`koji_pool_take` and `koji_pool_give` in prelude.h);
//! it calls `koji_pool_refill` when a list is empty, and this takes a chunk of blocks from the
//! system at once. A block given back stays in its list, for the next value of its size; larger
//! blocks go to and come from the system.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::UnsafeCell;

const POOLED: usize = 16;

/// The bytes of a chunk, cut into blocks of one size when a list is empty.
const CHUNK: usize = 8192;

/// The first free block of each size, or NULL.
#[no_mangle]
pub static mut koji_pool: [*mut u8; POOLED + 1] = [std::ptr::null_mut(); POOLED + 1];

/// The chunks taken from the system (never given back: their blocks are in the lists).
struct Chunks(UnsafeCell<Vec<*mut u8>>);

// SAFETY: the runtime of a Koji program runs on one thread.
unsafe impl Sync for Chunks {}

static CHUNKS: Chunks = Chunks(UnsafeCell::new(Vec::new()));

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes.max(8), 8).expect("the size of a block")
}

fn system(bytes: usize) -> *mut u8 {
    let block = unsafe { alloc(layout(bytes)) };
    if block.is_null() {
        std::alloc::handle_alloc_error(layout(bytes));
    }
    block
}

/// A block of `words` 8-byte words (not zeroed), aligned to 8 bytes.
pub(crate) fn take(words: usize) -> *mut u8 {
    if words <= POOLED {
        unsafe {
            let head = (*std::ptr::addr_of!(koji_pool))[words];
            if !head.is_null() {
                (*std::ptr::addr_of_mut!(koji_pool))[words] = *(head as *mut *mut u8);
                return head;
            }
        }
    }
    koji_pool_refill(words)
}

/// Gives back a block from `take(words)`.
pub(crate) fn give(block: *mut u8, words: usize) {
    if words > POOLED {
        koji_pool_free(block, words);
        return;
    }
    unsafe {
        let heads = &mut *std::ptr::addr_of_mut!(koji_pool);
        *(block as *mut *mut u8) = heads[words];
        heads[words] = block;
    }
}

/// A block of `words` words when its list is empty: the list gets a new chunk of blocks, but
/// the first one. A larger block comes from the system.
#[no_mangle]
pub extern "C" fn koji_pool_refill(words: usize) -> *mut u8 {
    if words > POOLED {
        return system(words * 8);
    }
    let size = words.max(1) * 8;
    let count = (CHUNK / size).max(8);
    let chunk = system(size * count);
    unsafe {
        (*CHUNKS.0.get()).push(chunk);
        let heads = &mut *std::ptr::addr_of_mut!(koji_pool);
        for index in (1..count).rev() {
            let block = chunk.add(index * size);
            *(block as *mut *mut u8) = heads[words];
            heads[words] = block;
        }
    }
    chunk
}

/// Gives a block larger than the pooled ones back to the system.
#[no_mangle]
pub extern "C" fn koji_pool_free(block: *mut u8, words: usize) {
    unsafe { dealloc(block, layout(words * 8)) };
}
