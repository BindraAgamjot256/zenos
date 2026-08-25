use crate::{
    arch::{PhysAddr, VirtAddr, mem::PAGE_SIZE},
    vmm::node::VmmNode,
};
use kprimitives::alloc::boxed::KBox;

mod node;

/// A virtual-memory arena that manages a range of virtual addresses.
///
/// Allocated regions are stored in a singly linked list sorted by virtual
/// address. New allocations use a first-fit strategy, placing them in the
/// first gap large enough to satisfy the request.
///
/// ```text
/// virt_start                         virt_end
///     │                                  │
///     ▼                                  ▼
///     ┌────────┬───────┬────────┬───────┐
///     │  free  │ node  │  free  │ node  │
///     └────────┴───────┴────────┴───────┘
/// ```
///
/// Each node tracks a virtual region and its corresponding physical address.
#[derive(Debug)]
pub struct VmmArena {
    /// Start of the virtual address range managed by the arena.
    virt_start: VirtAddr,

    /// End of the virtual address range managed by the arena.
    virt_end: VirtAddr,

    /// Head of the sorted list of allocated regions.
    head: Option<KBox<node::VmmNode, node::VmmNodeAllocator>>,
}

impl VmmArena {
    /// Creates an empty arena covering `[virt_start, virt_end)`.
    pub const fn new(virt_start: VirtAddr, virt_end: VirtAddr) -> Self {
        Self {
            virt_start,
            virt_end,
            head: None,
        }
    }

    /// Allocates a page-aligned virtual region backed by `phys`.
    ///
    /// The allocator searches for the first free gap large enough to fit the
    /// requested region, then inserts the new node into the sorted list.
    pub fn allocate_region(&mut self, phys: PhysAddr, size: usize) -> Option<VirtAddr> {
        log::debug!(
            "VMM: allocate request: phys={:#x}, size={:#x}, arena=[{:#x}, {:#x})",
            phys.as_usize(),
            size,
            self.virt_start.as_usize(),
            self.virt_end.as_usize(),
        );

        if size == 0 {
            log::warn!("VMM: rejected zero-sized allocation");
            return None;
        }

        let aligned_size = match size.checked_add(PAGE_SIZE - 1) {
            Some(size) => size & !(PAGE_SIZE - 1),
            None => {
                log::error!(
                    "VMM: allocation size overflow while aligning size={:#x}",
                    size
                );
                return None;
            }
        };

        log::trace!(
            "VMM: allocation size aligned: requested={:#x}, aligned={:#x}",
            size,
            aligned_size
        );

        let mut candidate = self.virt_start;
        let mut curr = &mut self.head;

        // Walk the allocation list until we find a gap large enough for the
        // requested region. `curr` is kept at the insertion point so the new
        // node can be inserted without another traversal.
        loop {
            if curr.is_none() {
                log::trace!(
                    "VMM: reached end of allocation list, candidate={:#x}",
                    candidate.as_usize()
                );
                break;
            }

            let node = curr.as_ref().unwrap();
            let node_start = node.virtaddr.as_usize();

            log::trace!(
                "VMM: checking allocation: virt={:#x}, phys={:#x}, size={:#x}",
                node.virtaddr.as_usize(),
                node.physaddr.as_usize(),
                node.size
            );

            if node_start >= candidate.as_usize() {
                let available_gap = match node_start.checked_sub(candidate.as_usize()) {
                    Some(gap) => gap,
                    None => {
                        log::error!(
                            "VMM: address underflow while calculating gap: \
                             node_start={:#x}, candidate={:#x}",
                            node_start,
                            candidate.as_usize()
                        );
                        return None;
                    }
                };

                log::trace!(
                    "VMM: gap before node: start={:#x}, size={:#x}",
                    candidate.as_usize(),
                    available_gap
                );

                if available_gap >= aligned_size {
                    log::debug!(
                        "VMM: found free gap: virt={:#x}, size={:#x}",
                        candidate.as_usize(),
                        aligned_size
                    );
                    break;
                }
            }

            candidate = match node_start.checked_add(node.size) {
                Some(end) => VirtAddr::new(end as u64),
                None => {
                    log::error!(
                        "VMM: allocation end overflow: virt={:#x}, size={:#x}",
                        node_start,
                        node.size
                    );
                    return None;
                }
            };

            log::trace!("VMM: advancing candidate to {:#x}", candidate.as_usize());

            // Re-bind `curr` to the next link. Keeping this inside the loop
            // avoids extending the immutable borrow of `node`.
            curr = &mut curr.as_mut().unwrap().next;
        }

        let candidate_end = match candidate.as_usize().checked_add(aligned_size) {
            Some(end) => end,
            None => {
                log::error!(
                    "VMM: candidate end overflow: candidate={:#x}, size={:#x}",
                    candidate.as_usize(),
                    aligned_size
                );
                return None;
            }
        };

        if candidate_end > self.virt_end.as_usize() {
            log::debug!(
                "VMM: allocation failed: out of virtual address space: \
                 candidate={:#x}, end={:#x}, arena_end={:#x}",
                candidate.as_usize(),
                candidate_end,
                self.virt_end.as_usize()
            );
            return None;
        }

        let new_node = match KBox::new(VmmNode {
            virtaddr: candidate,
            physaddr: phys,
            size: aligned_size,
            next: curr.take(),
        })
        .ok()
        {
            Some(node) => node,
            None => {
                log::error!(
                    "VMM: failed to allocate VmmNode: virt={:#x}, phys={:#x}, size={:#x}",
                    candidate.as_usize(),
                    phys.as_usize(),
                    aligned_size
                );
                return None;
            }
        };

        *curr = Some(new_node);

        log::debug!(
            "VMM: allocation successful: virt={:#x}, phys={:#x}, size={:#x}, end={:#x}",
            candidate.as_usize(),
            phys.as_usize(),
            aligned_size,
            candidate_end
        );

        Some(candidate)
    }

    /// Frees the allocation beginning at `target_vaddr`.
    ///
    /// Returns `None` if no allocation starts at the given address.
    pub fn free_region(&mut self, target_vaddr: VirtAddr) -> Option<()> {
        log::debug!("VMM: free request: virt={:#x}", target_vaddr.as_usize());

        let mut curr = &mut self.head;

        loop {
            let should_remove = match curr.as_ref() {
                Some(node) => {
                    log::trace!(
                        "VMM: checking allocation for free: virt={:#x}, phys={:#x}, size={:#x}",
                        node.virtaddr.as_usize(),
                        node.physaddr.as_usize(),
                        node.size
                    );

                    node.virtaddr == target_vaddr
                }
                None => {
                    log::debug!(
                        "VMM: free failed: no allocation at virt={:#x}",
                        target_vaddr.as_usize()
                    );
                    return None;
                }
            };

            if should_remove {
                let mut node = curr.take().unwrap();

                log::debug!(
                    "VMM: freeing allocation: virt={:#x}, phys={:#x}, size={:#x}",
                    node.virtaddr.as_usize(),
                    node.physaddr.as_usize(),
                    node.size
                );

                *curr = node.next.take();

                log::debug!("VMM: free successful: virt={:#x}", target_vaddr.as_usize());

                return Some(());
            }

            curr = &mut curr.as_mut().unwrap().next;
        }
    }
}
