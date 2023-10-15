//! The 'stroke!()' macro.
//!
//! This macro convererts a steno stroke in textual format into the internal
//! integer representation at compile time.

use bbq_steno::stroke::Stroke;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn stroke(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);

    let stroke = match Stroke::from_text(&input.value()) {
        Ok(str) => str,
        Err(e) => {
            return syn::Error::new(input.span(), e)
                .into_compile_error()
                .into();
        }
    };

    let stroke = stroke.into_raw();
    let expanded = quote! {
        ::bbq_steno::stroke::Stroke::from_raw(#stroke)
    };

    TokenStream::from(expanded)
}
