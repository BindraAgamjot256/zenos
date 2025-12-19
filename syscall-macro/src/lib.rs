use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, LitInt, parse_macro_input};

#[proc_macro_attribute]
pub fn syscall(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let fn_name = &input_fn.sig.ident;

    let syscall_lit = parse_macro_input!(args as LitInt);
    let syscall_number = syscall_lit.base10_parse::<usize>().unwrap();

    let static_name = format_ident!("__SYSCALL_{}", fn_name);

    let expanded = quote! {
        // 1. Emit the function (extern "C" to ensure register usage)
        #[unsafe(no_mangle)]
        pub extern "C" #input_fn

        // 2. Emit the struct into the special section
        #[used]
        #[allow(non_upper_case_globals)]
        #[unsafe(link_section = "syscall_table")] // Section name
        static #static_name: SyscallPtr = SyscallPtr {
            id: #syscall_number,
            handler: unsafe {
                core::mem::transmute(
                    #fn_name as *const ()
                )
            },
        };
    };

    TokenStream::from(expanded)
}
#[proc_macro_attribute]
pub fn test(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let fn_name = &input_fn.sig.ident;

    let static_name = format_ident!("__SYSCALL_{}", fn_name);
    let name = fn_name.to_string();
    let name = name.as_str();
    let expanded = quote! {
        #input_fn

        // 2. Emit the struct into the special section
        #[used]
        #[allow(non_upper_case_globals)]
        #[unsafe(link_section = "tests")] // Section name
        static #static_name: Test = Test {
                handler:    #fn_name,
            name:       #name,
        };
    };

    TokenStream::from(expanded)
}
