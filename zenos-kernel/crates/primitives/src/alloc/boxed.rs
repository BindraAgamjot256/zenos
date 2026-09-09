use crate::alloc::{Allocation, Allocator, CreatableKernelObject};
use core::{
    alloc::Layout,
    marker::{PhantomData, Unsize},
    ops::{CoerceUnsized, Deref, DerefMut},
    ptr::NonNull,
};

use crate::alloc::{AllocationError, KernelObject};

#[must_use = "KBox Allocates heap memory. DO NOT WASTE THE FUCKING MEMORY."]
pub struct KBox<T: KernelObject + ?Sized, A: Allocator> {
    data: NonNull<T>,
    allocator: PhantomData<A>,
    _marker: PhantomData<T>,
}

impl<T: CreatableKernelObject + Sized> KBox<T, T::Allocator> {
    #[inline]
    pub fn new(data: T) -> Result<Self, AllocationError> {
        let allocation = T::Allocator::allocate(T::layout())?;
        let raw = allocation.as_ptr().cast::<T>();
        unsafe { raw.write(data) };
        Ok(Self {
            data: raw,
            _marker: PhantomData,
            allocator: PhantomData,
        })
    }
}

impl<T: KernelObject + ?Sized, A: Allocator> KBox<T, A> {
    pub fn raw_ptr(&self) -> *mut T {
        self.data.as_ptr()
    }
}

impl<T: KernelObject + core::fmt::Debug + ?Sized, A: Allocator> core::fmt::Debug for KBox<T, A> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let data = unsafe { self.data.as_ref() };
        f.debug_tuple("KBox").field(&data).finish()
    }
}

impl<T: KernelObject + ?Sized, A: Allocator> Deref for KBox<T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<T: KernelObject + ?Sized, A: Allocator> DerefMut for KBox<T, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.as_mut() }
    }
}

impl<T: KernelObject + ?Sized, A: Allocator> Drop for KBox<T, A> {
    #[inline]
    fn drop(&mut self) {
        unsafe { core::ptr::drop_in_place(self.data.as_ptr()) };
        A::deallocate(Allocation::from_ptr(self.data.cast()), unsafe {
            Layout::for_value_raw(self.data.as_ptr())
        })
    }
}

impl<T, U, A> CoerceUnsized<KBox<U, A>> for KBox<T, A>
where
    T: KernelObject + ?Sized + Unsize<U>,
    U: KernelObject + ?Sized,
    A: Allocator,
{
}

unsafe impl<T: KernelObject + ?Sized + Send, A: Allocator> Send for KBox<T, A> {}
unsafe impl<T: KernelObject + ?Sized + Sync, A: Allocator> Sync for KBox<T, A> {}

impl<T: KernelObject + ?Sized, A: Allocator> Clone for KBox<T, A> {
    fn clone(&self) -> Self {
        let data = self.data;

        let layout = unsafe { core::alloc::Layout::for_value_raw(data.as_ptr()) };

        let allocation = A::alloc_zeroed(layout).expect("KBox::clone: allocation failed");
        let new_data = allocation.as_ptr().cast::<u8>().as_ptr();

        let size = layout.size();
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr().cast::<u8>(), new_data, size);
        }

        let metadata = core::ptr::metadata(data.as_ptr());
        let new = core::ptr::from_raw_parts_mut::<T>(new_data, metadata);

        Self {
            data: NonNull::new(new).unwrap(),
            _marker: PhantomData,
            allocator: PhantomData,
        }
    }
}
