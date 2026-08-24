use core::marker::PhantomData;

//
// Page Sizes
//

pub trait PageSize {
    const SIZE: usize;

    fn is_aligned(addr: usize) -> bool {
        addr.is_multiple_of(Self::SIZE)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Size4K;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Size2M;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Size1G;

impl PageSize for Size4K {
    const SIZE: usize = 0x1000;
}

impl PageSize for Size2M {
    const SIZE: usize = Size4K::SIZE * 512;
}

impl PageSize for Size1G {
    const SIZE: usize = Size2M::SIZE * 512;
}

//
// Virtual Page
//

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Page<S: PageSize> {
    number: usize,
    _size: PhantomData<S>,
}

impl<S: PageSize> Page<S> {
    pub fn containing_address(addr: usize) -> Self {
        Self {
            number: addr / S::SIZE,
            _size: PhantomData,
        }
    }

    pub fn start_address(&self) -> usize {
        self.number * S::SIZE
    }

    pub fn number(&self) -> usize {
        self.number
    }
}

impl<S: PageSize> core::fmt::Debug for Page<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Page(size={}, number={}, addr={:#x})",
            S::SIZE,
            self.number,
            self.start_address()
        )
    }
}

//
// Physical Frame
//

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frame<S: PageSize> {
    number: usize,
    _size: PhantomData<S>,
}

impl<S: PageSize> Frame<S> {
    pub fn containing_address(addr: usize) -> Self {
        Self {
            number: addr / S::SIZE,
            _size: PhantomData,
        }
    }

    pub fn start_address(&self) -> usize {
        self.number * S::SIZE
    }

    pub fn number(&self) -> usize {
        self.number
    }
}

impl<S: PageSize> core::fmt::Debug for Frame<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Frame(size={}, number={}, addr={:#x})",
            S::SIZE,
            self.number,
            self.start_address()
        )
    }
}
