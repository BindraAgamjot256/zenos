use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
};

use kprimitives::mutex::Mutex;

use crate::mm::{BUDDY_ALLOCATOR, buddy::Mapping};

pub struct SlabBackend;

impl kmm::slab::Backend for SlabBackend {
    unsafe fn allocate(&mut self) -> Option<NonNull<u8>> {
        let mut allocation = BUDDY_ALLOCATOR.alloc(0).ok()?;
        let slice = allocation.as_mut_slice();

        NonNull::new(slice.as_mut_ptr())
    }

    unsafe fn deallocate(&mut self, page: NonNull<u8>) {
        let mapping = Mapping::new(page.as_ptr() as usize, 0);

        BUDDY_ALLOCATOR
            .free(mapping)
            .expect("failed to free slab page");
    }

    fn set_metadata(&mut self, meta: kmm::slab::Metadata, page: NonNull<u8>) {
        let mut mapping = Mapping::new(page.as_ptr() as usize, 0);
        let pages = mapping.as_mut_slice_pages();

        pages[0].mark_slab(meta);
    }

    fn get_metadata(&self, page: NonNull<u8>) -> Option<kmm::slab::Metadata> {
        let mapping = Mapping::new(page.as_ptr() as usize, 0);
        let pages = mapping.as_slice_pages();
        let page = &pages[0];

        if page.is_slab() {
            Some(unsafe { page.meta.slab })
        } else {
            None
        }
    }
}

type Slab16 = kmm::slab::SlabCache<16, SlabBackend>;
type Slab32 = kmm::slab::SlabCache<32, SlabBackend>;
type Slab64 = kmm::slab::SlabCache<64, SlabBackend>;
type Slab128 = kmm::slab::SlabCache<128, SlabBackend>;

struct SlabAllocator {
    slab16: Mutex<Slab16>,
    slab32: Mutex<Slab32>,
    slab64: Mutex<Slab64>,
    slab128: Mutex<Slab128>,
}

impl SlabAllocator {
    pub const fn new() -> Self {
        Self {
            slab16: Mutex::new(Slab16::new(SlabBackend)),
            slab32: Mutex::new(Slab32::new(SlabBackend)),
            slab64: Mutex::new(Slab64::new(SlabBackend)),
            slab128: Mutex::new(Slab128::new(SlabBackend)),
        }
    }

    #[inline]
    fn class(layout: Layout) -> Option<SlabClass> {
        let required = layout.size().max(layout.align());

        match required {
            0..=16 => Some(SlabClass::Size16),
            17..=32 => Some(SlabClass::Size32),
            33..=64 => Some(SlabClass::Size64),
            65..=128 => Some(SlabClass::Size128),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum SlabClass {
    Size16,
    Size32,
    Size64,
    Size128,
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

unsafe impl GlobalAlloc for SlabAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return layout.align() as *mut u8;
        }

        let Some(class) = Self::class(layout) else {
            return core::ptr::null_mut();
        };

        match class {
            SlabClass::Size16 => {
                let mut slab = self.slab16.lock();

                slab.allocate()
                    .map_or(core::ptr::null_mut(), NonNull::as_ptr)
            }

            SlabClass::Size32 => {
                let mut slab = self.slab32.lock();

                slab.allocate()
                    .map_or(core::ptr::null_mut(), NonNull::as_ptr)
            }

            SlabClass::Size64 => {
                let mut slab = self.slab64.lock();

                slab.allocate()
                    .map_or(core::ptr::null_mut(), NonNull::as_ptr)
            }

            SlabClass::Size128 => {
                let mut slab = self.slab128.lock();

                slab.allocate()
                    .map_or(core::ptr::null_mut(), NonNull::as_ptr)
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }

        let Some(class) = Self::class(layout) else {
            return;
        };

        unsafe {
            match class {
                SlabClass::Size16 => {
                    let mut slab = self.slab16.lock();

                    slab.deallocate(NonNull::new_unchecked(ptr));
                }

                SlabClass::Size32 => {
                    let mut slab = self.slab32.lock();

                    slab.deallocate(NonNull::new_unchecked(ptr));
                }

                SlabClass::Size64 => {
                    let mut slab = self.slab64.lock();

                    slab.deallocate(NonNull::new_unchecked(ptr));
                }

                SlabClass::Size128 => {
                    let mut slab = self.slab128.lock();

                    slab.deallocate(NonNull::new_unchecked(ptr));
                }
            }
        }
    }
}

#[global_allocator]
static ALLOCATOR: SlabAllocator = SlabAllocator::new();
