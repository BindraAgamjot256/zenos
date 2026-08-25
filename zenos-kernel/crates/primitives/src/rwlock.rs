use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

const WRITE_BIT: usize = 1usize << (usize::BITS - 1);
const READER_MASK: usize = !WRITE_BIT;

/// A simple spinning reader-writer lock.
///
/// Multiple readers may hold the lock simultaneously, but only one writer
/// may hold it, and a writer excludes all readers.
///
/// This lock does not allocate and is suitable for `no_std` environments.
///
/// # Fairness
///
/// This implementation is reader-preferred. A writer may starve if readers
/// continuously acquire the lock.
pub struct RwLock<T> {
    inner: UnsafeCell<T>,
    state: AtomicUsize,
}

// The lock can be moved between threads when T can be moved safely.
unsafe impl<T: Send> Send for RwLock<T> {}

// Multiple threads may access the lock when T can safely be shared and
// transferred between threads.
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
            state: AtomicUsize::new(0),
        }
    }

    /// Acquires a shared/read lock.
    ///
    /// This spins until no writer owns the lock and then atomically adds
    /// this reader to the reader count.
    pub fn read(&self) -> ReadGuard<'_, T> {
        loop {
            let state = self.state.load(Ordering::Acquire);

            // A writer currently owns the lock.
            if state & WRITE_BIT != 0 {
                core::hint::spin_loop();
                continue;
            }

            // Avoid overflowing into the writer bit.
            if state == READER_MASK {
                core::hint::spin_loop();
                continue;
            }

            // Atomically add ourselves as a reader.
            //
            // If a writer acquires the lock between our load and this CAS,
            // the CAS fails and we retry.
            match self.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return ReadGuard {
                        lock: self,
                        _marker: PhantomData,
                    };
                }

                Err(_) => {
                    core::hint::spin_loop();
                }
            }
        }
    }

    /// Acquires an exclusive/write lock.
    ///
    /// The writer can only transition the state from completely unlocked
    /// (`0`) to the writer-owned state.
    pub fn write(&self) -> WriteGuard<'_, T> {
        loop {
            match self.state.compare_exchange_weak(
                0,
                WRITE_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return WriteGuard {
                        lock: self,
                        _marker: PhantomData,
                    };
                }

                Err(_) => {
                    core::hint::spin_loop();
                }
            }
        }
    }

    /// Returns true if a writer currently owns the lock.
    ///
    /// This is only a snapshot. The state may change immediately after
    /// this function returns.
    #[inline]
    pub fn is_write_locked(&self) -> bool {
        self.state.load(Ordering::Relaxed) & WRITE_BIT != 0
    }

    /// Returns the current number of readers.
    ///
    /// This is only a snapshot and should not be used for synchronization.
    #[inline]
    pub fn reader_count(&self) -> usize {
        self.state.load(Ordering::Relaxed) & READER_MASK
    }

    /// Returns true if the lock currently has no readers and no writer.
    ///
    /// This is only a snapshot.
    #[inline]
    pub fn is_unlocked(&self) -> bool {
        self.state.load(Ordering::Relaxed) == 0
    }
}

/// RAII guard for a shared/read lock.
pub struct ReadGuard<'a, T> {
    lock: &'a RwLock<T>,
    _marker: PhantomData<&'a T>,
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // We hold a read lock, so no writer can currently mutate `T`.
        unsafe { &*self.lock.inner.get() }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // Remove ourselves from the reader count.
        //
        // Release ensures operations performed while holding the read lock
        // are not reordered after releasing it.
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

/// RAII guard for an exclusive/write lock.
pub struct WriteGuard<'a, T> {
    lock: &'a RwLock<T>,
    _marker: PhantomData<&'a mut T>,
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // We own the WRITE_BIT, therefore there can be no readers or
        // another writer accessing T.
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // Release the writer ownership.
        self.lock.state.store(0, Ordering::Release);
    }
}
