use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{parse_macro_input, parse_quote, Field, Ident, ItemFn, ItemStruct};

fn root() -> TokenStream {
    use std::env::{var as env_var, VarError};

    let hydroflow_crate = proc_macro_crate::crate_name("cm_worker")
        .expect("cm_worker should be present in `Cargo.toml`");
    match hydroflow_crate {
        proc_macro_crate::FoundCrate::Itself => {
            if Err(VarError::NotPresent) == env_var("CARGO_BIN_NAME")
                && Err(VarError::NotPresent) != env_var("CARGO_PRIMARY_PACKAGE")
                && Ok("cm_worker") == env_var("CARGO_CRATE_NAME").as_deref()
            {
                // In the crate itself, including unit tests.
                quote! { crate }
            } else {
                // In an integration test, example, bench, etc.
                quote! { ::cm_worker }
            }
        }
        proc_macro_crate::FoundCrate::Name(name) => {
            let ident: Ident = Ident::new(&name, Span::call_site());
            quote! { ::#ident }
        }
    }
}

#[proc_macro_attribute]
pub fn local_async(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut f = parse_macro_input!(item as ItemFn);
    if f.sig.asyncness.is_none() {
        return quote_spanned! {f.sig.span()=>
            ::std::compile_error!("Must be `async`.")
        }
        .into();
    }
    let root = root();
    let block = &f.block;
    f.block = parse_quote! {
        {
            #root::local_future!(async #block).await
        }
    };
    f.to_token_stream().into()
}

#[proc_macro_derive(FromRefStatic)]
pub fn derive_from_ref_static(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let st = parse_macro_input!(item as ItemStruct);
    let root = root();
    let item_ident = &st.ident;
    st.fields
        .iter()
        .map(|Field { ident, ty, .. }| {
            quote! {
                impl #root::axum::extract::FromRef<&'static #item_ident> for &'static #ty {
                    fn from_ref(input: &&'static #item_ident) -> Self {
                        &input.#ident
                    }
                }
            }
        })
        .collect::<TokenStream>()
        .into()
}
