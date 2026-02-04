use core::mem::MaybeUninit;

/// A fixed-size overwriting ring buffer (circular buffer).
/// When full, pushing overwrites the oldest element.
pub struct RingBuf<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    head: usize,
    tail: usize,
    len: usize,
}

impl<T: Copy, const N: usize> RingBuf<T, N> {
    pub const fn new() -> Self {
        assert!(N > 0, "RingBuf capacity must be > 0");

        Self {
            buf: [MaybeUninit::uninit(); N],
            head: 0,
            tail: 0,
            len: 0,
        }
    }
}
impl<T, const N: usize> RingBuf<T, N> {
    #[inline]
    pub fn capacity(&self) -> usize {
        N
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Pushes to the back. Overwrites oldest element if full.
    pub fn push_back(&mut self, item: T) {
        if self.is_full() {
            // Drop the oldest element at head
            unsafe {
                self.buf[self.head].assume_init_drop();
            }
            self.head = (self.head + 1) % N;
            self.len -= 1;
        }

        self.buf[self.tail].write(item);
        self.tail = (self.tail + 1) % N;
        self.len += 1;
    }

    /// Pops the oldest element.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let item = unsafe { self.buf[self.head].assume_init_read() };
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(item)
    }

    pub fn front(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { &*self.buf[self.head].as_ptr() })
        }
    }

    pub fn back(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            let idx = if self.tail == 0 { N - 1 } else { self.tail - 1 };
            Some(unsafe { &*self.buf[idx].as_ptr() })
        }
    }

    pub fn clear(&mut self) {
        while self.pop_front().is_some() {}
    }

    pub fn iter(&self) -> Iter<'_, T, N> {
        Iter {
            buf: self,
            idx: self.head,
            remaining: self.len,
        }
    }
}

impl<T, const N: usize> Drop for RingBuf<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T: Copy, const N: usize> Default for RingBuf<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Iter<'a, T, const N: usize> {
    buf: &'a RingBuf<T, N>,
    idx: usize,
    remaining: usize,
}

impl<'a, T, const N: usize> Iterator for Iter<'a, T, N> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let item = unsafe { &*self.buf.buf[self.idx].as_ptr() };
        self.idx = (self.idx + 1) % N;
        self.remaining -= 1;
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
