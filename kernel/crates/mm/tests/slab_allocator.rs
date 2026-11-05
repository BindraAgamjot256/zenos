mod support;

use kmm::slab::SlabCache;
use std::collections::HashSet;
use support::{HostSlabBackend, HostSlabStats};

#[test]
fn multi_page_workload_preserves_data_and_reuses_capacity() {
    const OBJECT_SIZE: usize = 64;
    const OBJECTS: usize = 256;

    let stats = HostSlabStats::default();
    let backend = HostSlabBackend::with_stats(8, stats.clone());
    let mut cache = SlabCache::<OBJECT_SIZE, _>::new(backend);
    let mut objects = Vec::with_capacity(OBJECTS);

    for index in 0..OBJECTS {
        let object = cache.allocate().expect("slab workload should fit");
        assert_eq!(object.addr().get() % OBJECT_SIZE, 0);
        unsafe { object.as_ptr().write_bytes(index as u8, OBJECT_SIZE) };
        objects.push((object, index as u8));
    }

    assert!(stats.page_allocations() > 1);
    assert_eq!(
        objects
            .iter()
            .map(|(ptr, _)| ptr.addr().get())
            .collect::<HashSet<_>>()
            .len(),
        OBJECTS
    );
    for (object, pattern) in &objects {
        for offset in 0..OBJECT_SIZE {
            assert_eq!(unsafe { object.as_ptr().add(offset).read() }, *pattern);
        }
    }

    objects.sort_by_key(|(ptr, _)| (ptr.addr().get() / OBJECT_SIZE) * 37);
    for (object, _) in objects.drain(..) {
        assert_eq!(cache.deallocate(object), Some(()));
    }

    assert_eq!(stats.live_pages(), 2);
    assert!(stats.page_deallocations() > 0);

    let reused: Vec<_> = (0..OBJECTS)
        .map(|_| {
            cache
                .allocate()
                .expect("released capacity should be reusable")
        })
        .collect();
    assert_eq!(
        reused
            .iter()
            .map(|ptr| ptr.addr().get())
            .collect::<HashSet<_>>()
            .len(),
        OBJECTS
    );
    for object in reused {
        assert_eq!(cache.deallocate(object), Some(()));
    }
}

#[test]
fn backend_exhaustion_recovers_after_objects_are_returned() {
    let stats = HostSlabStats::default();
    let backend = HostSlabBackend::with_stats(3, stats.clone());
    let mut cache = SlabCache::<2048, _>::new(backend);

    let objects: Vec<_> = (0..3).map(|_| cache.allocate().unwrap()).collect();
    assert!(cache.allocate().is_none());
    assert_eq!(stats.live_pages(), 3);
    assert_eq!(stats.allocation_attempts(), 4);

    for object in objects {
        assert_eq!(cache.deallocate(object), Some(()));
    }
    assert_eq!(stats.live_pages(), 2);

    let objects: Vec<_> = (0..3).map(|_| cache.allocate().unwrap()).collect();
    assert_eq!(stats.live_pages(), 3);
    for object in objects {
        assert_eq!(cache.deallocate(object), Some(()));
    }
}

#[test]
fn caches_reject_objects_owned_by_another_backend() {
    let mut first = SlabCache::<128, _>::new(HostSlabBackend::new(1));
    let mut second = SlabCache::<128, _>::new(HostSlabBackend::new(1));
    let object = first.allocate().unwrap();
    let obj_2 = second.allocate().unwrap();

    assert_eq!(second.deallocate(object), None);
    assert_eq!(first.deallocate(object), Some(()));
    assert_eq!(first.deallocate(obj_2), None);
    assert_eq!(second.deallocate(obj_2), Some(()));
}
