#![no_std]

extern crate alloc;
extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use alloc::string::ToString;
use proc_macro::TokenStream;

#[proc_macro_derive(RuntimeReason)]
pub fn derive_runtime_reason(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_runtime_reason(&ast);
    gen.parse().unwrap()
}

fn impl_runtime_reason(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    quote! { impl RuntimeReason for #name {} }
}

#[proc_macro_derive(SystemReason)]
pub fn derive_system_reason(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_system_reason(&ast);
    gen.parse().unwrap()
}

fn impl_system_reason(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    quote! { impl SystemReason for #name {} }
}
