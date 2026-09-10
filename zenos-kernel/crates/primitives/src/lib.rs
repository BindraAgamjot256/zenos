#![cfg_attr(not(test), no_std)]
#![expect(incomplete_features)]
#![feature(coerce_unsized)]
#![feature(unsize)]
#![feature(ptr_metadata)]
#![feature(generic_const_exprs)]

pub mod alloc;
pub mod bitmap_allocator;
pub mod mutex;
pub mod rwlock;
