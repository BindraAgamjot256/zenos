use super::{Allocator, RefCounted};
use core::{
    alloc::Layout,
    marker::PhantomData,
    marker::Unsize,
    ops::{CoerceUnsized, Deref},
    ptr::NonNull,
    sync::atomic::{Ordering, fence},
};

pub struct KArc<T: RefCounted + ?Sized, A: Allocator> {
    pub(super) data: NonNull<T>,
    pub(super) allocator: PhantomData<A>,
    pub(super) _marker: PhantomData<T>,
}

impl<T: RefCounted + ?Sized, A: Allocator> KArc<T, A> {
    pub fn as_ref(&self) -> &T {
        // safe cuz KArc is guaranteed to live as long as T.
        unsafe { self.data.as_ref() }
    }

    pub fn clone_inner(&self) -> Self {
        self.as_ref().inc_refcount(Ordering::Relaxed);

        Self {
            data: self.data,
            allocator: self.allocator,
            _marker: self._marker,
        }
    }
}

impl<T: RefCounted + ?Sized, A: Allocator> Clone for KArc<T, A> {
    #[inline]
    fn clone(&self) -> Self {
        self.clone_inner()
    }
}

impl<T: RefCounted + ?Sized, A: Allocator> Deref for KArc<T, A> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T: RefCounted + ?Sized, A: Allocator> AsRef<T> for KArc<T, A> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.as_ref()
    }
}

impl<T: RefCounted + ?Sized, A: Allocator> Drop for KArc<T, A> {
    #[inline]
    fn drop(&mut self) {
        /*
         * dec_refcount() must return the NEW reference count.
         *
         * If it returns zero, this KArc released the final reference and
         * therefore owns destruction of the object.
         */
        let remaining = unsafe { self.data.as_ref() }.dec_refcount(Ordering::Release);
        if remaining != 0 {
            return;
        }
        /*
         * Acquire synchronization pairs with the Release operations from
         * other references before the final reference is destroyed.
         */
        fence(Ordering::Acquire);

        /*
         * Calculate the layout before dropping the value.
         *
         * This is important for dynamically-sized types. `Layout::for_value`
         * uses the object's metadata, so it works for slices and trait
         * objects as well.
         */
        let layout = unsafe { Layout::for_value(self.data.as_ref()) };
        unsafe {
            /*
             * First destroy the object.
             */
            core::ptr::drop_in_place(self.data.as_ptr());

            /*
             * Then return the allocation to the allocator that originally
             * allocated it.
             */
            A::deallocate(super::Allocation::from_ptr(self.data.cast()), layout);
        }
    }
}

impl<T, U, A> CoerceUnsized<KArc<U, A>> for KArc<T, A>
where
    T: RefCounted + ?Sized + Unsize<U>,
    U: RefCounted + ?Sized,
    A: Allocator,
{
}

unsafe impl<T: RefCounted + ?Sized + Send, A: Allocator> Send for KArc<T, A> {}
unsafe impl<T: RefCounted + ?Sized + Sync, A: Allocator> Sync for KArc<T, A> {}

impl<T: RefCounted + core::fmt::Debug + ?Sized, A: Allocator> core::fmt::Debug for KArc<T, A> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let data = unsafe { self.data.as_ref() };
        f.debug_struct("KArc")
            .field("data", &data)
            .field("refcount", &(data.ref_count()))
            .finish()
    }
}
