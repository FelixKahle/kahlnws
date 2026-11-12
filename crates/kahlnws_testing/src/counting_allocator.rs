// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Heap allocations counted on the thread that makes them, for the tests that claim a call allocates nothing.
//!
//! A count of the whole process would make that claim about every thread at once: the test harness's main thread
//! recording a test it just started lands in the window whenever scheduling puts it there. So [`CountingAllocator`]
//! counts each allocation on the thread that made it, and a test reads its own thread's count with
//! [`allocations_on_this_thread`] before and after the work it measures.
//!
//! What is counted is every request, whether or not the system serves it: an allocation, a zeroed allocation and a
//! reallocation each count as one block of the size asked for, before the request is forwarded; a deallocation counts
//! nothing.
//!
//! The allocator only counts and forwards: every method passes its arguments to `System` unchanged and returns what
//! `System` returned. A global allocator cannot be written without implementing the unsafe trait `GlobalAlloc`, so
//! each forwarding call is an `unsafe` block of its own, preceded by a `debug_assert!` of what the trait's caller
//! promises as far as it can be stated: a layout of at least one byte, a pointer that is not null, and a new size that
//! makes a layout with the old alignment. That the pointer is a block this allocator handed out with this layout is
//! the caller's promise too, and cannot be checked here.
//!
//! The counts live in thread-locals that are constant-initialised and have no destructor. On the targets with native
//! thread-local storage (macOS, Linux, Windows) reading them neither allocates nor fails, not even while a thread
//! exits, so the allocator never re-enters itself.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::fmt;
use core::ops::Sub;
use std::alloc::System;

/// The system allocator, counting every allocation on the thread that makes it. A test binary installs it as its
/// `#[global_allocator]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CountingAllocator;

/// Heap blocks allocated and the bytes they asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Allocations {
    /// The blocks: one per request to allocate, allocate zeroed or reallocate, served or not.
    pub blocks: u64,
    /// The bytes those blocks asked for.
    pub bytes: u64,
}

impl Allocations {
    /// Nothing allocated.
    pub const NONE: Self = Self { blocks: 0, bytes: 0 };
}

impl fmt::Display for Allocations {
    /// The count as `3 blocks of 96 bytes`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} blocks of {} bytes", self.blocks, self.bytes)
    }
}

impl Sub for Allocations {
    type Output = Self;

    /// What was allocated between the reading `earlier` and this one, both of the same thread.
    ///
    /// # Panics
    /// Panics if `earlier` holds more blocks or bytes than this reading.
    #[track_caller]
    fn sub(self, earlier: Self) -> Self {
        Self { blocks: self.blocks.strict_sub(earlier.blocks), bytes: self.bytes.strict_sub(earlier.bytes) }
    }
}

thread_local! {
    /// The blocks the thread allocated so far. Constant-initialised and without a destructor, so reading or writing it
    /// from inside the allocator never allocates, not even while the thread exits.
    static BLOCKS: Cell<u64> = const { Cell::new(0) };
    /// The bytes those blocks asked for.
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

/// Counts one block of `size` bytes, a request about to be forwarded, on the calling thread.
fn count(size: usize) {
    // The counts saturate and wrap rather than panic: a panic inside the allocator cannot unwind and aborts.
    let size = u64::try_from(size).unwrap_or(u64::MAX);

    BLOCKS.set(BLOCKS.get().wrapping_add(1));
    BYTES.set(BYTES.get().wrapping_add(size));
}

// SAFETY: every method forwards its arguments to `System` unchanged and returns what `System` returned, so the contract
// `System` keeps is the one kept here; counting touches only thread-locals, never the heap.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());

        debug_assert!(layout.size() != 0, "called `GlobalAlloc::alloc` with a layout of 0 bytes, which should be >= 1");
        // SAFETY: the caller promises `GlobalAlloc::alloc` a layout of non-zero size, which is all `System` asks.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());

        debug_assert!(
            layout.size() != 0,
            "called `GlobalAlloc::alloc_zeroed` with a layout of 0 bytes, which should be >= 1"
        );
        // SAFETY: the caller promises `GlobalAlloc::alloc_zeroed` a layout of non-zero size, all that `System` asks.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);

        debug_assert!(
            !ptr.is_null()
                && layout.size() != 0
                && new_size != 0
                && Layout::from_size_align(new_size, layout.align()).is_ok(),
            "called `GlobalAlloc::realloc` with pointer {ptr:?} from {} bytes to {new_size} bytes at alignment {}, \
             which should be a block of at least 1 byte resized to a layout of at least 1 byte",
            layout.size(),
            layout.align()
        );
        // SAFETY: the caller promises `GlobalAlloc::realloc` that `ptr` is a block this allocator, which forwards to
        // `System`, handed out with `layout`, and that `new_size` is non-zero and makes a layout with its alignment.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        debug_assert!(
            !ptr.is_null() && layout.size() != 0,
            "called `GlobalAlloc::dealloc` with pointer {ptr:?} and a layout of {} bytes, which should be a block of \
             at least 1 byte",
            layout.size()
        );
        // SAFETY: the caller promises `GlobalAlloc::dealloc` that `ptr` is a block this allocator, which forwards to
        // `System`, handed out with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// What the calling thread allocated so far through a [`CountingAllocator`]. Only the thread's own allocations move
/// it.
pub fn allocations_on_this_thread() -> Allocations {
    Allocations { blocks: BLOCKS.get(), bytes: BYTES.get() }
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;

    use super::*;

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn test_a_push_within_capacity_allocates_nothing() {
        let mut buffer = Vec::<u64>::with_capacity(16);

        let before = allocations_on_this_thread();
        buffer.extend(0..16);
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE);

        black_box(buffer);
    }

    #[test]
    fn test_growing_past_capacity_counts_one_block_of_the_new_size() {
        let mut buffer = Vec::<u64>::with_capacity(4);
        buffer.extend(0..4);

        let before = allocations_on_this_thread();
        buffer.reserve_exact(4);
        assert_eq!(allocations_on_this_thread() - before, Allocations { blocks: 1, bytes: 64 });

        black_box(buffer);
    }

    #[test]
    fn test_a_box_counts_its_size_and_freeing_it_counts_nothing() {
        let before = allocations_on_this_thread();
        let boxed = black_box(Box::new([0_u64; 3]));
        assert_eq!(allocations_on_this_thread() - before, Allocations { blocks: 1, bytes: 24 });

        let before = allocations_on_this_thread();
        drop(boxed);
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE);
    }

    #[test]
    fn test_a_zeroed_allocation_counts_as_one_block() {
        let before = allocations_on_this_thread();
        let zeroed = black_box(vec![0_u32; 5]);
        assert_eq!(allocations_on_this_thread() - before, Allocations { blocks: 1, bytes: 20 });

        black_box(zeroed);
    }

    #[test]
    fn test_another_threads_allocations_count_there_and_not_here() {
        const LARGE: u64 = 1 << 20;

        let before = allocations_on_this_thread();
        let theirs = std::thread::spawn(|| {
            let before = allocations_on_this_thread();
            black_box(vec![0_u8; 1 << 20]);
            allocations_on_this_thread() - before
        })
        .join()
        .expect("the spawned thread should not panic");
        let ours = allocations_on_this_thread() - before;

        assert!(theirs.bytes >= LARGE, "the spawned thread counted {theirs}");
        assert!(ours.bytes < LARGE, "this thread counted {ours}, the spawned thread's block among it");
    }

    #[test]
    fn test_a_request_the_system_refuses_counts_too() {
        let mut buffer = Vec::<u8>::new();

        let before = allocations_on_this_thread();
        let refused = buffer.try_reserve_exact(1 << 62);
        assert!(refused.is_err(), "the system should refuse 2^62 bytes");
        assert_eq!(allocations_on_this_thread() - before, Allocations { blocks: 1, bytes: 1 << 62 });
    }

    #[test]
    fn test_display_gives_blocks_and_bytes() {
        assert_eq!(Allocations { blocks: 3, bytes: 96 }.to_string(), "3 blocks of 96 bytes");
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn test_sub_refuses_a_later_reading_first() {
        let _ = Allocations::NONE - Allocations { blocks: 1, bytes: 8 };
    }
}
