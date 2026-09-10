use core::ops::Range;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

pub struct BitmapAllocator<const N: usize>
where
    [(); N.div_ceil(64)]: Sized,
{
    bitmap: [AtomicU64; N.div_ceil(64)],
}

impl<const N: usize> BitmapAllocator<N>
where
    [(); N.div_ceil(64)]: Sized,
{
    pub const fn new() -> Self {
        #[allow(clippy::declare_interior_mutable_const)]
        const ZEROED: AtomicU64 = AtomicU64::new(0);
        Self {
            bitmap: [ZEROED; N.div_ceil(64)],
        }
    }

    /// Reserves every bit in `range`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no allocation or deallocation
    /// operation can concurrently modify the affected bits.
    pub unsafe fn reserve(&self, range: Range<usize>) {
        for index in range.start..core::cmp::min(range.end, N) {
            self.bitmap[index / 64].fetch_or(1u64 << (index % 64), Ordering::Relaxed);
        }
    }

    pub fn alloc(&self) -> Option<usize> {
        for (word_index, word) in self.bitmap.iter().enumerate() {
            let valid_bits = if word_index == self.bitmap.len() - 1 {
                let rem = N % 64;
                if rem == 0 { 64 } else { rem }
            } else {
                64
            };

            let valid_mask = if valid_bits == 64 {
                u64::MAX
            } else {
                (1u64 << valid_bits) - 1
            };

            let mut value = word.load(Ordering::Relaxed);

            loop {
                let free = (!value) & valid_mask;

                if free == 0 {
                    break;
                }

                let bit = free.trailing_zeros() as usize;
                let mask = 1u64 << bit;
                let new = value | mask;

                match word.compare_exchange_weak(value, new, Ordering::AcqRel, Ordering::Relaxed) {
                    Ok(_) => {
                        return Some(word_index * 64 + bit);
                    }
                    Err(v) => value = v,
                }
            }
        }

        None
    }

    pub fn free(&self, index: usize) -> Option<usize> {
        if index >= N {
            return None;
        }

        let word_index = index / 64;
        let bit = index % 64;
        let mask = 1u64 << bit;

        let old = self.bitmap[word_index].fetch_and(!mask, Ordering::Relaxed);

        if old & mask == 0 {
            return None; // fucking double free
        }

        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_allocator_is_completely_free() {
        let allocator = BitmapAllocator::<128>::new();

        for expected in 0..128 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn alloc_returns_indices_in_order() {
        let allocator = BitmapAllocator::<10>::new();

        for expected in 0..10 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn free_makes_index_available_again() {
        let allocator = BitmapAllocator::<10>::new();

        assert_eq!(allocator.alloc(), Some(0));
        assert_eq!(allocator.alloc(), Some(1));
        assert_eq!(allocator.alloc(), Some(2));

        assert_eq!(allocator.free(1), Some(1));

        // The allocator should reuse the lowest available bit.
        assert_eq!(allocator.alloc(), Some(1));
    }

    #[test]
    fn double_free_returns_none() {
        let allocator = BitmapAllocator::<10>::new();

        assert_eq!(allocator.alloc(), Some(0));

        assert_eq!(allocator.free(0), Some(0));
        assert_eq!(allocator.free(0), None);
    }

    #[test]
    fn free_out_of_range_returns_none() {
        let allocator = BitmapAllocator::<10>::new();

        assert_eq!(allocator.free(10), None);
        assert_eq!(allocator.free(100), None);
        assert_eq!(allocator.free(usize::MAX), None);
    }

    #[test]
    fn allocator_handles_exact_word_boundary() {
        let allocator = BitmapAllocator::<64>::new();

        for expected in 0..64 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn allocator_handles_multiple_words() {
        let allocator = BitmapAllocator::<128>::new();

        // Fill the first 64-bit word.
        for expected in 0..64 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        // Allocation should continue in the second word.
        for expected in 64..128 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn allocator_handles_partial_final_word() {
        let allocator = BitmapAllocator::<65>::new();

        for expected in 0..65 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        // Bit 65..63 of the final word don't exist and must never be
        // returned by alloc().
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn reserve_prevents_allocation() {
        let allocator = BitmapAllocator::<16>::new();

        unsafe {
            allocator.reserve(0..4);
        }

        assert_eq!(allocator.alloc(), Some(4));
        assert_eq!(allocator.alloc(), Some(5));
        assert_eq!(allocator.alloc(), Some(6));
        assert_eq!(allocator.alloc(), Some(7));
    }

    #[test]
    fn reserve_can_cover_multiple_words() {
        let allocator = BitmapAllocator::<128>::new();

        unsafe {
            allocator.reserve(60..68);
        }

        // 0..60 are still available.
        for expected in 0..60 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        // 60..67 were reserved, so allocation resumes at 68.
        assert_eq!(allocator.alloc(), Some(68));
    }

    #[test]
    fn reserve_is_clamped_to_allocator_size() {
        let allocator = BitmapAllocator::<10>::new();

        unsafe {
            allocator.reserve(8..100);
        }

        for expected in 0..8 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn reserve_entire_allocator() {
        let allocator = BitmapAllocator::<100>::new();

        unsafe {
            allocator.reserve(0..100);
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn freeing_reserved_bit_makes_it_allocatable() {
        let allocator = BitmapAllocator::<16>::new();

        unsafe {
            allocator.reserve(0..4);
        }

        assert_eq!(allocator.alloc(), Some(4));

        // `free` doesn't know about reservations. Once a reserved bit is
        // cleared, it becomes available for allocation.
        assert_eq!(allocator.free(2), Some(2));
        assert_eq!(allocator.alloc(), Some(2));
    }

    #[test]
    fn allocation_and_freeing_work_across_word_boundary() {
        let allocator = BitmapAllocator::<100>::new();

        // Allocate everything in the first word.
        for expected in 0..64 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        // Free a bit in the first word.
        assert_eq!(allocator.free(10), Some(10));

        // The allocator scans from the beginning, so it should reuse 10
        // before allocating from the second word.
        assert_eq!(allocator.alloc(), Some(10));

        // The next allocation should come from the second word.
        assert_eq!(allocator.alloc(), Some(64));
    }

    #[test]
    fn free_unallocated_index_returns_none() {
        let allocator = BitmapAllocator::<32>::new();

        // Nothing has been allocated yet.
        assert_eq!(allocator.free(5), None);

        // Allocate it, then freeing it should succeed.
        assert_eq!(allocator.alloc(), Some(0));
        assert_eq!(allocator.free(0), Some(0));

        // And freeing it again should fail.
        assert_eq!(allocator.free(0), None);
    }

    #[test]
    fn reserve_zero_length_range_does_nothing() {
        let allocator = BitmapAllocator::<16>::new();

        unsafe {
            allocator.reserve(5..5);
        }

        assert_eq!(allocator.alloc(), Some(0));
    }

    #[test]
    fn reserve_out_of_bounds_range_does_not_panic() {
        let allocator = BitmapAllocator::<16>::new();

        unsafe {
            allocator.reserve(16..100);
        }

        for expected in 0..16 {
            assert_eq!(allocator.free(expected), None);
        }

        assert!(allocator.alloc().is_some());
    }

    #[test]
    fn allocator_can_reuse_many_freed_indices() {
        let allocator = BitmapAllocator::<128>::new();

        // Allocate everything.
        for expected in 0..128 {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);

        // Free every other index.
        for index in (0..128).step_by(2) {
            assert_eq!(allocator.free(index), Some(index));
        }

        // Allocations should reuse exactly those freed indices.
        for expected in (0..128).step_by(2) {
            assert_eq!(allocator.alloc(), Some(expected));
        }

        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn concurrent_allocations_are_unique() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        const N: usize = 4096;
        const THREADS: usize = 64;
        const ALLOCS_PER_THREAD: usize = N / THREADS;

        let allocator = Arc::new(BitmapAllocator::<N>::new());
        let barrier = Arc::new(Barrier::new(THREADS));

        let mut handles = Vec::new();

        for _ in 0..THREADS {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                // Start all threads at roughly the same time.
                barrier.wait();

                let mut allocations = Vec::with_capacity(ALLOCS_PER_THREAD);

                for _ in 0..ALLOCS_PER_THREAD {
                    let index = allocator
                        .alloc()
                        .expect("allocator ran out of space unexpectedly");

                    allocations.push(index);
                }

                allocations
            }));
        }

        let mut allocations = Vec::with_capacity(N);

        for handle in handles {
            allocations.extend(handle.join().expect("allocation thread panicked"));
        }

        assert_eq!(allocations.len(), N);

        // If two threads received the same index, this will fail.
        allocations.sort_unstable();

        for (expected, actual) in allocations.into_iter().enumerate() {
            assert_eq!(actual, expected);
        }

        // Everything should be allocated now.
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn concurrent_allocations_across_multiple_words_are_unique() {
        use std::collections::HashSet;
        use std::sync::{Arc, Barrier};
        use std::thread;

        const N: usize = 1024;
        const THREADS: usize = 64;
        const ALLOCS_PER_THREAD: usize = N / THREADS;

        let allocator = Arc::new(BitmapAllocator::<N>::new());
        let barrier = Arc::new(Barrier::new(THREADS));

        let mut handles = Vec::new();

        for _ in 0..THREADS {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                barrier.wait();

                let mut result = Vec::new();

                for _ in 0..ALLOCS_PER_THREAD {
                    if let Some(index) = allocator.alloc() {
                        result.push(index);
                    }
                }

                result
            }));
        }

        let mut all_allocations = Vec::new();

        for handle in handles {
            all_allocations.extend(handle.join().expect("allocation thread panicked"));
        }

        assert_eq!(all_allocations.len(), N);

        let unique: HashSet<_> = all_allocations.iter().copied().collect();

        assert_eq!(
            unique.len(),
            N,
            "duplicate allocation detected: got {} unique indices out of {}",
            unique.len(),
            N
        );

        for index in 0..N {
            assert!(unique.contains(&index), "index {index} was never allocated");
        }
    }

    #[test]
    fn concurrent_allocations_and_frees_do_not_corrupt_bitmap() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        const N: usize = 256;
        const THREADS: usize = 16;
        const ITERATIONS: usize = 10_000;

        let allocator = Arc::new(BitmapAllocator::<N>::new());
        let barrier = Arc::new(Barrier::new(THREADS));

        let mut handles = Vec::new();

        for thread_id in 0..THREADS {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                barrier.wait();

                for iteration in 0..ITERATIONS {
                    if let Some(index) = allocator.alloc() {
                        // Do something deterministic with the allocation so
                        // the compiler can't turn this into a boring no-op.
                        assert!(index < N);

                        assert_eq!(
                            allocator.free(index),
                            Some(index),
                            "thread {thread_id} failed to free its allocation \
                             on iteration {iteration}"
                        );
                    }
                }
            }));
        }

        for handle in handles {
            handle
                .join()
                .expect("concurrent allocation/free thread panicked");
        }

        // Every allocation should have been returned.
        //
        // If the bitmap got corrupted, we may either leak capacity or have
        // invalid state here.
        let mut allocated = Vec::new();

        for _ in 0..N {
            allocated.push(
                allocator
                    .alloc()
                    .expect("allocator lost capacity during concurrent operations"),
            );
        }

        assert_eq!(allocator.alloc(), None);

        allocated.sort_unstable();

        for (expected, actual) in allocated.into_iter().enumerate() {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn concurrent_allocations_with_partial_final_word_are_safe() {
        use std::collections::HashSet;
        use std::sync::{Arc, Barrier};
        use std::thread;

        // Deliberately not divisible by 64.
        const N: usize = 100;
        const THREADS: usize = 20;
        const ALLOCS_PER_THREAD: usize = N / THREADS;

        let allocator = Arc::new(BitmapAllocator::<N>::new());
        let barrier = Arc::new(Barrier::new(THREADS));

        let mut handles = Vec::new();

        for _ in 0..THREADS {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                barrier.wait();

                let mut result = Vec::new();

                for _ in 0..ALLOCS_PER_THREAD {
                    result.push(
                        allocator
                            .alloc()
                            .expect("allocator unexpectedly ran out of space"),
                    );
                }

                result
            }));
        }

        let mut allocations = Vec::new();

        for handle in handles {
            allocations.extend(handle.join().expect("thread panicked"));
        }

        assert_eq!(allocations.len(), N);

        let unique: HashSet<_> = allocations.iter().copied().collect();

        assert_eq!(unique.len(), N);

        for index in 0..N {
            assert!(unique.contains(&index));
        }

        // The invalid bits in the final u64 must never be returned.
        assert!(
            allocations.iter().all(|&index| index < N),
            "allocator returned an index outside its declared capacity"
        );
    }
}
