extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, TokenStreamExt};
use std::fmt::Display;
use syn::{parse_quote, FnDecl, Ident, ItemFn};

#[proc_macro_attribute]
pub fn ferros_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    let test_context = parse_test_context(attr.into()).unwrap_or_else(|e| panic!("{}", e));
    let transformed =
        transform_into_runnable_test(input, test_context).unwrap_or_else(|e| panic!("{}", e));
    let output = quote! {
        #transformed
    };
    output.into()
}

fn transform_into_runnable_test(
    input: ItemFn,
    maybe_test_context: Option<TestContext>,
) -> Result<ItemFn, Error> {
    let test_context = maybe_test_context.unwrap_or_else(|| TestContext::Process);
    // TODO - parse param-based resource requests
    // TODO - map original fn output to TestOutcome -> if original output is Default (unit), must run in a child thread/process
    let original_fn_name_literal = proc_macro2::Literal::string(&input.ident.to_string());
    let transformed_block = Box::new(parse_quote! { {
        // TODO - allocate param-based requests
        // TODO - if Process or Thread, produce a Proc/ThreadParams structure and RetypeForSetup impl
        // TODO - if Process or Thread, generate a test thread entry point function
        (#original_fn_name_literal , unimplemented!())
    } });

    let mut run_test_inputs = syn::punctuated::Punctuated::new();
    run_test_inputs.push(parse_quote!(
        slots: ferros::userland::LocalCNodeSlots<ferros::test_support::MaxTestCNodeSlots>
    ));
    run_test_inputs.push(parse_quote!(
        untyped:
            ferros::userland::LocalCap<
                ferros::userland::Untyped<ferros::test_support::MaxTestUntypedSize>,
            >
    ));
    run_test_inputs.push(parse_quote!(
        asid_pool:
            ferros::userland::LocalCap<
                ferros::userland::ASIDPool<ferros::test_support::MaxTestASIDPoolSize>,
            >
    ));
    run_test_inputs.push(parse_quote!(
        scratch: &mut ferros::userland::VSpaceScratchSlice<ferros::userland::role::Local>
    ));
    run_test_inputs.push(parse_quote!(
        local_cnode: &ferros::userland::LocalCap<ferros::userland::LocalCNode>
    ));
    run_test_inputs.push(parse_quote!(
        thread_authority: &ferros::userland::LocalCap<ferros::userland::ThreadPriorityAuthority>
    ));
    run_test_inputs.push(parse_quote!(
        user_image: &ferros::userland::UserImage<ferros::userland::role::Local>
    ));
    let run_test_decl = FnDecl {
        fn_token: syn::token::Fn::default(),
        generics: syn::Generics::default(),
        paren_token: syn::token::Paren::default(),
        inputs: run_test_inputs,
        variadic: None,
        output: syn::ReturnType::Type(
            syn::token::RArrow::default(),
            Box::new(parse_quote!((
                &'static str,
                ferros::test_support::TestOutcome
            ))),
        ),
    };
    let transformed_fn = ItemFn {
        attrs: input.attrs.clone(),
        vis: input.vis.clone(),
        constness: None,
        unsafety: None,
        asyncness: None,
        abi: None,
        ident: input.ident.clone(),
        decl: Box::new(run_test_decl),
        block: transformed_block,
    };
    Ok(parse_quote! {
        #transformed_fn
    })
}

// TODO - consider richer attribute content layout for better expandability
fn parse_test_context(attr: TokenStream2) -> Result<Option<TestContext>, Error> {
    let maybe_ident: Option<Ident> =
        syn::parse_macro_input::parse(attr.clone().into()).map_err(|_e| {
            Error::InvalidTestContext {
                found: attr.to_string(),
            }
        })?;
    if let Some(ident) = maybe_ident {
        let found = ident.to_string().to_lowercase();
        match found.as_ref() {
            "local" => Ok(Some(TestContext::Local)),
            "process" => Ok(Some(TestContext::Process)),
            "thread" => Ok(Some(TestContext::Thread)),
            _ => Err(Error::InvalidTestContext { found }),
        }
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy)]
enum TestContext {
    /// Runs in the test harness' local scope (often the root task)
    Local,
    /// Runs in an isolated child process
    Process,
    /// Runs in a distinct thread in the test harness' virtual address space (often the root task process)
    Thread,
}

#[derive(Debug)]
enum Error {
    UnknownParameterType { fn_name: String, param: String },
    InvalidTestContext { found: String },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let s = match self {
            Error::UnknownParameterType { fn_name, param } => format!(
                "{} contained a parameter {} with an unrecognized type",
                fn_name, param
            ),
            Error::InvalidTestContext { found } => format!(
                "Invalid test context found: {}. Select one of local or process",
                found
            ),
        };
        f.write_str(&s)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
