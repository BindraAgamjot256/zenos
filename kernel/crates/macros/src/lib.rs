mod allocator;
mod isrs;

#[proc_macro_attribute]
pub fn allocator(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let expanded = allocator::expand(attr, item);

    expanded.unwrap_or_else(|e| e.to_compile_error().into())
}

#[proc_macro]
pub fn gen_isrs(args: proc_macro::TokenStream) -> proc_macro::TokenStream {
    isrs::expand(args)
}
