use core::ptr::NonNull;

pub(crate) struct FreeBlockNode {
    pub(crate) next: Option<NonNull<FreeBlockNode>>,
    pub(crate) prev: Option<NonNull<FreeBlockNode>>,
}
