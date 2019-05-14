extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use std::fmt::Display;
use syn::spanned::Spanned;
use syn::{parse_quote, Error as SynError, FnDecl, Ident, ItemFn, ReturnType, Type};

mod resource;

#[proc_macro_attribute]
pub fn ferros_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    let test_context = match parse_test_context(attr.into()) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };
    let test = match transform_into_runnable_test(input, test_context) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error().into(),
    };
    let output = quote! {
        #test
    };
    output.into()
}

fn transform_into_runnable_test(
    input: ItemFn,
    maybe_test_context: Option<TestContext>,
) -> Result<ItemFn, Error> {
    let test_context = maybe_test_context.unwrap_or_else(|| TestContext::Process);
    let resource_params = resource::extract_expected_resources(&input.decl.inputs)?;
    let user_fn_output_kind = UserTestFnOutput::interpret(&input.decl.output)?;
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
                span: attr.span(),
            }
        })?;
    if let Some(ident) = maybe_ident {
        let found = ident.to_string().to_lowercase();
        match found.as_ref() {
            "local" => Ok(Some(TestContext::Local)),
            "process" => Ok(Some(TestContext::Process)),
            "thread" => Ok(Some(TestContext::Thread)),
            _ => Err(Error::InvalidTestContext {
                found,
                span: ident.span(),
            }),
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
#[derive(Debug, Clone, Copy)]
enum UserTestFnOutput {
    Unit,
    TestOutcome,
    Result,
}

impl UserTestFnOutput {
    fn interpret(return_type: &ReturnType) -> Result<Self, Error> {
        match return_type {
            ReturnType::Default => Ok(UserTestFnOutput::Unit),
            ReturnType::Type(_, box_ty) => {
                match box_ty.as_ref() {
                    Type::Tuple(tuple) => {
                        // allow the explicit unit tuple, `()` case
                        if tuple.elems.is_empty() {
                            Ok(UserTestFnOutput::Unit)
                        } else {
                            Err(Error::InvalidReturnType { span: tuple.span() })
                        }
                    }
                    Type::Path(type_path) => {
                        let segment = type_path
                            .path
                            .segments
                            .last()
                            .ok_or_else(|| Error::InvalidReturnType {
                                span: type_path.span(),
                            })?
                            .into_value();
                        match segment.ident.to_string().as_ref() {
                            "Result" => Ok(UserTestFnOutput::Result),
                            "TestOutcome" => Ok(UserTestFnOutput::TestOutcome),
                            _ => Err(Error::InvalidReturnType {
                                span: type_path.span(),
                            }),
                        }
                    }
                    _ => Err(Error::InvalidReturnType {
                        span: return_type.span(),
                    }),
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum Error {
    InvalidArgumentType { msg: String, span: Span },
    InvalidTestContext { found: String, span: Span },
    InvalidReturnType { span: Span },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let s = match self {
            Error::InvalidArgumentType { msg, .. } => msg.clone(),
            Error::InvalidTestContext { found, .. } => format!(
                "Invalid test context found: {}. Select one of local or process",
                found
            ),
            Error::InvalidReturnType { .. } => {
                "Invalid return type, prefer returning either TestOutcome or a Result<T, E> type"
                    .to_string()
            }
        };
        f.write_str(&s)
    }
}

impl Error {
    fn span(&self) -> Span {
        match self {
            Error::InvalidArgumentType { span, .. } => *span,
            Error::InvalidTestContext { span, .. } => *span,
            Error::InvalidReturnType { span, .. } => *span,
        }
    }

    fn to_compile_error(&self) -> TokenStream2 {
        SynError::new(self.span(), self).to_compile_error()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
