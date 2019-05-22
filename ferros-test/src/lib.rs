#![recursion_limit = "256"]
extern crate proc_macro;
use proc_macro::TokenStream;
use quote::ToTokens;

mod codegen;
mod model;
mod parse;

use model::*;

#[proc_macro_attribute]
pub fn ferros_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let syn_content = match SynContent::parse(attr.into(), item.into()) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };
    let model = match TestModel::parse(syn_content) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };
    model
        .generate_runnable_test(codegen::UuidGenerator::default())
        .into_token_stream()
        .into()
}
