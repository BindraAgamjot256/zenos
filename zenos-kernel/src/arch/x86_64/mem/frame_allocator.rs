use core::{
    cell::OnceCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use log::{debug, info, trace};

use crate::arch::PhysAddr;
use crate::arch::VirtAddr;

const PAGE_SIZE: usize = 4096;
const FRAMES_PER_BITMAP: usize = 64;

/// Errors that can occur during frame allocation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAllocError {
    /// Allocator has not been initialized yet.
    /// Operations must wait until `init()` completes.
    Uninitialized,

    /// No free physical frames remain.
    /// All available physical memory has been allocated.
    OutOfMemory,

    /// Physical address is outside the range covered by the bitmap.
    /// The address exceeds the highest known physical address.
    UnmanagedFrame,

    /// Bitmap storage placement failed during initialization.
    /// Could not find a suitable location to store allocator metadata.
    BitmapPlacementFailed,

    /// Attempted a zero-sized range operation.
    /// Range operations must cover at least one frame.
    EmptyRange,

    /// Address calculation overflowed.
    /// The address range computation wrapped around.
    AddressOverflow,

    /// Allocation requested is too long.
    TooLong,
}

#[derive(Debug)]
struct Bitmap {
    /// Allocation bitmap: 1 = used/reserved, 0 = free.
    /// Each bit represents one 4 KiB physical frame.
    map: AtomicU64,

    /// Base frame index covered by this bitmap.
    /// This bitmap covers frames [base, base + 64).
    base: usize,
}

/// A bitmap-based physical frame allocator.
///
/// This allocator manages physical memory using a bitmap approach. Each bit in a 64-bit
/// bitmap represents one 4 KiB physical frame. Multiple bitmaps are chained together to
/// cover the entire addressable physical space.
///
/// The allocator is initialized once during early boot with the memory map from the
/// bootloader. After initialization, it can safely allocate and deallocate frames in
/// a multi-threaded environment using atomic operations.
///
/// # Architecture
/// - Each bitmap covers 64 frames (256 KiB of physical memory)
/// - Allocation uses a first-fit strategy with a hint to avoid scanning from the start
/// - All operations are atomic for SMP safety
/// - The null page (frame 0) is always reserved to catch null pointer dereferences
#[derive(Debug)]
pub struct FrameAllocator {
    head: OnceCell<&'static mut [Bitmap]>,

    /// Bitmap index allocation hint
    hint: AtomicUsize,
}

impl FrameAllocator {
    /// Creates a new uninitialized frame allocator.
    ///
    /// The allocator must be initialized by calling `init()` before it can be used.
    /// This is a const function suitable for static initialization.
    pub const fn new_uninit() -> Self {
        Self {
            head: OnceCell::new(),
            hint: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn align_down(addr: usize, align: usize) -> usize {
        addr & !(align - 1)
    }

    #[inline]
    fn align_up(addr: usize, align: usize) -> usize {
        (addr + align - 1) & !(align - 1)
    }

    #[inline]
    fn frame_index(addr: usize) -> usize {
        addr / PAGE_SIZE
    }

    #[inline]
    fn bitmap_mask_for_frame_range(start_frame: usize, count: usize, bitmap_index: usize) -> u64 {
        let end_frame = start_frame + count;
        let start_bitmap = start_frame / FRAMES_PER_BITMAP;
        let end_bitmap = (end_frame - 1) / FRAMES_PER_BITMAP;

        let start_bit = if bitmap_index == start_bitmap {
            start_frame % FRAMES_PER_BITMAP
        } else {
            0
        };

        let end_bit = if bitmap_index == end_bitmap {
            let end_offset = end_frame % FRAMES_PER_BITMAP;
            if end_offset == 0 {
                FRAMES_PER_BITMAP
            } else {
                end_offset
            }
        } else {
            FRAMES_PER_BITMAP
        };

        if start_bit == 0 && end_bit == FRAMES_PER_BITMAP {
            u64::MAX
        } else {
            (((1u64 << (end_bit - start_bit)) - 1) << start_bit)
        }
    }

    fn try_alloc_range_at(&self, start_frame: usize, count: usize, bitmaps: &[Bitmap]) -> bool {
        let end_frame = start_frame + count;
        let start_bitmap = start_frame / FRAMES_PER_BITMAP;
        let end_bitmap = (end_frame - 1) / FRAMES_PER_BITMAP;

        // Fast reject if any part of the range is already allocated.
        for bitmap_index in start_bitmap..=end_bitmap {
            let mask = Self::bitmap_mask_for_frame_range(start_frame, count, bitmap_index);
            let old = bitmaps[bitmap_index].map.load(Ordering::Acquire);
            if old & mask != 0 {
                return false;
            }
        }

        for bitmap_index in start_bitmap..=end_bitmap {
            let mask = Self::bitmap_mask_for_frame_range(start_frame, count, bitmap_index);

            loop {
                let old = bitmaps[bitmap_index].map.load(Ordering::Acquire);

                if old & mask != 0 {
                    // Another thread allocated the region before we could claim it.
                    // Roll back any previous successful allocations for this range.
                    for rollback_bitmap in start_bitmap..bitmap_index {
                        let rollback_mask =
                            Self::bitmap_mask_for_frame_range(start_frame, count, rollback_bitmap);
                        bitmaps[rollback_bitmap]
                            .map
                            .fetch_and(!rollback_mask, Ordering::AcqRel);
                    }
                    return false;
                }

                let new = old | mask;
                if bitmaps[bitmap_index]
                    .map
                    .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    break;
                }
            }
        }

        self.hint.store(start_bitmap, Ordering::Relaxed);
        true
    }

    /// Initializes the frame allocator with the system memory map.
    ///
    /// This function must be called exactly once during early boot before interrupts
    /// or multi-processor initialization. It performs the following steps:
    /// 1. Analyzes the memory map to determine total addressable space
    /// 2. Allocates bitmap storage from usable memory
    /// 3. Initializes all memory as reserved
    /// 4. Marks usable regions as free
    /// 5. Re-reserves the bitmap storage itself
    /// 6. Reserves the null page
    ///
    /// # Arguments
    /// * `mem_map` - Memory map from bootloader with (start, len, usable) tuples
    /// * `phys_offset` - Virtual offset for accessing physical memory (higher half kernel)
    ///
    /// # Safety
    /// Must be called only once during early boot before SMP or interrupts are enabled.
    /// The memory map must be valid and remain unchanged during initialization.
    pub unsafe fn init(
        &self,
        mem_map: impl DoubleEndedIterator<Item = (usize, usize, bool)> + Clone,
        phys_offset: usize,
    ) -> Result<(), FrameAllocError> {
        info!("initializing frame allocator");

        let mut total_usable = 0usize;
        let mut max_phys = 0usize;

        for (start, len, usable) in mem_map.clone() {
            let end = start
                .checked_add(len)
                .ok_or(FrameAllocError::AddressOverflow)?;

            max_phys = max_phys.max(end);

            if usable {
                total_usable = total_usable.saturating_add(len);

                trace!(
                    "usable region: start={:#x}, len={:#x} ({} KiB)",
                    start,
                    len,
                    len / 1024
                );
            }
        }

        info!(
            "detected usable physical memory: {} MiB",
            total_usable / 1024 / 1024
        );

        info!("highest physical address: {:#x}", max_phys);

        //
        // Coverage determined by highest physical address.
        //
        let num_frames = Self::align_up(max_phys, PAGE_SIZE) / PAGE_SIZE;

        let bitmap_count = (num_frames + FRAMES_PER_BITMAP - 1) / FRAMES_PER_BITMAP;

        let bitmap_bytes = bitmap_count * core::mem::size_of::<Bitmap>();

        debug!(
            "allocator requires {} bitmaps ({} bytes)",
            bitmap_count, bitmap_bytes
        );

        //
        // Find bitmap storage placement.
        //
        for (start, len, usable) in mem_map.clone().rev() {
            if !usable {
                continue;
            }

            if len < bitmap_bytes {
                continue;
            }

            let region_end = start
                .checked_add(len)
                .ok_or(FrameAllocError::AddressOverflow)?;

            let addr = Self::align_down(region_end - bitmap_bytes, PAGE_SIZE);

            if addr < start {
                continue;
            }

            info!("placing allocator bitmap at physical address {:#x}", addr);

            let ptr = (addr + phys_offset) as *mut MaybeUninit<Bitmap>;

            let raw = unsafe { core::slice::from_raw_parts_mut(ptr, bitmap_count) };

            //
            // IMPORTANT:
            //
            // Initialize ALL memory as reserved.
            //
            for (i, slot) in raw.iter_mut().enumerate() {
                slot.write(Bitmap {
                    map: AtomicU64::new(u64::MAX),
                    base: i * FRAMES_PER_BITMAP,
                });

                trace!(
                    "bitmap {} covers frames {}..{}",
                    i,
                    i * FRAMES_PER_BITMAP,
                    (i + 1) * FRAMES_PER_BITMAP - 1
                );
            }

            let init_slice = unsafe { &mut *(raw as *mut [MaybeUninit<_>] as *mut [_]) };

            self.head
                .set(init_slice)
                .map_err(|_| FrameAllocError::BitmapPlacementFailed)?;

            info!("bitmap storage initialized");

            //
            // Unreserve all usable memory regions.
            //
            info!("unreserving usable memory");

            for (region_start, region_len, usable) in mem_map.clone() {
                if !usable {
                    continue;
                }

                self.unreserve_range(PhysAddr::new(region_start as u64), region_len)?;
            }

            //
            // Re-reserve allocator bitmap storage.
            //
            info!("reserving allocator bitmap backing pages");

            self.reserve_range(PhysAddr::new(addr as u64), bitmap_bytes)?;

            //
            // Reserve null page.
            //
            self.reserve(PhysAddr::new(0))?;

            info!("frame allocator initialization complete");

            return Ok(());
        }

        Err(FrameAllocError::BitmapPlacementFailed)
    }

    /// Reserves a single physical frame.
    ///
    /// Marks the frame at the given address as reserved (allocated).
    /// The address must be page-aligned.
    ///
    /// # Arguments
    /// * `addr` - Physical address of the frame to reserve
    ///
    /// # Returns
    /// `Ok(())` if successful, or an error if the frame is invalid/unmanaged
    ///
    /// # Example
    /// ```no_run
    /// allocator.reserve(PhysAddr::new(0x1000))?;
    /// ```
    #[inline]
    pub fn reserve(&self, addr: PhysAddr) -> Result<(), FrameAllocError> {
        self.reserve_range(addr, PAGE_SIZE)
    }

    /// Frees a single physical frame.
    ///
    /// Marks the frame at the given address as free (unallocated).
    /// The address must be page-aligned.
    ///
    /// # Arguments
    /// * `addr` - Physical address of the frame to free
    ///
    /// # Returns
    /// `Ok(())` if successful, or an error if the frame is invalid/unmanaged
    ///
    /// # Example
    /// ```no_run
    /// allocator.free(PhysAddr::new(0x1000))?;
    /// ```
    #[inline]
    pub fn free(&self, addr: PhysAddr) -> Result<(), FrameAllocError> {
        self.unreserve_range(addr, PAGE_SIZE)
    }

    /// Reserves a range of physical frames.
    ///
    /// Marks all frames in the given range as reserved. The range is automatically
    /// aligned to frame boundaries (4 KiB).
    ///
    /// # Arguments
    /// * `start` - Starting physical address
    /// * `len` - Number of bytes to reserve
    ///
    /// # Returns
    /// `Ok(())` if successful, or an error if the range is invalid
    #[inline]
    pub fn reserve_range(&self, start: PhysAddr, len: usize) -> Result<(), FrameAllocError> {
        self.modify_range(start, len, true)
    }

    /// Frees a range of physical frames.
    ///
    /// Marks all frames in the given range as free. The range is automatically
    /// aligned to frame boundaries (4 KiB).
    ///
    /// # Arguments
    /// * `start` - Starting physical address
    /// * `len` - Number of bytes to free
    ///
    /// # Returns
    /// `Ok(())` if successful, or an error if the range is invalid
    #[inline]
    pub fn unreserve_range(&self, start: PhysAddr, len: usize) -> Result<(), FrameAllocError> {
        self.modify_range(start, len, false)
    }

    fn modify_range(
        &self,
        start: PhysAddr,
        len: usize,
        reserve: bool,
    ) -> Result<(), FrameAllocError> {
        let Some(bitmaps) = self.head.get() else {
            return Err(FrameAllocError::Uninitialized);
        };

        if len == 0 {
            return Err(FrameAllocError::EmptyRange);
        }

        let end_raw = start
            .as_usize()
            .checked_add(len)
            .ok_or(FrameAllocError::AddressOverflow)?;

        let start_addr = Self::align_down(start.as_usize(), PAGE_SIZE);

        let end_addr = Self::align_up(end_raw, PAGE_SIZE);

        let start_frame = Self::frame_index(start_addr);

        let end_frame = Self::frame_index(end_addr);

        if start_frame >= end_frame {
            return Err(FrameAllocError::EmptyRange);
        }

        let start_bitmap = start_frame / FRAMES_PER_BITMAP;

        let end_bitmap = (end_frame - 1) / FRAMES_PER_BITMAP;

        if end_bitmap >= bitmaps.len() {
            return Err(FrameAllocError::UnmanagedFrame);
        }

        for bi in start_bitmap..=end_bitmap {
            let bitmap = &bitmaps[bi];

            let bitmap_start = bitmap.base;
            let bitmap_end = bitmap.base + FRAMES_PER_BITMAP;

            let local_start = start_frame.max(bitmap_start) - bitmap_start;

            let local_end = end_frame.min(bitmap_end) - bitmap_start;

            let bits = local_end - local_start;

            if bits == 0 {
                continue;
            }

            let mask = if bits >= 64 {
                u64::MAX
            } else {
                ((1u64 << bits) - 1) << local_start
            };

            if reserve {
                bitmap.map.fetch_or(mask, Ordering::AcqRel);
            } else {
                bitmap.map.fetch_and(!mask, Ordering::AcqRel);
            }

            trace!(
                "{} bitmap {} mask={:#018x}",
                if reserve { "reserved" } else { "unreserved" },
                bi,
                mask
            );
        }

        self.hint.store(start_bitmap, Ordering::Relaxed);

        Ok(())
    }

    /// Allocates a single free physical frame.
    ///
    /// Scans the bitmap for the first free frame and marks it as allocated.
    /// Uses a hint from the previous allocation to avoid always scanning from the start.
    /// Falls back to scanning from the beginning if needed.
    ///
    /// # Returns
    /// `Ok(PhysAddr)` with the allocated frame address, or an error if no frames are free
    ///
    /// # Example
    /// ```no_run
    /// match allocator.alloc() {
    ///     Ok(addr) => println!("Allocated frame at {}", addr),
    ///     Err(FrameAllocError::OutOfMemory) => println!("No free frames"),
    ///     Err(e) => println!("Error: {:?}", e),
    /// }
    /// ```
    pub fn alloc(&self) -> Result<PhysAddr, FrameAllocError> {
        self.alloc_range(1)
    }

    /// Allocates a range of count frames (count * PAGE_SIZE bytes).
    /// See `alloc()` for more details.
    pub fn alloc_range(&self, count: usize) -> Result<PhysAddr, FrameAllocError> {
        if count == 0 {
            return Err(FrameAllocError::EmptyRange);
        }

        let bitmaps = self.head.get().ok_or(FrameAllocError::Uninitialized)?;

        let total_frames = bitmaps
            .len()
            .checked_mul(FRAMES_PER_BITMAP)
            .ok_or(FrameAllocError::TooLong)?;
        if count > total_frames {
            return Err(FrameAllocError::TooLong);
        }

        let start_hint = self.hint.load(Ordering::Relaxed).min(bitmaps.len());

        trace!(
            "range allocation start hint: {}, count: {}",
            start_hint, count
        );

        let start_frame_hint = start_hint * FRAMES_PER_BITMAP;
        let max_frame = total_frames - count;

        for pass in 0..2 {
            let range_start = if pass == 0 { start_frame_hint } else { 0 };
            let range_end = if pass == 0 {
                total_frames
            } else {
                start_frame_hint
            };

            let mut frame = range_start;
            while frame <= max_frame && frame < range_end {
                if self.try_alloc_range_at(frame, count, bitmaps) {
                    let addr = PhysAddr::new((frame * PAGE_SIZE) as u64);

                    debug!(
                        "allocated {} frame(s) starting at frame {} ({:#x})",
                        count,
                        frame,
                        addr.as_usize()
                    );

                    return Ok(addr);
                }
                frame += 1;
            }
        }

        Err(FrameAllocError::OutOfMemory)
    }
}

unsafe impl Send for FrameAllocator {}
unsafe impl Sync for FrameAllocator {}
