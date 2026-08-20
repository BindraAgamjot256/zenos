use core::{alloc::Layout, cmp::min, ptr::NonNull};

pub mod boxed;

/// A trait implemented by objects that can be allocated as kernel objects.
///
/// `KernelObject` provides a convenient way to obtain the [`Layout`] required
/// to store an instance of the implementing type.
///
/// The default implementation uses [`Layout::new`], which accounts for the
/// size and alignment requirements of `Self`.
pub trait KernelObject {
    /// Returns the memory layout required to store an instance of `Self`.
    ///
    /// By default, this is equivalent to:
    ///
    /// ```ignore
    /// Layout::new::<Self>()
    /// ```
    fn layout() -> Layout
    where
        Self: Sized,
    {
        Layout::new::<Self>()
    }
}

pub trait CreatableKernelObject: KernelObject + Sized {
    type Allocator: Allocator;
}

/// Errors that can occur while allocating memory.
pub enum AllocationError {
    /// The requested allocation size is invalid.
    InvalidSize,

    /// The requested [`Layout`] is invalid.
    ///
    /// This can occur when the size and alignment combination cannot be
    /// represented by a valid `Layout`.
    UnsupportedLayout,

    /// The allocator could not satisfy the allocation request because
    /// insufficient memory was available.
    OutOfMemory,
}

/// A block of memory returned by an [`Allocator`].
///
/// The allocation contains a non-null pointer to a block of memory. The
/// actual usable size of the allocation is determined by the [`Layout`]
/// supplied when the allocation was created.
///
/// An `Allocation` does not itself contain information about its layout.
/// Callers must therefore retain the [`Layout`] associated with an allocation
/// and provide it when the allocation is deallocated or reallocated.
pub struct Allocation(NonNull<u8>);

impl Allocation {
    pub fn from_ptr(ptr: NonNull<u8>) -> Self {
        Self(ptr)
    }

    pub fn as_ptr(&self) -> NonNull<u8> {
        self.0.cast()
    }
}

/// An interface for allocating and deallocating memory.
///
/// Implementors are responsible for providing memory that satisfies the
/// requested [`Layout`]. Allocations returned by [`Allocator::allocate`] must
/// remain valid until they are passed back to [`Allocator::deallocate`].
///
/// # Safety
///
/// The trait itself is safe to implement, but implementations must uphold
/// the memory-safety requirements implied by the allocation methods. In
/// particular:
///
/// - Returned allocations must be valid for the requested layout.
/// - Returned allocations must be non-null.
/// - An allocation must not overlap with another live allocation unless
///   explicitly permitted by the allocator's design.
/// - `deallocate` must only release memory belonging to the allocator.
/// - The layout supplied to `deallocate` must correspond to the layout used
///   to create the allocation.
pub unsafe trait Allocator {
    /// Allocates a block of memory described by `layout`.
    ///
    /// On success, returns an [`Allocation`] containing the allocated memory.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError::InvalidSize`] if the requested size is not
    /// supported by the allocator.
    ///
    /// Returns [`AllocationError::InvalidLayout`] if the requested layout
    /// cannot be handled by the allocator.
    ///
    /// Returns [`AllocationError::OutOfMemory`] if sufficient memory is not
    /// available.
    fn allocate(layout: Layout) -> Result<Allocation, AllocationError>;

    /// Releases a previously allocated block of memory.
    ///
    /// The `layout` must describe the same allocation layout that was used
    /// when `allocation` was created.
    ///
    /// After this method returns, `allocation` must no longer be used.
    fn deallocate(allocation: Allocation, layout: Layout);

    /// Allocates a zero-initialized block of memory.
    ///
    /// This method first allocates memory using [`Allocator::allocate`] and
    /// then initializes the requested number of bytes to zero.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by [`Allocator::allocate`].
    ///
    /// # Safety
    ///
    /// The allocator's implementation of [`Allocator::allocate`] must return
    /// an allocation containing at least `layout.size()` writable bytes.
    fn alloc_zeroed(layout: Layout) -> Result<Allocation, AllocationError> {
        let allocation = Self::allocate(layout)?;

        unsafe {
            allocation.0.cast::<u8>().write_bytes(0, layout.size());
        }

        Ok(allocation)
    }

    /// Allocates a new block with a different size and copies the contents
    /// of the existing allocation into it.
    ///
    /// The new allocation preserves the alignment of the original `layout`.
    /// The first `layout.size()` bytes of the old allocation are copied into
    /// the new allocation.
    ///
    /// The old allocation is deallocated after the copy completes.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError::UnsupportedLayout`] if `new_size` combined with
    /// the original alignment does not form a valid [`Layout`].
    ///
    /// Propagates any error returned by [`Allocator::allocate`].
    ///
    /// # Safety
    ///
    /// The allocator must return an allocation large enough to hold
    /// `new_size` bytes when the new allocation is created.
    ///
    /// The caller must ensure that `allocation` is a valid allocation
    /// corresponding to `layout` and that its first `layout.size()` bytes
    /// may be read.
    fn reallocate(
        allocation: Allocation,
        layout: Layout,
        new_size: usize,
    ) -> Result<Allocation, AllocationError> {
        let new_layout = Layout::from_size_align(new_size, layout.align())
            .map_err(|_| AllocationError::UnsupportedLayout)?;

        let new_allocation = Self::allocate(new_layout)?;

        unsafe {
            allocation
                .0
                .cast::<u8>()
                .copy_to(new_allocation.0.cast::<u8>(), min(layout.size(), new_size));
        }

        Self::deallocate(allocation, layout);

        Ok(new_allocation)
    }
}
