use core::mem::{align_of, size_of};

use super::FreeSlot;
use super::PAGE_SIZE;
use super::SlabHeader;

/// Rounds `value` upward to an `align`-byte boundary.
///
/// Values already on the requested boundary are returned unchanged. The
/// bit-mask calculation requires `align` to be a nonzero power of two.
///
/// # Panics
///
/// Panics if `align` is zero or is not a power of two.
///
/// # Overflow
///
/// If `value + align - 1` exceeds [`usize::MAX`], debug builds panic and
/// release builds wrap according to Rust's integer-overflow behavior.
pub const fn align_up(value: usize, align: usize) -> usize {
    assert!(align.is_power_of_two());

    // Adding the alignment mask moves every non-aligned value into the next
    // boundary's range; clearing the low bits then selects that boundary.
    (value + align - 1) & !(align - 1)
}

/// Rounds `value` downward to an `align`-byte boundary.
///
/// Values already on the requested boundary are returned unchanged. The
/// bit-mask calculation requires `align` to be a nonzero power of two.
///
/// # Panics
///
/// Panics if `align` is zero or is not a power of two.
pub const fn align_down(value: usize, align: usize) -> usize {
    assert!(align.is_power_of_two());

    // Power-of-two alignments encode their remainder in the low address bits.
    value & !(align - 1)
}

/// Calculates the padded header size for `size`-byte objects.
///
/// Rounding the physical [`SlabHeader`] size up to an object-size boundary
/// ensures that the first object slot begins immediately after the padding and
/// that every subsequent slot has the same alignment.
///
/// # Panics
///
/// Panics when `size` is not a nonzero power of two.
pub const fn header_size(size: usize) -> usize {
    // Padding prevents the header from shifting every object off its required
    // size-based alignment.
    align_up(size_of::<SlabHeader<0>>(), size)
}

/// Calculates how many `size`-byte objects fit in one slab page.
///
/// The padded header is subtracted from [`PAGE_SIZE`], and the remaining space
/// is divided into equal object slots. Any trailing bytes that cannot hold a
/// complete object are left unused.
///
/// # Panics
///
/// Panics if `size` is zero, is not a power of two, or does not preserve the
/// alignment required by [`FreeSlot`]. This function also panics on subtraction
/// overflow if the padded header is larger than a page.
pub const fn get_num_objects(size: usize) -> usize {
    assert!(size > 0);
    assert!(size.is_power_of_two());
    assert!(size.is_multiple_of(align_of::<FreeSlot>()));

    // Integer division intentionally discards any tail too small for a full
    // object slot.
    let start = header_size(size);
    (PAGE_SIZE - start) / size
}

/// Validates an object size for use by the slab allocator.
///
/// A supported size must:
///
/// - be large enough to store a [`FreeSlot`],
/// - satisfy the link's alignment,
/// - be a power of two,
/// - leave room for at least one object after the padded header.
///
/// # Panics
///
/// Panics if any slab-layout requirement is violated.
pub const fn validate_size(size: usize) {
    // A free slot stores allocator metadata in-band, so every object must be
    // able to contain and align that metadata.
    assert!(size >= size_of::<FreeSlot>());
    assert!(size.is_multiple_of(align_of::<FreeSlot>()));
    assert!(size.is_power_of_two());

    // Reject caches whose padded header leaves no usable object storage.
    assert!(header_size(size) + size <= PAGE_SIZE);
}

/// Backend metadata used to identify the cache that owns a slab page.
///
/// The metadata is deliberately stored outside the slab page. This lets
/// [`super::SlabCache::deallocate`] verify an aligned page address before it
/// dereferences a potentially untrusted object pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    /// Object size recorded when the slab page was created.
    pub size: usize,

    /// Per-cache identifier used to reject pages owned by another cache.
    pub owner: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_helpers_round_to_power_of_two_boundaries() {
        assert_eq!(align_up(0x1000, 0x1000), 0x1000);
        assert_eq!(align_up(0x1001, 0x1000), 0x2000);
        assert_eq!(align_down(0x1fff, 0x1000), 0x1000);
        assert_eq!(align_down(0x2000, 0x1000), 0x2000);
    }

    #[test]
    fn object_count_respects_padded_header() {
        for size in [8, 16, 64, 512, 2048] {
            validate_size(size);

            let header = header_size(size);
            let count = get_num_objects(size);

            assert_eq!(header % size, 0);
            assert!(header + count * size <= PAGE_SIZE);
            assert!(header + (count + 1) * size > PAGE_SIZE);
        }
    }

    #[test]
    #[should_panic]
    fn validate_size_rejects_non_power_of_two_sizes() {
        validate_size(24);
    }

    #[test]
    #[should_panic]
    fn validate_size_rejects_objects_that_do_not_fit() {
        validate_size(PAGE_SIZE);
    }
}
