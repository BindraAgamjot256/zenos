mod support;

use kmm::buddy::{AllocError, FreeError, MAX_ORDER, PageFrameNum};
use support::buddy_allocator;

#[test]
fn mixed_order_workload_preserves_alignment_and_coalesces() {
    const ROOT_ORDER: usize = 6;
    let mut allocator = buddy_allocator(ROOT_ORDER);
    let requests = [0, 2, 1, 3, 0, 2, 1, 0];
    let mut allocations = Vec::new();

    for order in requests {
        let pfn = allocator.alloc(order).expect("mixed workload should fit");
        let start = pfn.number();
        let end = start + (1usize << order);

        assert_eq!(start % (1usize << order), 0);
        assert!(
            allocations
                .iter()
                .all(|(other, other_order): &(PageFrameNum, usize)| {
                    let other_start = other.number();
                    let other_end = other_start + (1usize << other_order);
                    end <= other_start || other_end <= start
                })
        );
        allocations.push((pfn, order));
    }

    allocations.sort_by_key(|(pfn, _)| (pfn.number() * 17) % (1 << ROOT_ORDER));
    for (pfn, order) in allocations {
        allocator.free(pfn, order).unwrap();
    }

    assert_eq!(allocator.alloc(ROOT_ORDER), Ok(PageFrameNum::new(0)));
    assert_eq!(allocator.alloc(0), Err(AllocError::OutOfMemory));
}

#[test]
fn fragmentation_blocks_larger_requests_until_buddies_are_freed() {
    const ROOT_ORDER: usize = 5;
    let mut allocator = buddy_allocator(ROOT_ORDER);
    let mut pages: Vec<_> = (0..(1 << ROOT_ORDER))
        .map(|_| allocator.alloc(0).unwrap())
        .collect();
    pages.sort_by_key(PageFrameNum::number);

    for pfn in pages.iter().step_by(2) {
        allocator.free(*pfn, 0).unwrap();
    }
    assert_eq!(allocator.alloc(1), Err(AllocError::OutOfMemory));

    for pfn in pages.iter().skip(1).step_by(2) {
        allocator.free(*pfn, 0).unwrap();
    }
    assert_eq!(allocator.alloc(ROOT_ORDER), Ok(PageFrameNum::new(0)));
}

#[test]
fn invalid_orders_and_double_frees_are_reported_without_losing_memory() {
    let mut allocator = buddy_allocator(2);

    assert_eq!(
        allocator.alloc(MAX_ORDER + 1),
        Err(AllocError::InvalidOrder)
    );
    let page = allocator.alloc(0).unwrap();
    assert_eq!(
        allocator.free(page, MAX_ORDER + 1),
        Err(FreeError::InvalidOrder)
    );
    assert_eq!(allocator.free(page, 0), Ok(()));
    assert_eq!(allocator.free(page, 0), Err(FreeError::AlreadyFree));

    assert_eq!(allocator.alloc(2), Ok(PageFrameNum::new(0)));
}
