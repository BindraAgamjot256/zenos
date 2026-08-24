use crate::buddy;
use core::ptr::NonNull;
use log::*;

pub(crate) struct FreeBlockNode {
    pub(crate) next: Option<NonNull<FreeBlockNode>>,
    pub(crate) prev: Option<NonNull<FreeBlockNode>>,
}

/// Maximum supported buddy order.
///
/// An order-N block contains:
///
/// ```text
/// 2^N pages
/// ```
///
/// For example:
///
/// - order 0 -> 1 page
/// - order 1 -> 2 pages
/// - ...
/// - order 10 -> 1024 pages
pub const MAX_ORDER: usize = 10;

/// Errors returned by [`RawBuddyAllocator::alloc`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// The requested order exceeds [`MAX_ORDER`].
    InvalidOrder,
    /// No free block of the requested order (or any larger order that could
    /// be split down to it) is currently available.
    OutOfMemory,
}

/// Errors returned by [`RawBuddyAllocator::free`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeError {
    /// The requested order exceeds [`MAX_ORDER`].
    InvalidOrder,
    /// The block is already free, so freeing it again would be a double free.
    AlreadyFree,
    /// The block is not currently marked as an allocated block head.
    ///
    /// This variant is returned when the backend can determine that the page
    /// frame does not refer to a block that was handed out by the allocator.
    /// Some backends cannot distinguish this from a merely non-free page; in
    /// that case [`FreeError::AlreadyFree`] is
    /// returned instead.
    NotAllocated,
}

/// Physical page frame number.
///
/// PFNs are used instead of raw pointers inside allocator logic because
/// buddy relationships are naturally expressed through arithmetic on
/// physical page indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFrameNum(usize);

impl PageFrameNum {
    /// Creates a new PFN from a raw page index.
    pub const fn new(pfn: usize) -> Self {
        Self(pfn)
    }

    pub const fn number(&self) -> usize {
        self.0
    }
}

/// Low-level buddy allocator implementation.
///
/// This allocator manages physically contiguous memory blocks using the
/// buddy allocation algorithm.
///
/// Memory is divided into power-of-two sized blocks. Every possible block
/// size has its own free list:
///
/// ```text
/// order 0 -> blocks of 1 page
/// order 1 -> blocks of 2 pages
/// order 2 -> blocks of 4 pages
/// ...
/// order N -> blocks of 2^N pages
/// ```
///
/// Allocation:
///
/// 1. Searches for a free block of the requested order.
/// 2. If unavailable, searches larger orders.
/// 3. Splits larger blocks until the requested size is reached.
///
/// Freeing:
///
/// 1. Marks the block as free.
/// 2. Checks whether its buddy is also free.
/// 3. Merges repeatedly until no merge is possible.
///
/// The allocator itself does not know how physical memory is represented.
/// That responsibility is delegated to [`BuddyBackend`].
pub struct RawBuddyAllocator<B: BuddyBackend> {
    /// Head pointers for every order's doubly-linked free list.
    ///
    /// Each entry points to the first [`FreeBlockNode`] belonging to
    /// that order.
    free_lists: [Option<NonNull<FreeBlockNode>>; MAX_ORDER + 1],

    /// Backend responsible for translating between PFNs and memory.
    backend: B,
}

/// Backend interface required by the buddy allocator.
///
/// The allocator needs metadata about every physical page frame but does
/// not care where that metadata is stored.
///
/// A backend may store allocation state:
///
/// - inside page structures,
/// - in separate arrays,
/// - in bitmap form,
/// - or using any other representation.
///
/// The buddy allocator only requires this interface.
pub trait BuddyBackend {
    /// Converts a virtual pointer into a physical page frame number.
    fn ptr_to_pfn(&self, ptr: *mut u8) -> PageFrameNum;

    /// Converts a physical page frame number into a usable pointer.
    fn pfn_to_ptr(&self, pfn: PageFrameNum) -> *mut u8;

    /// Returns whether the page frame currently belongs to a free block.
    fn is_free(&self, pfn: PageFrameNum) -> bool;

    /// Marks a page frame as free.
    fn mark_free(&mut self, pfn: PageFrameNum);

    /// Marks a page frame as allocated.
    fn mark_allocated(&mut self, pfn: PageFrameNum);

    /// Returns the buddy order stored for a block head.
    fn get_order(&self, pfn: PageFrameNum) -> usize;

    /// Updates the buddy order stored for a block head.
    fn set_order(&mut self, pfn: PageFrameNum, order: usize);
}

impl<B: BuddyBackend> RawBuddyAllocator<B> {
    /// Creates an empty buddy allocator.
    ///
    /// No physical memory is available immediately after creation.
    /// Memory must be inserted through [`RawBuddyAllocator::insert_block`] before allocations
    /// can succeed.
    pub const fn new(backend: B) -> Self {
        Self {
            free_lists: [None; MAX_ORDER + 1],
            backend,
        }
    }

    /// Inserts a free block into its corresponding order list.
    ///
    /// The block is inserted at the head of the linked list.
    ///
    /// The caller must guarantee:
    ///
    /// - `block_pfn` points to the first page of the block.
    /// - `order` correctly describes the block size.
    /// - the block does not already exist in a free list.
    ///
    /// The first page of every free block stores a [`FreeBlockNode`] so the
    /// allocator can maintain the linked list without additional memory.
    pub fn insert_block(&mut self, block_pfn: PageFrameNum, order: usize) {
        assert!(order <= MAX_ORDER);

        debug!(
            "Inserting block PFN {:?} into free list order {}",
            block_pfn, order
        );

        /*
         * Store the order before linking the block.
         *
         * Future merge operations rely on this metadata to determine
         * whether two buddies can be combined.
         */
        self.backend.set_order(block_pfn, order);

        let node_ptr = self.backend.pfn_to_ptr(block_pfn) as *mut FreeBlockNode;

        unsafe {
            /*
             * Free lists are intrusive doubly-linked lists.
             *
             * The block itself contains the pointers, avoiding external
             * allocation for allocator metadata.
             */
            (*node_ptr).next = self.free_lists[order];
            (*node_ptr).prev = None;

            /*
             * If another block already exists, make it point back to the
             * newly inserted head.
             */
            if let Some(mut next_node) = self.free_lists[order] {
                (next_node.as_mut()).prev = Some(NonNull::new_unchecked(node_ptr));
            }

            self.free_lists[order] = Some(NonNull::new_unchecked(node_ptr));
        }

        self.backend.mark_free(block_pfn);

        trace!(
            "Block PFN {:?} inserted successfully at order {}",
            block_pfn, order
        );
    }

    /// Removes the first block from a free list.
    ///
    /// Returns the PFN of the removed block.
    ///
    /// Removing a block only updates the linked-list state. The actual
    /// memory remains untouched because physical memory management is
    /// handled by the backend.
    pub fn remove_block(&mut self, order: usize) -> Option<PageFrameNum> {
        assert!(order <= MAX_ORDER);

        debug!("Removing block from free list order {}", order);

        if let Some(mut node_ptr) = self.free_lists[order] {
            unsafe {
                /*
                 * Advance the list head to the next node.
                 */
                let next_node = (node_ptr.as_mut()).next;

                if let Some(mut next_node_ptr) = next_node {
                    (next_node_ptr.as_mut()).prev = None;
                }

                self.free_lists[order] = next_node;

                let block_pfn = self.backend.ptr_to_pfn(node_ptr.as_ptr() as *mut u8);

                /*
                 * A removed block is no longer available for allocation.
                 */
                self.backend.mark_allocated(block_pfn);

                info!(
                    "Removed block PFN {:?} from order {} free list",
                    block_pfn, order
                );

                return Some(block_pfn);
            }
        }

        trace!("No block available in order {}", order);
        None
    }

    /// Removes a specific block from its free list.
    ///
    /// Unlike [`RawBuddyAllocator::remove_block`], which removes the current list head, this
    /// function searches for a known block directly through its PFN.
    ///
    /// This operation is mainly used during buddy merging, where the
    /// allocator already knows exactly which buddy must be removed.
    ///
    /// Returns:
    ///
    /// - `true` if the block was removed successfully.
    /// - `false` if the block was not free.
    pub fn remove_block_pfn(&mut self, block_pfn: PageFrameNum) -> bool {
        debug!("Removing specific block PFN {:?}", block_pfn);

        /*
         * Only free blocks can exist in allocator lists.
         *
         * Attempting to remove an allocated block indicates corrupted
         * allocator state or an invalid caller.
         */
        if self.backend.is_free(block_pfn) {
            let order = self.backend.get_order(block_pfn);

            let node_ptr = self.backend.pfn_to_ptr(block_pfn) as *mut FreeBlockNode;

            unsafe {
                /*
                 * Detach this node from the doubly-linked list.
                 *
                 * Three cases are handled:
                 *
                 * 1. Node has a previous node:
                 *      previous -> next = current.next
                 *
                 * 2. Node is the list head:
                 *      list head = current.next
                 *
                 * 3. Node has a next node:
                 *      next.previous = current.previous
                 */
                let prev_node = (*node_ptr).prev;
                let next_node = (*node_ptr).next;

                if let Some(mut prev_node_ptr) = prev_node {
                    (prev_node_ptr.as_mut()).next = next_node;
                } else {
                    self.free_lists[order] = next_node;
                }

                if let Some(mut next_node_ptr) = next_node {
                    (next_node_ptr.as_mut()).prev = prev_node;
                }
            }

            /*
             * Once removed, the block is owned by the allocator logic and
             * can no longer be considered available memory.
             */
            self.backend.mark_allocated(block_pfn);

            info!("Removed PFN {:?} from free list order {}", block_pfn, order);

            return true;
        }

        warn!("Attempted to remove allocated PFN {:?}", block_pfn);
        false
    }

    /// Splits a buddy block into two smaller blocks.
    ///
    /// An order-N block is divided into two order-(N-1) blocks:
    ///
    /// ```text
    /// Before:
    ///
    /// +-----------------------+
    /// |       order N          |
    /// +-----------------------+
    ///
    /// After:
    ///
    /// +-----------+ +-----------+
    /// | order N-1 |-| order N-1 |
    /// +-----------+ +-----------+
    /// ```
    ///
    /// The returned blocks are not inserted into free lists. The caller
    /// decides which half continues allocation and which half is returned
    /// to the allocator.
    pub fn split_block(
        &mut self,
        block_pfn: PageFrameNum,
        order: usize,
    ) -> (PageFrameNum, PageFrameNum) {
        assert!(order > 0 && order <= MAX_ORDER);

        /*
         * Buddy addresses are calculated by toggling the bit that
         * represents the current block size.
         *
         * Example:
         *
         * order 2 block size = 4 pages
         *
         * PFN 0b0100
         * XOR 0b0100
         *      ----
         *      0b0000
         *
         * gives the adjacent buddy.
         */
        let buddy_pfn = buddy::buddy_of(block_pfn, order - 1);

        info!(
            "Splitting PFN {:?} order {} into {:?} and {:?} order {}",
            block_pfn,
            order,
            block_pfn,
            buddy_pfn,
            order - 1
        );

        /*
         * Both resulting blocks become valid blocks of the lower order.
         */
        self.backend.set_order(block_pfn, order - 1);
        self.backend.set_order(buddy_pfn, order - 1);

        (block_pfn, buddy_pfn)
    }

    /// Attempts to merge a block with its buddy.
    ///
    /// Two blocks may merge only when:
    ///
    /// - the buddy exists,
    /// - the buddy is free,
    /// - both blocks have the same order.
    ///
    /// The resulting block starts at the lower PFN because buddy blocks
    /// are always adjacent and aligned.
    ///
    /// Returns the PFN of the merged block if successful.
    pub fn merge_buddy(&mut self, block_pfn: PageFrameNum, order: usize) -> Option<PageFrameNum> {
        if order >= MAX_ORDER {
            trace!(
                "Cannot merge PFN {:?} at order {}: already at MAX_ORDER",
                block_pfn, order
            );

            return None;
        }

        let buddy_pfn = buddy::buddy_of(block_pfn, order);

        trace!(
            "Checking buddy {:?} for PFN {:?} order {}",
            buddy_pfn, block_pfn, order
        );

        /*
         * A merge is only possible when both halves are free blocks of
         * identical size.
         */
        if self.backend.is_free(buddy_pfn)
            && self.backend.get_order(buddy_pfn) == order
            && self.backend.get_order(block_pfn) == order
        {
            info!(
                "Merging PFN {:?} with buddy {:?} at order {}",
                block_pfn, buddy_pfn, order
            );

            /*
             * The buddy will no longer exist independently after merging,
             * so remove it from the free list.
             */
            self.remove_block_pfn(buddy_pfn);

            /*
             * The merged block must begin at the smaller PFN.
             *
             * This restores the alignment requirements needed for future
             * buddy calculations.
             */
            let merged = if block_pfn.0 < buddy_pfn.0 {
                block_pfn
            } else {
                buddy_pfn
            };

            self.backend.set_order(merged, order + 1);

            debug!(
                "Merged block starts at PFN {:?}, new order {}",
                merged,
                order + 1
            );

            Some(merged)
        } else {
            trace!(
                "Cannot merge PFN {:?}, buddy {:?} unavailable or wrong order",
                block_pfn, buddy_pfn
            );

            None
        }
    }

    /// Allocates a contiguous buddy block of the requested order.
    ///
    /// The allocator first searches for an exact match. If unavailable,
    /// larger blocks are used and repeatedly split.
    ///
    /// Example:
    ///
    /// Request:
    ///
    /// ```text
    /// order 2
    /// ```
    ///
    /// Available:
    ///
    /// ```text
    /// order 4
    /// ```
    ///
    /// Result:
    ///
    /// ```text
    /// order 4
    ///       |
    ///       v
    /// order 3 + order 3
    ///       |
    ///       v
    /// order 2 + order 2
    /// ```
    pub fn alloc(&mut self, order: usize) -> Result<PageFrameNum, AllocError> {
        info!("Allocating block of order {}", order);

        if order > MAX_ORDER {
            error!(
                "Allocation failed: requested order {} exceeds MAX_ORDER {}",
                order, MAX_ORDER
            );
            return Err(AllocError::InvalidOrder);
        }

        let mut current_order = order;

        /*
         * Find the smallest available block that can satisfy the request.
         */
        let mut block_pfn = loop {
            if current_order > MAX_ORDER {
                error!(
                    "Allocation failed: no block available up to MAX_ORDER {}",
                    MAX_ORDER
                );

                return Err(AllocError::OutOfMemory);
            }

            if let Some(pfn) = self.remove_block(current_order) {
                break pfn;
            }

            debug!(
                "No block at order {}, trying order {}",
                current_order,
                current_order + 1
            );

            current_order += 1;
        };

        /*
         * Reduce larger blocks until the requested size is reached.
         *
         * The first half continues toward allocation while the second
         * half returns to the free list.
         */
        while current_order > order {
            let (first_half, second_half) = self.split_block(block_pfn, current_order);

            block_pfn = first_half;

            self.insert_block(second_half, current_order - 1);

            current_order -= 1;
        }

        info!(
            "Allocation successful: PFN {:?}, order {}",
            block_pfn, order
        );

        Ok(block_pfn)
    }
    /// Frees a previously allocated buddy block.
    ///
    /// The allocator attempts to merge the returned block with its buddy
    /// repeatedly until:
    ///
    /// - the buddy is allocated,
    /// - the buddy has a different order,
    /// - or the maximum order is reached.
    ///
    /// The final merged block is inserted back into the appropriate free
    /// list.
    ///
    /// # Errors
    ///
    /// This function validates the request before mutating any allocator
    /// state and returns [`FreeError`] without side effects if:
    ///
    /// - `order` exceeds [`MAX_ORDER`] ([`FreeError::InvalidOrder`]),
    /// - the block is already free, i.e. a double free
    ///   ([`FreeError::AlreadyFree`]),
    pub fn free(&mut self, block_pfn: PageFrameNum, order: usize) -> Result<(), FreeError> {
        info!("Freeing block PFN {:?}, order {}", block_pfn, order);

        if order > MAX_ORDER {
            error!(
                "Free failed: requested order {} exceeds MAX_ORDER {}",
                order, MAX_ORDER
            );
            return Err(FreeError::InvalidOrder);
        }

        /*
         * Reject a double free: a block that is still free is already
         * tracked by the allocator and must not be freed again.
         */
        if self.backend.is_free(block_pfn) {
            warn!("Double free attempted on PFN {:?}", block_pfn);
            return Err(FreeError::AlreadyFree);
        }

        // If we reach here, this means that the block is currently allocated and can be freed.

        let mut current_order = order;
        let mut current_block_pfn = block_pfn;

        /*
         * Buddy merging works upward through the allocator hierarchy.
         *
         * Example:
         *
         *      order 0 + order 0
         *              |
         *              v
         *          order 1
         *
         *      order 1 + order 1
         *              |
         *              v
         *          order 2
         *
         * The process continues until the block can no longer grow.
         */
        while let Some(merged_block) = self.merge_buddy(current_block_pfn, current_order) {
            /*
             * The merged block may itself have a free buddy at the
             * next order, so continue attempting larger merges.
             */
            current_block_pfn = merged_block;
            current_order += 1;
        }

        /*
         * No more merging is possible.
         *
         * The resulting block becomes available for future allocations.
         */
        self.insert_block(current_block_pfn, current_order);

        info!(
            "Free complete: PFN {:?}, final order {}",
            current_block_pfn, current_order
        );

        Ok(())
    }
}

/// Calculates the buddy block of a given PFN and order.
///
/// Buddy allocation relies on power-of-two alignment. The buddy of a block
/// can therefore be found by flipping the bit corresponding to the block
/// size:
///
/// ```text
/// buddy = PFN XOR block_size
/// ```
///
/// Example:
///
/// ```text
/// order 2
/// block size = 4 pages
///
/// PFN:
/// 0100
///
/// block size:
/// 0100
///
/// XOR:
/// ----
/// 0000
///
/// buddy PFN = 0
/// ```
///
/// The returned PFN is always aligned to the same order as the input.
fn buddy_of(pfn: PageFrameNum, order: usize) -> PageFrameNum {
    /*
     * Each order represents a block containing 2^order pages.
     *
     * Flipping that bit moves between the two halves of the parent block.
     */
    let block_size = 1 << order;

    let buddy_pfn = pfn.0 ^ block_size;

    trace!(
        "Buddy calculation: PFN {:?}, order {} -> PFN {:?}",
        pfn,
        order,
        PageFrameNum(buddy_pfn)
    );

    PageFrameNum(buddy_pfn)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBackend {
        // Two usize values give us enough aligned storage for the intrusive
        // FreeBlockNode used by the allocator.
        memory: Vec<[usize; 64]>,

        free: Vec<bool>,
        orders: Vec<usize>,
    }

    impl TestBackend {
        fn new(page_count: usize) -> Self {
            Self {
                memory: vec![[0; 64]; page_count],
                free: vec![false; page_count],
                orders: vec![0; page_count],
            }
        }
    }

    impl BuddyBackend for TestBackend {
        fn ptr_to_pfn(&self, ptr: *mut u8) -> PageFrameNum {
            let base = self.memory.as_ptr() as *const u8;
            let page_size = core::mem::size_of::<[usize; 64]>();

            let offset = unsafe { ptr.offset_from(base) } as usize;

            PageFrameNum::new(offset / page_size)
        }

        fn pfn_to_ptr(&self, pfn: PageFrameNum) -> *mut u8 {
            self.memory[pfn.number()].as_ptr() as *mut u8
        }

        fn is_free(&self, pfn: PageFrameNum) -> bool {
            self.free[pfn.number()]
        }

        fn mark_free(&mut self, pfn: PageFrameNum) {
            self.free[pfn.number()] = true;
        }

        fn mark_allocated(&mut self, pfn: PageFrameNum) {
            self.free[pfn.number()] = false;
        }

        fn get_order(&self, pfn: PageFrameNum) -> usize {
            self.orders[pfn.number()]
        }

        fn set_order(&mut self, pfn: PageFrameNum, order: usize) {
            self.orders[pfn.number()] = order;
        }
    }

    fn pfn(number: usize) -> PageFrameNum {
        PageFrameNum::new(number)
    }

    fn allocator(page_count: usize) -> RawBuddyAllocator<TestBackend> {
        RawBuddyAllocator::new(TestBackend::new(page_count))
    }

    #[test]
    fn test_buddy_of() {
        let pfn = PageFrameNum(0b0100);
        let order = 2;

        let buddy = buddy_of(pfn, order);

        assert_eq!(buddy, PageFrameNum(0b0000));
    }

    #[test]
    fn test_buddy_of_multiple_orders() {
        assert_eq!(buddy_of(pfn(0), 0), pfn(1));
        assert_eq!(buddy_of(pfn(1), 0), pfn(0));

        assert_eq!(buddy_of(pfn(0), 1), pfn(2));
        assert_eq!(buddy_of(pfn(2), 1), pfn(0));

        assert_eq!(buddy_of(pfn(4), 2), pfn(0));
        assert_eq!(buddy_of(pfn(0), 2), pfn(4));

        assert_eq!(buddy_of(pfn(8), 3), pfn(0));
        assert_eq!(buddy_of(pfn(0), 3), pfn(8));
    }

    #[test]
    fn test_insert_and_remove_block() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 2);

        assert!(allocator.backend.is_free(pfn(0)));
        assert_eq!(allocator.backend.get_order(pfn(0)), 2);

        let removed = allocator.remove_block(2);

        assert_eq!(removed, Some(pfn(0)));
        assert!(!allocator.backend.is_free(pfn(0)));
        assert!(allocator.remove_block(2).is_none());
    }

    #[test]
    fn test_insert_multiple_blocks_same_order() {
        let mut allocator = allocator(32);

        allocator.insert_block(pfn(0), 2);
        allocator.insert_block(pfn(4), 2);
        allocator.insert_block(pfn(8), 2);

        assert_eq!(allocator.remove_block(2), Some(pfn(8)));
        assert_eq!(allocator.remove_block(2), Some(pfn(4)));
        assert_eq!(allocator.remove_block(2), Some(pfn(0)));
        assert!(allocator.remove_block(2).is_none());
    }

    #[test]
    fn test_remove_block_pfn_head() {
        let mut allocator = allocator(32);

        allocator.insert_block(pfn(0), 2);
        allocator.insert_block(pfn(4), 2);
        allocator.insert_block(pfn(8), 2);

        assert!(allocator.remove_block_pfn(pfn(8)));

        assert!(!allocator.backend.is_free(pfn(8)));

        assert_eq!(allocator.remove_block(2), Some(pfn(4)));
        assert_eq!(allocator.remove_block(2), Some(pfn(0)));
    }

    #[test]
    fn test_remove_block_pfn_middle() {
        let mut allocator = allocator(32);

        allocator.insert_block(pfn(0), 2);
        allocator.insert_block(pfn(4), 2);
        allocator.insert_block(pfn(8), 2);

        assert!(allocator.remove_block_pfn(pfn(4)));

        assert!(!allocator.backend.is_free(pfn(4)));

        assert_eq!(allocator.remove_block(2), Some(pfn(8)));
        assert_eq!(allocator.remove_block(2), Some(pfn(0)));
    }

    #[test]
    fn test_remove_block_pfn_tail() {
        let mut allocator = allocator(32);

        allocator.insert_block(pfn(0), 2);
        allocator.insert_block(pfn(4), 2);
        allocator.insert_block(pfn(8), 2);

        assert!(allocator.remove_block_pfn(pfn(0)));

        assert_eq!(allocator.remove_block(2), Some(pfn(8)));
        assert_eq!(allocator.remove_block(2), Some(pfn(4)));
        assert!(allocator.remove_block(2).is_none());
    }

    #[test]
    fn test_remove_block_pfn_allocated_block_fails() {
        let mut allocator = allocator(16);

        assert!(!allocator.remove_block_pfn(pfn(0)));
    }

    #[test]
    fn test_split_block() {
        let mut allocator = allocator(16);

        let (first, second) = allocator.split_block(pfn(0), 2);

        assert_eq!(first, pfn(0));
        assert_eq!(second, pfn(2));

        assert_eq!(allocator.backend.get_order(pfn(0)), 1);
        assert_eq!(allocator.backend.get_order(pfn(2)), 1);
    }

    #[test]
    fn test_alloc_exact_order() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 2);

        let allocated = allocator.alloc(2);

        assert_eq!(allocated, Ok(pfn(0)));
        assert!(!allocator.backend.is_free(pfn(0)));
    }

    #[test]
    fn test_alloc_splits_larger_block() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 3);

        let allocated = allocator.alloc(1);

        assert_eq!(allocated, Ok(pfn(0)));

        // Original order 3 block:
        //
        // order 3: [0 ........ 7]
        // order 2: [0..3] [4..7]
        // order 1: [0..1] [2..3]
        //
        // The allocator should allocate [0..1] and leave
        // [2..3] and [4..7] free.

        assert!(!allocator.backend.is_free(pfn(0)));

        assert!(allocator.backend.is_free(pfn(2)));
        assert_eq!(allocator.backend.get_order(pfn(2)), 1);

        assert!(allocator.backend.is_free(pfn(4)));
        assert_eq!(allocator.backend.get_order(pfn(4)), 2);
    }

    #[test]
    fn test_alloc_multiple_blocks() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 3);

        let first = allocator.alloc(1);
        let second = allocator.alloc(1);
        let third = allocator.alloc(1);

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert!(third.is_ok());

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
    }

    #[test]
    fn test_alloc_fails_when_memory_exhausted() {
        let mut allocator = allocator(8);

        allocator.insert_block(pfn(0), 3);

        assert!(allocator.alloc(3).is_ok());
        assert!(allocator.alloc(0).is_err());
    }

    #[test]
    fn test_free_without_merging() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 2);

        let allocated = allocator.alloc(2).unwrap();

        allocator.free(allocated, 2).unwrap();

        assert!(allocator.backend.is_free(pfn(0)));
        assert_eq!(allocator.backend.get_order(pfn(0)), 2);

        assert_eq!(allocator.remove_block(2), Some(pfn(0)));
    }

    #[test]
    fn test_free_merges_two_buddies() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 2);

        let first = allocator.alloc(1).unwrap();
        let second = allocator.alloc(1).unwrap();

        assert_ne!(first, second);

        allocator.free(first, 1).unwrap();
        allocator.free(second, 1).unwrap();

        // The two order-1 blocks should have merged back into order 2.
        assert_eq!(allocator.remove_block(2), Some(pfn(0)));
        assert!(allocator.remove_block(1).is_none());
    }

    #[test]
    fn test_free_does_not_merge_with_different_order() {
        let mut allocator = allocator(32);

        allocator.insert_block(pfn(0), 3);

        let allocated = allocator.alloc(0).unwrap();

        // The buddy at PFN 1 is not necessarily an order-0 free block
        // in this situation. The allocator must respect the stored order.
        let sec = allocator.alloc(1).unwrap();
        allocator.free(allocated, 1).unwrap();

        assert_ne!(allocator.remove_block(3), Some(pfn(0)));
        allocator.free(sec, 1).unwrap();
        assert_eq!(allocator.remove_block(3), Some(pfn(0)));
    }

    #[test]
    fn test_free_then_reallocate() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 2);

        let first = allocator.alloc(2).unwrap();

        allocator.free(first, 2).unwrap();

        let second = allocator.alloc(2).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn test_full_coalescing_after_multiple_allocations() {
        let mut allocator = allocator(16);

        allocator.insert_block(pfn(0), 3);

        let a = allocator.alloc(0).unwrap();
        let b = allocator.alloc(0).unwrap();
        let c = allocator.alloc(0).unwrap();
        let d = allocator.alloc(0).unwrap();

        allocator.free(a, 0).unwrap();
        allocator.free(b, 0).unwrap();
        allocator.free(c, 0).unwrap();
        allocator.free(d, 0).unwrap();

        // All four pages should eventually form an order-3 block.
        assert_eq!(allocator.remove_block(3), Some(pfn(0)));
    }
}
