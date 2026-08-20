use quote::format_ident;
use syn::Token;

/// Expands the allocator attribute.
///
/// The attribute expects the following syntax:
///
/// ```text
/// #[allocator(type = SomeType)]
/// struct SomeAllocator;
/// ```
///
/// The generated allocator uses a slab cache to allocate objects of the
/// specified type. Allocation requests must have a layout that exactly
/// matches the object's size and alignment.
pub fn expand(
    attr: proc_macro::TokenStream, // type = {object_type}
    item: proc_macro::TokenStream, // struct {allocator_name}
) -> syn::Result<proc_macro::TokenStream> {
    let args: AllocatorArgs = syn::parse2(attr.into())?;

    let object_type = &args.object_type;
    let item: syn::ItemStruct = syn::parse2(item.into())?;
    let allocator_name = &item.ident;

    let cache_struct_name = format_ident!("{}Cache", allocator_name);
    let cache_static_name = format_ident!("{}", cache_struct_name.to_string().to_ascii_uppercase());
    let module_name = format_ident!(
        "{}_private",
        &allocator_name.to_string().to_ascii_lowercase()
    );

    Ok(quote::quote! {
        #item

        #[doc(hidden)]
        mod #module_name {
            pub(super) struct #cache_struct_name {
                pub(super) cache: kprimitives::mutex::Mutex<
                    kmm::slab::SlabCache<
                        {
                            let size = core::alloc::Layout::new::<super::#object_type>().size();
                            let align = core::alloc::Layout::new::<super::#object_type>().align();
                            if size >= align {
                                size.next_power_of_two()
                            } else {
                                align
                            }
                        },
                        crate::mm::SlabBackend,
                    >,
                >,
            }

            unsafe impl Send for #cache_struct_name {}
            unsafe impl Sync for #cache_struct_name {}

            pub(super) static #cache_static_name: #cache_struct_name = #cache_struct_name {
                cache: kprimitives::mutex::Mutex::new(
                    kmm::slab::SlabCache::new(crate::mm::SlabBackend)
                ),
            };
        }

        unsafe impl kprimitives::alloc::Allocator for #allocator_name {
            fn allocate(
                layout: core::alloc::Layout,
            ) -> Result<kprimitives::alloc::Allocation, kprimitives::alloc::AllocationError> {
                if layout.size() != core::alloc::Layout::new::<#object_type>().size() {
                    return Err(kprimitives::alloc::AllocationError::UnsupportedLayout);
                }
                if layout.align() != core::alloc::Layout::new::<#object_type>().align() {
                    return Err(kprimitives::alloc::AllocationError::UnsupportedLayout);
                }
                let mut cache = #module_name::#cache_static_name.cache.lock();
                let allocation = cache
                    .allocate()
                    .ok_or(kprimitives::alloc::AllocationError::OutOfMemory)?;
                Ok(kprimitives::alloc::Allocation::from_ptr(allocation))
            }

            fn deallocate(
                allocation: kprimitives::alloc::Allocation,
                layout: core::alloc::Layout,
            ) {
                if layout.size() != core::alloc::Layout::new::<#object_type>().size() {
                    panic!(
                        "layout size mismatch: expected {}, got {}",
                        core::alloc::Layout::new::<#object_type>().size(),
                        layout.size()
                    );
                }
                if layout.align() != core::alloc::Layout::new::<#object_type>().align() {
                    panic!(
                        "layout align mismatch: expected {}, got {}",
                        core::alloc::Layout::new::<#object_type>().align(),
                        layout.align()
                    );
                }
                let mut cache = #module_name::#cache_static_name.cache.lock();
                cache.deallocate(allocation.as_ptr());
            }
        }
    }
    .into())
}

/// Arguments accepted by the allocator attribute.
struct AllocatorArgs {
    pub object_type: syn::Type,
}

impl syn::parse::Parse for AllocatorArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Parse "type = {syn::Type}"
        let _type: Token![type] = input.parse()?;
        let _equals: Token![=] = input.parse()?;
        let ty: syn::Type = input.parse()?;

        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }

        Ok(Self { object_type: ty })
    }
}
