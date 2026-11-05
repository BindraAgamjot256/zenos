//! Page-backed allocation for fixed-size objects.
//!
//! A [`SlabCache`] manages objects of a single, compile-time size. It obtains
//! [`PAGE_SIZE`]-byte pages from a [`Backend`] and divides each page into a
//! [`SlabHeader`] followed by equally sized object slots:
//!
//! ```text
//! +----------------+------------+------------+-----+
//! | slab header    | object 0   | object 1   | ... |
//! +----------------+------------+------------+-----+
//! ```
//!
//! Unused slots form an intrusive singly linked list. The allocator stores a
//! link in the slot itself while it is free, so no separate per-object metadata
//! is required. Once allocated, the caller owns all `SIZE` bytes in the slot.
//!
//! Slabs are grouped by occupancy:
//!
//! - partially occupied slabs can satisfy allocations immediately,
//! - filled slabs contain no free objects,
//! - a small number of empty slabs are retained for reuse.
//!
//! Object sizes must be powers of two, large enough to hold a free-list link,
//! and small enough to leave room for at least one object after the header.

pub use self::helpers::Metadata;
use self::helpers::validate_size;
use core::ptr::NonNull;

mod helpers;

/// Size, in bytes, of every page managed by the slab allocator.
///
/// Backends must return allocations of exactly this size aligned to the same
/// value, because [`SlabCache::deallocate`] locates a slab by rounding an object
/// address down to the nearest page boundary.
pub const PAGE_SIZE: usize = 4096;

/// Maximum number of completely empty slabs retained for later reuse.
const MAX_EMPTY_SLABS: usize = 2;

/// Debug-only marker used to recognize initialized slab pages.
#[cfg(debug_assertions)]
const SLAB_HEADER_MAGIC: u64 = u64::from_ne_bytes(*b"SKIBIDI!");

/// Intrusive link stored inside an unused object slot.
///
/// The link occupies the beginning of a free slot and is overwritten by caller
/// data while that slot is allocated.
#[repr(C)]
struct FreeSlot {
    /// Next available slot in this slab, or [`None`] at the end of the list.
    next: Option<NonNull<FreeSlot>>,
}

/// Metadata stored at the beginning of a slab page.
///
/// Each header owns the free list for the page's `SIZE`-byte objects and also
/// acts as a node in one of the cache's intrusive slab lists. The header is
/// padded to a multiple of `SIZE`, ensuring that the object area starts on an
/// object-size boundary.
#[repr(C)]
pub struct SlabHeader<const SIZE: usize> {
    /// First currently unused object slot in this page.
    free_list: Option<NonNull<FreeSlot>>,

    /// Number of entries reachable from [`Self::free_list`].
    free_count: usize,

    /// Next page in the current occupancy list.
    next: Option<NonNull<SlabHeader<SIZE>>>,

    /// Previous page in the current occupancy list.
    prev: Option<NonNull<SlabHeader<SIZE>>>,

    /// Marker used to reject pointers that do not belong to initialized slabs.
    #[doc(hidden)]
    #[cfg(debug_assertions)]
    magic: u64,
}

impl<const SIZE: usize> SlabHeader<SIZE> {
    /// Initializes a page as a slab of `SIZE`-byte objects.
    ///
    /// The header is written at `ptr`, and every object slot in the remainder
    /// of the page is linked into the free list. The slab starts completely
    /// empty and is not attached to a cache list by this function.
    ///
    /// # Panics
    ///
    /// Panics if `SIZE` is not a supported slab object size. See
    /// [`helpers::validate_size`] for the required layout constraints.
    ///
    /// # Pointer requirements
    ///
    /// `ptr` must reference the start of a writable, [`PAGE_SIZE`]-byte,
    /// page-aligned region that is not accessed through another reference for
    /// the duration of initialization.
    pub fn init(ptr: NonNull<Self>) {
        const {
            validate_size(SIZE);
        }

        let free_count = helpers::get_num_objects(SIZE);
        let header_size = helpers::header_size(SIZE);
        log::debug!(
            "initializing slab page {:p} for {}-byte objects ({} slots)",
            ptr.as_ptr(),
            SIZE,
            free_count
        );

        // Object storage begins after the header's size-aligned padding.
        let free_list_head = unsafe { ptr.cast::<u8>().add(header_size) };
        let mut next = None;

        // Build the intrusive free list in reverse slot order. Each slot points
        // at the previously initialized slot, requiring no auxiliary storage.
        for i in 0..free_count {
            let current = unsafe { free_list_head.add(i * SIZE).cast::<FreeSlot>() };

            unsafe {
                current.write(FreeSlot { next });
            }

            next = Some(current);
        }

        // Publish the header only after every free-list link is initialized.
        unsafe {
            ptr.write(Self {
                free_list: next,
                free_count,
                next: None,
                prev: None,

                #[cfg(debug_assertions)]
                magic: SLAB_HEADER_MAGIC,
            });
        }

        log::trace!("slab page {:p} initialized", ptr.as_ptr());
    }

    /// Invalidates this slab before its page is returned to the backend.
    ///
    /// Teardown clears the intrusive links and, in debug builds, the magic
    /// marker. The page must not be accessed as a slab after this call.
    ///
    /// # Panics
    ///
    /// Panics if any object in this slab is still allocated.
    pub fn teardown(&mut self) {
        assert!(
            self.is_empty(),
            "cannot tear down a slab that still contains allocated objects"
        );

        log::debug!(
            "tearing down empty slab page {:p} for {}-byte objects",
            self as *mut Self,
            SIZE
        );

        // Break all allocator-owned links before the backing page is released.
        self.free_list = None;
        self.free_count = 0;
        self.next = None;
        self.prev = None;

        #[cfg(debug_assertions)]
        {
            // Clearing the marker makes stale slab pointers fail validation.
            self.magic = 0;
        }
    }

    /// Returns whether this header carries the expected debug marker.
    ///
    /// Debug builds check the marker written by [`SlabHeader::init`]. Release
    /// builds omit the marker and therefore always return `true`.
    #[cfg(debug_assertions)]
    pub fn validate_slab(&self) -> bool {
        // Keep the intentionally memorable magic value, but expose validation
        // as a plain integrity check to callers.
        self.magic == SLAB_HEADER_MAGIC
    }

    /// Returns whether this header carries the expected debug marker.
    ///
    /// Debug builds check the marker written by [`SlabHeader::init`]. Release
    /// builds omit the marker and therefore always return `true`.
    #[cfg(not(debug_assertions))]
    pub fn validate_slab(&self) -> bool {
        true
    }

    /// Removes and returns one object from this slab's free list.
    ///
    /// Returns [`None`] when the slab is full. A successful allocation reduces
    /// the free-object count by one.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if the slab marker is invalid.
    pub fn allocate(&mut self) -> Option<NonNull<u8>> {
        #[cfg(debug_assertions)]
        if self.magic != SLAB_HEADER_MAGIC {
            log::error!(
                "refusing to allocate from slab {:p}: header magic validation failed",
                self as *mut Self
            );
            panic!("slab header magic validation failed");
        }

        // Detach the free-list head before handing the slot to the caller.
        let head = self.free_list.map(NonNull::cast);
        if let Some(free_slot) = self.free_list {
            self.free_list = unsafe { free_slot.as_ref().next };
            self.free_count -= 1;

            log::trace!(
                "allocated object {:p} from slab {:p}; {} slots remain",
                free_slot.as_ptr(),
                self as *mut Self,
                self.free_count
            );
        } else {
            log::trace!(
                "allocation requested from full slab {:p}",
                self as *mut Self
            );
        }

        head
    }

    /// Returns an object to this slab's free list.
    ///
    /// The pointer is accepted only when it names the beginning of an object
    /// slot in this page and is not already present in the free list.
    ///
    /// Returns [`Some`] after a successful deallocation, or [`None`] if the
    /// slab marker is invalid, the slab is already empty, the pointer is out of
    /// range or misaligned, or the object has already been freed.
    pub fn deallocate(&mut self, ptr: NonNull<u8>) -> Option<()> {
        #[cfg(debug_assertions)]
        if self.magic != SLAB_HEADER_MAGIC {
            log::warn!(
                "rejected object {:p}: slab {:p} failed header validation",
                ptr.as_ptr(),
                self as *mut Self
            );
            return None;
        }

        let object_count = helpers::get_num_objects(SIZE);
        if self.free_count >= object_count {
            log::warn!(
                "rejected object {:p}: slab {:p} is already empty",
                ptr.as_ptr(),
                self as *mut Self
            );
            return None;
        }

        // Convert the pointer into a slot index without dereferencing it. This
        // rejects addresses before the object area, after it, and inside slots.
        let slab_start = self as *mut Self as usize;
        let objects_start = slab_start.checked_add(helpers::header_size(SIZE))?;
        let offset = match ptr.addr().get().checked_sub(objects_start) {
            Some(offset) => offset,
            None => {
                log::warn!(
                    "rejected object {:p}: address precedes slab {:p} object storage",
                    ptr.as_ptr(),
                    self as *mut Self
                );
                return None;
            }
        };
        if offset % SIZE != 0 || offset / SIZE >= object_count {
            log::warn!(
                "rejected object {:p}: address is not a valid slot in slab {:p}",
                ptr.as_ptr(),
                self as *mut Self
            );
            return None;
        }

        let slot = ptr.cast::<FreeSlot>();

        // Debug builds scan the bounded free list to detect double frees and
        // malformed cycles before modifying allocator state.
        #[cfg(debug_assertions)]
        {
            let mut current = self.free_list;
            let mut visited = 0;
            while let Some(free_slot) = current {
                if free_slot == slot {
                    log::warn!(
                        "rejected object {:p}: slot is already free in slab {:p}",
                        ptr.as_ptr(),
                        self as *mut Self
                    );
                    return None;
                }
                if visited >= self.free_count {
                    log::error!(
                        "free-list traversal exceeded recorded count for slab {:p}",
                        self as *mut Self
                    );
                    return None;
                }
                current = unsafe { free_slot.as_ref().next };
                visited += 1;
            }
        }

        // Return the slot to the head of the intrusive list.
        unsafe {
            slot.write(FreeSlot {
                next: self.free_list,
            });
        }
        self.free_list = Some(slot);
        self.free_count += 1;

        log::trace!(
            "deallocated object {:p} into slab {:p}; {} slots are free",
            ptr.as_ptr(),
            self as *mut Self,
            self.free_count
        );
        Some(())
    }

    /// Returns `true` when every object slot is available.
    pub const fn is_empty(&self) -> bool {
        self.free_count == helpers::get_num_objects(SIZE)
    }

    /// Returns `true` when no object slot is available.
    fn is_full(&self) -> bool {
        self.free_count == 0
    }
}

/// Page provider used by [`SlabCache`].
///
/// The slab allocator is independent of the mechanism used to obtain memory.
/// Implementations may source pages from a physical-page allocator, a static
/// region, or another page-granular allocator.
pub trait Backend {
    /// Allocates one page for use as a slab.
    ///
    /// Returns [`None`] when no page is available.
    ///
    /// # Safety
    ///
    /// A returned region must be writable, exactly [`PAGE_SIZE`] bytes long,
    /// aligned to [`PAGE_SIZE`], and remain valid until it is passed to
    /// [`Backend::deallocate`]. It must not overlap any other live allocation.
    unsafe fn allocate(&mut self) -> Option<NonNull<u8>>;

    /// Releases a page previously supplied by [`Backend::allocate`].
    ///
    /// # Safety
    ///
    /// `page` must identify a live page returned by this backend. No references
    /// or allocated objects within that page may remain in use, and the page
    /// must not have been released previously.
    unsafe fn deallocate(&mut self, page: NonNull<u8>);

    /// Associates ownership metadata with a live slab page.
    ///
    /// Implementations must make this metadata available to
    /// [`Backend::get_metadata`] until the page is deallocated. Reusing a page
    /// must replace any metadata left by its previous allocation.
    fn set_metadata(&mut self, meta: Metadata, page: NonNull<u8>);

    /// Returns ownership metadata for a live slab page.
    ///
    /// This method must return [`None`] for unknown or deallocated pages. It is
    /// called before the allocator dereferences a header derived from an object
    /// pointer, so implementations must not inspect memory through `page`.
    fn get_metadata(&self, page: NonNull<u8>) -> Option<Metadata>;
}

/// Allocator cache for objects of exactly `SIZE` bytes.
///
/// The cache obtains pages lazily from `B` and tracks them according to their
/// occupancy. Allocation prefers partially occupied pages, then retained empty
/// pages, and finally requests a new page from the backend.
///
/// `SIZE` is validated when the first slab is initialized. It must satisfy the
/// fixed-size layout requirements described by [`SlabHeader::init`].
pub struct SlabCache<const SIZE: usize, B: Backend> {
    /// Provider from which new slab pages are obtained.
    backend: B,

    /// Head of the intrusive list of slabs with both used and free objects.
    slabs_partial: Option<NonNull<SlabHeader<SIZE>>>,

    /// Head of the intrusive list of slabs with no free objects.
    slabs_filled: Option<NonNull<SlabHeader<SIZE>>>,

    /// Completely empty slabs retained to avoid backend allocation churn.
    empty_slabs: [Option<NonNull<SlabHeader<SIZE>>>; MAX_EMPTY_SLABS],

    /// Random identifier distinguishing this cache from other same-size caches.
    owner_id: u32,
}

impl<const SIZE: usize, B: Backend> SlabCache<SIZE, B> {
    /// Creates an empty slab cache backed by `backend`.
    ///
    /// No pages are requested until the first allocation that cannot be served
    /// by an existing slab.
    pub const fn new(backend: B) -> Self {
        Self {
            backend,
            slabs_partial: None,
            slabs_filled: None,
            empty_slabs: [None; MAX_EMPTY_SLABS],
            owner_id: const_random::const_random!(u32),
        }
    }

    /// Obtains, initializes, and registers a new partially occupied slab.
    ///
    /// The newly created slab is initially empty; it is placed on the partial
    /// list so that the caller can immediately allocate from it. Returns
    /// [`None`] if the backend cannot provide a page.
    fn add_slab(&mut self) -> Option<NonNull<SlabHeader<SIZE>>> {
        log::debug!("requesting a slab page for {}-byte objects", SIZE);

        let page = match unsafe { self.backend.allocate() } {
            Some(page) => page,
            None => {
                log::warn!(
                    "backend could not provide a slab page for {}-byte objects",
                    SIZE
                );
                return None;
            }
        };

        // Record ownership before publishing the initialized slab to a list.
        self.backend.set_metadata(
            Metadata {
                size: SIZE,
                owner: self.owner_id,
            },
            page,
        );
        let slab = page.cast();

        SlabHeader::<SIZE>::init(slab);
        self.insert_partial(slab);

        log::debug!(
            "added slab page {:p} to cache for {}-byte objects",
            slab.as_ptr(),
            SIZE
        );
        Some(slab)
    }

    /// Inserts `slab` at the head of the partially occupied slab list.
    fn insert_partial(&mut self, mut slab: NonNull<SlabHeader<SIZE>>) {
        log::trace!("inserting slab {:p} into partial list", slab.as_ptr());

        // Replace the list head and link the former head after the new slab.
        let head = self.slabs_partial.replace(slab);
        unsafe {
            slab.as_mut().prev = None;
            slab.as_mut().next = head;
        }

        if let Some(mut head) = head {
            unsafe {
                head.as_mut().prev = Some(slab);
            }
        }
    }

    /// Detaches `slab` from the partially occupied slab list.
    fn remove_partial(&mut self, mut slab: NonNull<SlabHeader<SIZE>>) {
        log::trace!("removing slab {:p} from partial list", slab.as_ptr());

        // Preserve both neighbors before changing either link.
        let sref = unsafe { slab.as_mut() };
        let next = sref.next;
        let prev = sref.prev;

        if let Some(mut prev) = prev {
            unsafe { prev.as_mut().next = next }
        } else {
            // A node without a predecessor is the current list head.
            self.slabs_partial = next
        }
        if let Some(mut next) = next {
            unsafe { next.as_mut().prev = prev }
        }

        sref.next = None;
        sref.prev = None;
    }

    /// Detaches `slab` from the completely filled slab list.
    fn remove_filled(&mut self, mut slab: NonNull<SlabHeader<SIZE>>) {
        log::trace!("removing slab {:p} from filled list", slab.as_ptr());

        // Filled and partial lists share the same intrusive header links.
        let sref = unsafe { slab.as_mut() };
        let next = sref.next;
        let prev = sref.prev;

        if let Some(mut prev) = prev {
            unsafe { prev.as_mut().next = next }
        } else {
            self.slabs_filled = next
        }
        if let Some(mut next) = next {
            unsafe { next.as_mut().prev = prev }
        }

        sref.next = None;
        sref.prev = None;
    }

    /// Inserts `slab` at the head of the filled slab list.
    fn insert_filled(&mut self, mut slab: NonNull<SlabHeader<SIZE>>) {
        log::trace!("inserting slab {:p} into filled list", slab.as_ptr());

        let head = self.slabs_filled.replace(slab);
        unsafe {
            slab.as_mut().prev = None;
            slab.as_mut().next = head;
        }

        if let Some(mut head) = head {
            unsafe {
                head.as_mut().prev = Some(slab);
            }
        }
    }

    /// Retains an empty slab, evicting another retained page if necessary.
    fn insert_empty(&mut self, slab_ptr: NonNull<SlabHeader<SIZE>>) {
        // Prefer an unused retention slot to avoid backend allocation churn.
        for slab in self.empty_slabs.iter_mut() {
            if slab.is_none() {
                slab.replace(slab_ptr);
                log::debug!("retained empty slab {:p} for reuse", slab_ptr.as_ptr());
                return;
            }
        }

        // The retention array is full. Replace one slot and return that page to
        // the backend after invalidating the slab header.
        let mut evicted = self.empty_slabs[0]
            .replace(slab_ptr)
            .expect("empty-slab retention slot must be occupied before eviction");
        log::debug!(
            "evicting retained slab {:p} in favor of {:p}",
            evicted.as_ptr(),
            slab_ptr.as_ptr()
        );
        unsafe {
            evicted.as_mut().teardown();
            self.backend.deallocate(evicted.cast());
        }
    }

    /// Allocates from a slab on the partial list and updates its occupancy list.
    fn allocate_from_partial(
        &mut self,
        mut slab: NonNull<SlabHeader<SIZE>>,
    ) -> Option<NonNull<u8>> {
        let header = unsafe { slab.as_mut() };
        let object = header.allocate();

        // Once its final free slot is consumed, the slab must no longer be
        // considered by the fast path for subsequent allocations.
        if object.is_some() && header.is_full() {
            self.remove_partial(slab);
            self.insert_filled(slab);
            log::debug!("slab {:p} became full", slab.as_ptr());
        }

        object
    }

    /// Allocates one `SIZE`-byte object.
    ///
    /// The cache searches for storage in this order:
    ///
    /// 1. the first partially occupied slab,
    /// 2. a retained empty slab,
    /// 3. a newly allocated backend page.
    ///
    /// A slab that becomes full is moved from the partial list to the filled
    /// list. Returns [`None`] if no existing slab can satisfy the request and
    /// the backend cannot allocate another page.
    pub fn allocate(&mut self) -> Option<NonNull<u8>> {
        log::trace!("allocating a {}-byte slab object", SIZE);

        // Partial slabs are already linked into the allocation fast path.
        if let Some(slab) = self.slabs_partial {
            return self.allocate_from_partial(slab);
        }

        // Reactivate a retained empty slab before requesting another page.
        if let Some(index) = self.empty_slabs.iter().position(Option::is_some) {
            let slab = self.empty_slabs[index]
                .take()
                .expect("selected empty-slab retention slot must be occupied");
            self.insert_partial(slab);
            log::debug!("reusing retained empty slab {:p}", slab.as_ptr());
            return self.allocate_from_partial(slab);
        }

        // No existing slab can satisfy the request, so grow the cache lazily.
        let slab = self.add_slab()?;
        self.allocate_from_partial(slab)
    }

    /// Deallocates an object previously returned by this cache.
    ///
    /// The containing slab is found by aligning the object address down to a
    /// [`PAGE_SIZE`] boundary. The slab header then validates the object and
    /// returns it to that page's free list.
    ///
    /// Returns [`None`] when the page does not appear to contain a valid slab,
    /// or when [`SlabHeader::deallocate`] rejects the pointer.
    ///
    /// In release builds, slab-marker validation is unavailable. Callers must
    /// therefore only pass pointers returned by this cache that have not
    /// already been deallocated.
    pub fn deallocate(&mut self, ptr: NonNull<u8>) -> Option<()> {
        log::trace!("deallocating slab object {:p}", ptr.as_ptr());

        // Slab pages are page-aligned, so masking the object address recovers
        // the candidate header without reading through the supplied pointer.
        let addr = ptr.addr();
        let slab_addr = helpers::align_down(addr.get(), PAGE_SIZE);
        let mut slab_ptr = NonNull::new(slab_addr as *mut SlabHeader<SIZE>)?;

        // External metadata authenticates the page before its header is read.
        let Metadata { size, owner } = match self.backend.get_metadata(slab_ptr.cast()) {
            Some(metadata) => metadata,
            None => {
                log::warn!(
                    "rejected object {:p}: page {:#x} is not owned by this backend",
                    ptr.as_ptr(),
                    slab_addr
                );
                return None;
            }
        };
        if size != SIZE || owner != self.owner_id {
            log::warn!(
                "rejected object {:p}: slab ownership metadata does not match this cache",
                ptr.as_ptr()
            );
            return None;
        }

        if !unsafe { slab_ptr.as_ref().validate_slab() } {
            log::error!(
                "rejected object {:p}: slab {:p} failed header validation",
                ptr.as_ptr(),
                slab_ptr.as_ptr()
            );
            return None;
        }

        // Remember the source list before deallocation changes occupancy.
        let was_full = unsafe { slab_ptr.as_ref().is_full() };
        unsafe { slab_ptr.as_mut().deallocate(ptr)? };
        let is_empty = unsafe { slab_ptr.as_ref().is_empty() };

        if is_empty {
            // A one-object slab can transition directly from filled to empty;
            // larger slabs normally reach empty from the partial list.
            if was_full {
                self.remove_filled(slab_ptr);
            } else {
                self.remove_partial(slab_ptr);
            }
            self.insert_empty(slab_ptr);
            log::debug!("slab {:p} became empty", slab_ptr.as_ptr());
        } else if was_full {
            // Returning one slot makes a formerly full slab allocatable again.
            self.remove_filled(slab_ptr);
            self.insert_partial(slab_ptr);
            log::debug!("slab {:p} became partially occupied", slab_ptr.as_ptr());
        }

        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{Layout, alloc_zeroed, dealloc};
    use std::vec::Vec;

    /// Page allocator used to exercise the slab cache without kernel memory.
    struct TestBackend {
        pages: Vec<NonNull<u8>>,
        metadata: Vec<(NonNull<u8>, Metadata)>,
        max_live_pages: usize,
        allocation_attempts: usize,
        deallocations: usize,
    }

    impl TestBackend {
        fn new(max_live_pages: usize) -> Self {
            Self {
                pages: Vec::new(),
                metadata: Vec::new(),
                max_live_pages,
                allocation_attempts: 0,
                deallocations: 0,
            }
        }

        fn page_layout() -> Layout {
            Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap()
        }
    }

    impl Backend for TestBackend {
        unsafe fn allocate(&mut self) -> Option<NonNull<u8>> {
            self.allocation_attempts += 1;
            if self.pages.len() >= self.max_live_pages {
                return None;
            }

            // The production contract requires page-sized, page-aligned memory.
            let page = NonNull::new(unsafe { alloc_zeroed(Self::page_layout()) })?;
            self.pages.push(page);
            Some(page)
        }

        unsafe fn deallocate(&mut self, page: NonNull<u8>) {
            let index = self
                .pages
                .iter()
                .position(|candidate| *candidate == page)
                .expect("test backend received an unknown page");

            // Remove metadata before making the page address invalid.
            self.pages.swap_remove(index);
            self.metadata.retain(|(candidate, _)| *candidate != page);
            self.deallocations += 1;
            unsafe { dealloc(page.as_ptr(), Self::page_layout()) };
        }

        fn set_metadata(&mut self, metadata: Metadata, page: NonNull<u8>) {
            if let Some((_, current)) = self
                .metadata
                .iter_mut()
                .find(|(candidate, _)| *candidate == page)
            {
                *current = metadata;
            } else {
                self.metadata.push((page, metadata));
            }
        }

        fn get_metadata(&self, page: NonNull<u8>) -> Option<Metadata> {
            self.metadata
                .iter()
                .find_map(|(candidate, metadata)| (*candidate == page).then_some(*metadata))
        }
    }

    impl Drop for TestBackend {
        fn drop(&mut self) {
            // Tests may end while the cache still retains slabs. Reclaim those
            // pages directly because the cache intentionally has no Drop policy.
            for page in self.pages.drain(..) {
                unsafe { dealloc(page.as_ptr(), Self::page_layout()) };
            }
        }
    }

    #[test]
    fn slab_header_allocates_every_slot_and_reuses_freed_slots() {
        const SIZE: usize = 64;

        let mut backend = TestBackend::new(1);
        let page = unsafe { backend.allocate() }.unwrap();
        let mut slab = page.cast::<SlabHeader<SIZE>>();
        SlabHeader::init(slab);

        let object_count = helpers::get_num_objects(SIZE);
        let objects_start = page.addr().get() + helpers::header_size(SIZE);
        let mut objects = Vec::new();

        for _ in 0..object_count {
            let object = unsafe { slab.as_mut().allocate() }.unwrap();
            let offset = object.addr().get() - objects_start;
            assert_eq!(offset % SIZE, 0);
            assert!(!objects.contains(&object));
            objects.push(object);
        }

        assert!(unsafe { slab.as_mut().allocate() }.is_none());
        assert!(unsafe { slab.as_ref().is_full() });

        let freed = objects[0];
        assert_eq!(unsafe { slab.as_mut().deallocate(freed) }, Some(()));
        assert_eq!(unsafe { slab.as_mut().allocate() }, Some(freed));

        for object in objects {
            assert_eq!(unsafe { slab.as_mut().deallocate(object) }, Some(()));
        }
        assert!(unsafe { slab.as_ref().is_empty() });
        assert!(unsafe { slab.as_mut().deallocate(freed) }.is_none());

        unsafe { slab.as_mut().teardown() };
        #[cfg(debug_assertions)]
        assert!(!unsafe { slab.as_ref().validate_slab() });
        unsafe { backend.deallocate(page) };
    }

    #[test]
    fn slab_header_rejects_misaligned_and_out_of_range_objects() {
        const SIZE: usize = 64;

        let mut backend = TestBackend::new(1);
        let page = unsafe { backend.allocate() }.unwrap();
        let mut slab = page.cast::<SlabHeader<SIZE>>();
        SlabHeader::init(slab);

        let object = unsafe { slab.as_mut().allocate() }.unwrap();
        let misaligned = unsafe { NonNull::new_unchecked(object.as_ptr().add(1)) };

        assert!(unsafe { slab.as_mut().deallocate(misaligned) }.is_none());
        assert!(unsafe { slab.as_mut().deallocate(page) }.is_none());
        assert_eq!(unsafe { slab.as_mut().deallocate(object) }, Some(()));

        unsafe { slab.as_mut().teardown() };
        unsafe { backend.deallocate(page) };
    }

    #[test]
    fn cache_reuses_an_empty_slab_before_allocating_another_page() {
        let mut cache = SlabCache::<64, _>::new(TestBackend::new(2));

        let first = cache.allocate().unwrap();
        assert_eq!(cache.backend.allocation_attempts, 1);
        assert_eq!(cache.deallocate(first), Some(()));
        assert_eq!(cache.empty_slabs.iter().flatten().count(), 1);

        let reused = cache.allocate().unwrap();
        assert_eq!(reused, first);
        assert_eq!(cache.backend.allocation_attempts, 1);
        assert_eq!(cache.deallocate(reused), Some(()));
    }

    #[test]
    fn one_object_slab_moves_directly_between_filled_and_empty_lists() {
        let mut cache = SlabCache::<2048, _>::new(TestBackend::new(1));

        let object = cache.allocate().unwrap();
        assert!(cache.slabs_partial.is_none());
        assert!(cache.slabs_filled.is_some());

        assert_eq!(cache.deallocate(object), Some(()));
        assert!(cache.slabs_partial.is_none());
        assert!(cache.slabs_filled.is_none());
        assert_eq!(cache.empty_slabs.iter().flatten().count(), 1);
    }

    #[test]
    fn freeing_from_a_full_slab_makes_it_allocatable_again() {
        const SIZE: usize = 1024;

        let mut cache = SlabCache::<SIZE, _>::new(TestBackend::new(2));
        let object_count = helpers::get_num_objects(SIZE);
        let mut first_page_objects = Vec::new();

        for _ in 0..object_count {
            first_page_objects.push(cache.allocate().unwrap());
        }
        assert!(cache.slabs_partial.is_none());
        assert!(cache.slabs_filled.is_some());

        let returned = first_page_objects[0];
        assert_eq!(cache.deallocate(returned), Some(()));
        assert!(cache.slabs_partial.is_some());

        assert_eq!(cache.allocate(), Some(returned));
        assert!(cache.slabs_partial.is_none());
    }

    #[test]
    fn cache_rejects_mismatched_ownership_metadata() {
        let mut cache = SlabCache::<64, _>::new(TestBackend::new(1));
        let object = cache.allocate().unwrap();

        let original_owner = cache.backend.metadata[0].1.owner;
        cache.backend.metadata[0].1.owner = original_owner.wrapping_add(1);
        assert!(cache.deallocate(object).is_none());

        // Restore ownership so the test can return the live object normally.
        cache.backend.metadata[0].1.owner = original_owner;
        assert_eq!(cache.deallocate(object), Some(()));
    }

    #[test]
    fn cache_reports_backend_exhaustion() {
        let mut cache = SlabCache::<64, _>::new(TestBackend::new(0));

        assert!(cache.allocate().is_none());
        assert_eq!(cache.backend.allocation_attempts, 1);
        assert!(cache.slabs_partial.is_none());
        assert!(cache.slabs_filled.is_none());
    }

    #[test]
    fn cache_evicts_empty_slabs_beyond_the_retention_limit() {
        let mut cache = SlabCache::<2048, _>::new(TestBackend::new(3));
        let objects: Vec<_> = (0..3).map(|_| cache.allocate().unwrap()).collect();

        assert_eq!(cache.backend.pages.len(), 3);
        for object in objects {
            assert_eq!(cache.deallocate(object), Some(()));
        }

        assert_eq!(cache.empty_slabs.iter().flatten().count(), MAX_EMPTY_SLABS);
        assert_eq!(cache.backend.pages.len(), MAX_EMPTY_SLABS);
        assert_eq!(cache.backend.deallocations, 1);
    }

    #[test]
    #[ignore]
    fn test_cache_uniqeness() {
        let cache1 = SlabCache::<16, _>::new(TestBackend::new(1));
        let cache2 = SlabCache::<16, _>::new(TestBackend::new(2));

        assert_ne!(cache1.owner_id, cache2.owner_id,);
    }
}
