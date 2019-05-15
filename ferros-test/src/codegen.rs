use crate::model::*;
use proc_macro2::Span;
use syn::{parse_quote, Block, Expr, ExprCall, ExprPath, FnDecl, Ident, ItemFn};

impl Model {
    pub(crate) fn generate_runnable_test(self) -> Result<ItemFn, CodegenError> {
        let original_fn_name = self.fn_under_test.ident.clone();
        let original_fn_name_literal = proc_macro2::Literal::string(&original_fn_name.to_string());
        let mut fn_under_test = self.fn_under_test.clone();
        let fn_under_test_ident = Ident::new("under_test", Span::call_site());
        fn_under_test.ident = fn_under_test_ident.clone();
        let invocation_as_outcome = self.map_under_test_invocation_to_outcome(fn_under_test_ident);
        let transformed_block = Box::new(parse_quote! { {
            #fn_under_test
            let outcome = #invocation_as_outcome;
            (#original_fn_name_literal , outcome)
        } });
        let transformed_fn = ItemFn {
            attrs: fn_under_test.attrs.clone(),
            vis: fn_under_test.vis.clone(),
            constness: None,
            unsafety: None,
            asyncness: None,
            abi: None,
            ident: original_fn_name,
            decl: Box::new(run_test_decl()),
            block: transformed_block,
        };
        Ok(parse_quote!(#transformed_fn))
    }

    fn map_under_test_invocation_to_outcome(self, fn_under_test_ident: Ident) -> Block {
        // TODO - if Process or Thread, produce a Proc/ThreadParams structure and RetypeForSetup impl
        // TODO - if Process or Thread, generate a test thread entry point function
        match self.execution_context {
            TestExecutionContext::Local => local_test_execution(self, fn_under_test_ident),
            TestExecutionContext::Process => unimplemented!(),
            TestExecutionContext::Thread => unimplemented!(),
        }
    }
}
fn single_segment_exprpath(ident: Ident) -> ExprPath {
    ExprPath {
        attrs: Vec::new(),
        qself: None,
        path: single_segment_path(ident),
    }
}
fn single_segment_path(ident: Ident) -> syn::Path {
    let mut segments = syn::punctuated::Punctuated::new();
    segments.push(parse_quote!(#ident));
    syn::Path {
        leading_colon: None,
        segments,
    }
}

fn local_test_execution(model: Model, fn_under_test_ident: Ident) -> Block {
    let (mut alloc_block, allocated_params) = local_allocations(&model.resources);
    let mut args = syn::punctuated::Punctuated::new();
    for p in allocated_params {
        args.push(Expr::Path(single_segment_exprpath(p.output_ident)))
    }
    let call = ExprCall {
        attrs: Vec::new(),
        func: Box::new(Expr::Path(single_segment_exprpath(fn_under_test_ident))),
        paren_token: syn::token::Paren::default(),
        args,
    };
    let call_block: Block = match model.fn_under_test_output {
        UserTestFnOutput::Unit => parse_quote! { {
            #call;
            ferros::test_support::TestOutcome::Success
        } },
        UserTestFnOutput::TestOutcome => parse_quote! {{
            #call
        }},
        UserTestFnOutput::Result => parse_quote! {{
            match #call {
                Ok(_) => ferros::test_support::TestOutcome::Success,
                Err(_) => ferros::test_support::TestOutcome::Failure,
            }
        }},
    };
    alloc_block.stmts.extend(call_block.stmts);
    alloc_block
}

#[derive(Debug, Clone)]
struct AllocatedParam {
    param: Param,
    output_ident: Ident,
}

fn local_allocations(params: &[Param]) -> (Block, Vec<AllocatedParam>) {
    let mut allocated_params = Vec::new();
    let mut stmts = Vec::new();
    // Available ids provided as parameters by the enclosing function
    // These need to match the inputs declared by `run_test_decl`
    let slots = Ident::new("slots", Span::call_site());
    let untyped = Ident::new("untyped", Span::call_site());
    let asid_pool = Ident::new("asid_pool", Span::call_site());
    let scratch = Ident::new("scratch", Span::call_site());
    let local_cnode = Ident::new("local_cnode", Span::call_site());
    let thread_authority = Ident::new("thread_authority", Span::call_site());
    let user_image = Ident::new("user_image", Span::call_site());
    let ut_buddy_instance = Ident::new("ut_buddy_instance", Span::call_site());
    let buddy_block: Block = if params.iter().any(|p| {
        if let ParamKind::Untyped { .. } = p.kind {
            true
        } else {
            false
        }
    }) {
        parse_quote! {{
            let #ut_buddy_instance = ferros::alloc::ut_buddy(#untyped);
        }}
    } else {
        parse_quote!({})
    };
    stmts.extend(buddy_block.stmts);

    for p in params {
        let (p_block, output_ident): (Block, Ident) = match p.kind {
            ParamKind::CNodeSlots { .. } => {
                let slot_id = gen_id("cnodeslots");
                (
                    parse_quote! {{
                        let (#slot_id, #slots) = #slots.alloc();
                    }},
                    slot_id,
                )
            }
            ParamKind::Untyped { .. } => {
                let slot_id = gen_id("cnodeslots");
                let ut_id = gen_id("untyped");
                (
                    parse_quote! {{
                        let (#slot_id, #slots) = #slots.alloc();
                        // TODO - revisit the use of unwrap here versus piping out SeL4Error
                        let (#ut_id, #untyped) = #ut_buddy_instance.alloc(#slot_id).unwrap();
                    }},
                    ut_id,
                )
            }
            ParamKind::ASIDPool { .. } => {
                let pool_id = gen_id("asidpool");
                (
                    parse_quote! {{
                        let #pool_id = #asid_pool.truncate();
                    }},
                    pool_id,
                )
            }
            ParamKind::VSpaceScratch => (parse_quote!({}), scratch.clone()),
            ParamKind::CNode => (parse_quote!({}), local_cnode.clone()),
            ParamKind::ThreadPriorityAuthority => (parse_quote!({}), thread_authority.clone()),
            ParamKind::UserImage => (parse_quote!({}), user_image.clone()),
        };
        stmts.extend(p_block.stmts);
        allocated_params.push(AllocatedParam {
            param: p.clone(),
            output_ident,
        })
    }

    (
        Block {
            brace_token: syn::token::Brace::default(),
            stmts,
        },
        allocated_params,
    )
}

fn gen_id(name_hint: &'static str) -> Ident {
    syn::Ident::new(
        &format!(
            "__ferros_test_{}_{}",
            name_hint,
            uuid::Uuid::new_v4().to_simple()
        ),
        Span::call_site(),
    )
}

fn run_test_decl() -> FnDecl {
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
    FnDecl {
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
    }
}
