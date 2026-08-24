use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

pub fn expand(args: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert!(args.is_empty());

    // Generate an iterator of idents: isr_0, isr_1, ..., isr_255
    let idents: Vec<Ident> = (0..256)
        .map(|n| Ident::new(&format!("isr_{}", n), Span::call_site()))
        .collect();

    // Generate everything in a single macro expansion
    let expanded = quote! {
        // 1. Generate the extern declarations
        #(
            unsafe extern "C" {
                fn #idents();
            }
        )*

        // 2. Generate the static array mapping
        static ISR_HANDLERS: [unsafe extern "C" fn(); 256] = [
            #(
                #idents as unsafe extern "C" fn(),
            )*
        ];
    };

    TokenStream::from(expanded).into()
}
