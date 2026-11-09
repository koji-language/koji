//! The memory of small values: programs create and drop many of them, and taking a block from
//! here is much cheaper than `malloc` (and giving it back than `free`). One list of free blocks
//! per size in 8-byte words, for blocks of up to `POOLED` words: each free block holds the next
//! one in its first word. An empty list gets a chunk of blocks from the system at once; a block
//! given back stays in its list, for the next value of its size. Larger blocks go to and come
//! from the system.
//!
//! The lists of the thread that runs the program are `koji_pool`, which the generated code reads
//! and writes inline (`koji_pool_take` and `koji_pool_give` in prelude.h), calling
//! `koji_pool_refill` when a list is empty. The pool is also the allocator of the runtime's Rust
//! code (`Allocator`): the items of lists, the entries of maps, `String`s. Another thread (the
//! runtime starts none) has lists of its own; a block can go to any list of its size, since they
//! are all cut from chunks the same way.
//!
//! With `KOJI_POOL=0` in the environment, every block comes from the system and goes back to it,
//! so that valgrind sees each one (the tests run their programs so).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, UnsafeCell};
use std::ffi::c_char;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

const POOLED: usize = 16;

/// The bytes of a chunk, cut into blocks of one size when a list is empty.
const CHUNK: usize = 8192;

type Heads = [*mut u8; POOLED + 1];

/// The first free block of each size, or NULL, for the thread that runs the program.
#[no_mangle]
pub static mut koji_pool: Heads = [std::ptr::null_mut(); POOLED + 1];

/// Whether the pool is off (`KOJI_POOL=0`), for the generated code, once `enabled` has read it.
#[no_mangle]
pub static mut koji_pool_off: bool = false;

/// 0 before the environment is read, then 1 when the pool is on and 2 when it is off.
static STATE: AtomicU8 = AtomicU8::new(0);

extern "C" {
    fn getenv(name: *const c_char) -> *const c_char;
}

/// Whether the calling thread uses `koji_pool`: the thread that runs the program, once
/// `koji_init` has run with the pool on. The fast path (a thread-local variable is a direct access
/// in an executable).
#[inline(always)]
fn main_thread() -> bool {
    MAIN.with(|main| main.get())
}

/// Whether the pool is on: read once from the environment, without allocating.
fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let value = unsafe { getenv(c"KOJI_POOL".as_ptr()) };
            let off = !value.is_null() && unsafe { std::ffi::CStr::from_ptr(value) }.to_bytes() == b"0";
            unsafe { koji_pool_off = off };
            STATE.store(if off { 2 } else { 1 }, Ordering::Relaxed);
            !off
        }
    }
}

thread_local! {
    /// See `main_thread`.
    static MAIN: Cell<bool> = const { Cell::new(false) };
    /// The lists of another thread (or of the main one before `koji_init`).
    static OWN: UnsafeCell<Heads> = const { UnsafeCell::new([std::ptr::null_mut(); POOLED + 1]) };
}

/// The chunks taken from the system, each holding the previous one in its first word (they are
/// never given back: their blocks are in the lists).
static CHUNKS: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());

/// Makes the calling thread the one that runs the program (from `koji_init`).
pub(crate) fn adopt() {
    if enabled() {
        MAIN.with(|main| main.set(true));
    }
}

/// The lists of another thread than the main one.
fn own() -> *mut Heads {
    OWN.with(|own| own.get())
}

fn system_layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes.max(8), 8).expect("the size of a block")
}

fn system(bytes: usize) -> *mut u8 {
    let block = unsafe { System.alloc(system_layout(bytes)) };
    if block.is_null() {
        std::alloc::handle_alloc_error(system_layout(bytes));
    }
    block
}

/// A block of `words` words from the lists `heads`, refilling the list when it is empty.
unsafe fn take_from(heads: *mut Heads, words: usize) -> *mut u8 {
    let head = (*heads)[words];
    if !head.is_null() {
        (*heads)[words] = *(head as *mut *mut u8);
        return head;
    }
    refill(heads, words)
}

unsafe fn give_to(heads: *mut Heads, block: *mut u8, words: usize) {
    *(block as *mut *mut u8) = (*heads)[words];
    (*heads)[words] = block;
}

/// A new chunk cut into blocks of `words` words: the first one is returned, the others go to the
/// list. The chunk's first word links it to the others.
unsafe fn refill(heads: *mut Heads, words: usize) -> *mut u8 {
    let size = words.max(1) * 8;
    let count = (CHUNK / size).max(8);
    let chunk = system(8 + size * count);
    let mut previous = CHUNKS.load(Ordering::Relaxed);
    loop {
        *(chunk as *mut *mut u8) = previous;
        match CHUNKS.compare_exchange_weak(previous, chunk, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => break,
            Err(current) => previous = current,
        }
    }
    let first = chunk.add(8);
    for index in (1..count).rev() {
        give_to(heads, first.add(index * size), words);
    }
    first
}

/// A block of `words` 8-byte words (not zeroed), aligned to 8 bytes.
#[inline]
pub(crate) fn take(words: usize) -> *mut u8 {
    if words <= POOLED && main_thread() {
        return unsafe { take_from(std::ptr::addr_of_mut!(koji_pool), words) };
    }
    take_slowly(words)
}

#[cold]
fn take_slowly(words: usize) -> *mut u8 {
    if words > POOLED || !enabled() {
        return system(words * 8);
    }
    unsafe { take_from(own(), words) }
}

/// Gives back a block from `take(words)`.
#[inline]
pub(crate) fn give(block: *mut u8, words: usize) {
    if words <= POOLED && main_thread() {
        return unsafe { give_to(std::ptr::addr_of_mut!(koji_pool), block, words) };
    }
    give_slowly(block, words)
}

#[cold]
fn give_slowly(block: *mut u8, words: usize) {
    if words > POOLED || !enabled() {
        koji_pool_free(block, words);
        return;
    }
    unsafe { give_to(own(), block, words) }
}

/// A block of `words` words when the generated code finds its list empty (or the block is
/// larger than the pooled ones, or the pool is off).
#[no_mangle]
pub extern "C" fn koji_pool_refill(words: usize) -> *mut u8 {
    take(words)
}

/// Gives a block larger than the pooled ones (or any block, with the pool off) back to the system.
#[no_mangle]
pub extern "C" fn koji_pool_free(block: *mut u8, words: usize) {
    unsafe { System.dealloc(block, system_layout(words * 8)) };
}

/// The block `block` of `words` words, grown or shrunk to `new_words` (its contents kept), as the
/// allocator of the runtime does it (for the lists of std/koji/list.kj).
#[no_mangle]
pub extern "C" fn koji_pool_resize(block: *mut u8, words: usize, new_words: usize) -> *mut u8 {
    unsafe { Allocator.realloc(block, Layout::from_size_align_unchecked(words * 8, 8), new_words * 8) }
}

/// The allocator of the runtime: blocks of up to `POOLED` words (aligned to 8 bytes at most) come
/// from the pool, the others from the system.
pub struct Allocator;

/// The words of a block for `layout`, if it comes from the pool.
#[inline(always)]
fn pooled(layout: Layout) -> Option<usize> {
    if layout.align() > 8 || layout.size() > POOLED * 8 {
        return None;
    }
    Some(layout.size().div_ceil(8).max(1))
}

unsafe impl GlobalAlloc for Allocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match pooled(layout) {
            Some(words) if main_thread() => take_from(std::ptr::addr_of_mut!(koji_pool), words),
            Some(words) if enabled() => take_from(own(), words),
            _ => System.alloc(layout),
        }
    }

    #[inline]
    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        match pooled(layout) {
            Some(words) if main_thread() => give_to(std::ptr::addr_of_mut!(koji_pool), block, words),
            Some(words) if enabled() => give_to(own(), block, words),
            _ => System.dealloc(block, layout),
        }
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // With the pool off, every block is the system's.
        if !main_thread() && !enabled() {
            return System.realloc(block, layout, size);
        }
        let new_layout = Layout::from_size_align_unchecked(size, layout.align());
        match (pooled(layout), pooled(new_layout)) {
            // Both from the system: it may grow the block in place.
            (None, None) => System.realloc(block, layout, size),
            // The same block size.
            (Some(old), Some(new)) if old == new => block,
            _ => {
                let grown = self.alloc(new_layout);
                if !grown.is_null() {
                    std::ptr::copy_nonoverlapping(block, grown, layout.size().min(size));
                    self.dealloc(block, layout);
                }
                grown
            }
        }
    }
}

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
