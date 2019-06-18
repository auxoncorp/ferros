use crate::model::*;
use proc_macro2::Span;
use syn::{parse_quote, Block, Expr, ExprCall, ExprPath, FnDecl, Ident, ItemFn};

impl TestModel {
    pub(crate) fn generate_runnable_test<G: IdGenerator>(self, mut id_generator: G) -> ItemFn {
        let original_fn_name = self.fn_under_test.ident.clone();
        let original_fn_name_literal = proc_macro2::Literal::string(&original_fn_name.to_string());
        let mut fn_under_test = self.fn_under_test.clone();
        let fn_under_test_ident = Ident::new("under_test", Span::call_site());
        fn_under_test.ident = fn_under_test_ident.clone();
        let invocation_as_outcome =
            self.map_under_test_invocation_to_outcome(&mut id_generator, fn_under_test_ident);
        let transformed_block = Box::new(parse_quote! { {
            #fn_under_test
            let outcome = #invocation_as_outcome;
            (concat!(module_path!(), "::", #original_fn_name_literal) , outcome)
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
        parse_quote!(#transformed_fn)
    }

    fn map_under_test_invocation_to_outcome<G: IdGenerator>(
        self,
        id_generator: &mut G,
        fn_under_test_ident: Ident,
    ) -> Block {
        match self.execution_context {
            TestExecutionContext::Local => {
                local_test_execution(self, id_generator, fn_under_test_ident)
            }
            TestExecutionContext::Process => process_test_execution(self, fn_under_test_ident),
        }
    }
}

#[derive(Debug, Clone)]
struct AllocatedParam {
    param: Param,
    output_ident: Ident,
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

fn process_test_execution(model: TestModel, _fn_under_test_ident: Ident) -> Block {
    assert_eq!(model.execution_context, TestExecutionContext::Process);
    // TODO - produce a Proc/ThreadParams structure and RetypeForSetup impl
    // TODO - if Process, generate a test thread entry point function
    unimplemented!()
}

fn call_fn_under_test(
    fn_under_test_ident: Ident,
    fn_under_test_output: UserTestFnOutput,
    param_ids: impl Iterator<Item = Ident>,
) -> Block {
    let mut args = syn::punctuated::Punctuated::new();
    for id in param_ids {
        args.push(Expr::Path(single_segment_exprpath(id)))
    }
    let call = ExprCall {
        attrs: Vec::new(),
        func: Box::new(Expr::Path(single_segment_exprpath(fn_under_test_ident))),
        paren_token: syn::token::Paren::default(),
        args,
    };
    match fn_under_test_output {
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
    }
}

fn local_test_execution<G: IdGenerator>(
    model: TestModel,
    id_generator: &mut G,
    fn_under_test_ident: Ident,
) -> Block {
    assert_eq!(model.execution_context, TestExecutionContext::Local);
    let (mut alloc_block, allocated_params) = local_allocations(id_generator, &model.resources);
    let call_block = call_fn_under_test(
        fn_under_test_ident,
        model.fn_under_test_output,
        allocated_params.into_iter().map(|p| p.output_ident),
    );
    alloc_block.stmts.extend(call_block.stmts);
    alloc_block
}

fn local_allocations<G: IdGenerator>(
    id_generator: &mut G,
    params: &[Param],
) -> (Block, Vec<AllocatedParam>) {
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
    let mapped_memory_region = Ident::new("mapped_memory_region", Span::call_site());
    let is_untyped = |p: &Param| {
        if let ParamKind::Untyped { .. } = p.kind {
            true
        } else {
            false
        }
    };
    let buddy_block: Block = if params.iter().any(is_untyped) {
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
                let slot_id = gen_id(id_generator, "cnodeslots");
                (
                    parse_quote! {{
                        let (#slot_id, #slots) = #slots.alloc();
                    }},
                    slot_id,
                )
            }
            ParamKind::Untyped { .. } => {
                let slot_id = gen_id(id_generator, "cnodeslots");
                let ut_id = gen_id(id_generator, "untyped");
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
                let pool_id = gen_id(id_generator, "asidpool");
                (
                    parse_quote! {{
                        let #pool_id = #asid_pool.truncate();
                    }},
                    pool_id,
                )
            }
            ParamKind::VSpaceScratch => (parse_quote!({}), scratch.clone()),
            ParamKind::MappedMemoryRegion => {
                // TODO - be sure that split/alloc prevents making too-small of regions
                // such that page alignment would be violated
                let region_id = gen_id(id_generator, "mappedmemoryregion");
                (
                    parse_quote! {{
                        let (#region_id, #mapped_memory_region) = #mapped_memory_region.split_into().unwrap();
                    }},
                    region_id,
                )
            }
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

pub trait IdGenerator {
    fn gen(&mut self, name_hint: &'static str) -> String;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UuidGenerator;

impl IdGenerator for UuidGenerator {
    fn gen(&mut self, name_hint: &'static str) -> String {
        format!(
            "__ferros_test_{}_{}",
            name_hint,
            uuid::Uuid::new_v4().to_simple()
        )
    }
}

fn gen_id<G: IdGenerator>(g: &mut G, name_hint: &'static str) -> Ident {
    syn::Ident::new(&g.gen(name_hint), Span::call_site())
}

fn run_test_decl() -> FnDecl {
    let mut run_test_inputs = syn::punctuated::Punctuated::new();
    run_test_inputs.push(parse_quote!(
        slots: ferros::cap::LocalCNodeSlots<ferros::test_support::MaxTestCNodeSlots>
    ));
    run_test_inputs.push(parse_quote!(
        untyped:
            ferros::cap::LocalCap<ferros::cap::Untyped<ferros::test_support::MaxTestUntypedSize>>
    ));
    run_test_inputs.push(parse_quote!(
        asid_pool:
            ferros::cap::LocalCap<ferros::cap::ASIDPool<ferros::test_support::MaxTestASIDPoolSize>>
    ));
    run_test_inputs.push(parse_quote!(scratch: &mut ferros::vspace::ScratchRegion));
    run_test_inputs.push(parse_quote!(
        mapped_memory_region:
            ferros::vspace::MappedMemoryRegion<
                ferros::test_support::MaxMappedMemoryRegionBitSize,
                ferros::vspace::shared_status::Exclusive,
            >
    ));
    run_test_inputs.push(parse_quote!(
        local_cnode: &ferros::cap::LocalCap<ferros::cap::LocalCNode>
    ));
    run_test_inputs.push(parse_quote!(
        thread_authority: &ferros::cap::LocalCap<ferros::cap::ThreadPriorityAuthority>
    ));
    run_test_inputs.push(parse_quote!(
        user_image: &ferros::bootstrap::UserImage<ferros::cap::role::Local>
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;
    use syn::parse_quote;

    struct DummyIdGenerator {
        prefix: &'static str,
        count: usize,
    }

    impl IdGenerator for DummyIdGenerator {
        fn gen(&mut self, _name_hint: &'static str) -> String {
            let s = format!("{}{}", self.prefix, self.count);
            self.count += 1;
            s
        }
    }
    #[test]
    fn happy_path_generate_no_params() {
        let fn_under_test = parse_quote! {
            fn original_target() {
                assert!(true);
            }
        };
        let model = TestModel {
            execution_context: TestExecutionContext::Local,
            fn_under_test,
            fn_under_test_output: UserTestFnOutput::Unit,
            resources: Vec::new(),
        };
        let test = model.generate_runnable_test(DummyIdGenerator {
            prefix: "_a",
            count: 0,
        });
        assert_eq!("original_target", &test.ident.to_string());

        let expected: ItemFn = parse_quote! {
            fn original_target(
                slots: ferros::cap::LocalCNodeSlots<ferros::test_support::MaxTestCNodeSlots>,
                untyped: ferros::cap::LocalCap<
                    ferros::cap::Untyped<ferros::test_support::MaxTestUntypedSize>>,
                asid_pool: ferros::cap::LocalCap<
                    ferros::cap::ASIDPool<ferros::test_support::MaxTestASIDPoolSize>>,
                scratch: &mut ferros::vspace::ScratchRegion,
                mapped_memory_region: ferros::vspace::MappedMemoryRegion<
                    ferros::test_support::MaxMappedMemoryRegionBitSize, ferros::vspace::shared_status::Exclusive,>,
                local_cnode: &ferros::cap::LocalCap<ferros::cap::LocalCNode>,
                thread_authority: &ferros::cap::LocalCap<ferros::cap::ThreadPriorityAuthority>,
                user_image: &ferros::bootstrap::UserImage<ferros::cap::role::Local>
            ) -> (&'static str, ferros::test_support::TestOutcome) {
                fn under_test() {
                    assert!(true);
                }
                let outcome = {
                    under_test();
                    ferros::test_support::TestOutcome::Success
                };
                (concat!(module_path!(), "::", "original_target"), outcome)
            }
        };

        assert_eq!(
            expected.into_token_stream().to_string(),
            test.into_token_stream().to_string()
        );
    }

    #[test]
    fn happy_path_generate_with_params() {
        let fn_under_test = parse_quote! {
            fn original_target(ut: LocalCap<Untyped<U5>>, sl: LocalCNodeSlots<U4>) -> Result<(), SeL4Error> {
                let r = ut.split(sl);
                assert!(r.is_ok());
                Ok(())
            }
        };
        let model = TestModel {
            execution_context: TestExecutionContext::Local,
            fn_under_test,
            fn_under_test_output: UserTestFnOutput::Result,
            resources: vec![
                Param {
                    original_ident: Ident::new("ut", Span::call_site()),
                    kind: ParamKind::Untyped { bits: 5 },
                },
                Param {
                    original_ident: Ident::new("sl", Span::call_site()),
                    kind: ParamKind::CNodeSlots { count: 4 },
                },
            ],
        };
        let test = model.generate_runnable_test(DummyIdGenerator {
            prefix: "_a",
            count: 0,
        });
        assert_eq!("original_target", &test.ident.to_string());

        let expected: ItemFn = parse_quote! {
            fn original_target(
                slots: ferros::cap::LocalCNodeSlots<ferros::test_support::MaxTestCNodeSlots>,
                untyped: ferros::cap::LocalCap<
                    ferros::cap::Untyped<ferros::test_support::MaxTestUntypedSize>>,
                asid_pool: ferros::cap::LocalCap<
                    ferros::cap::ASIDPool<ferros::test_support::MaxTestASIDPoolSize>>,
                scratch: &mut ferros::vspace::ScratchRegion,
                mapped_memory_region: ferros::vspace::MappedMemoryRegion<
                    ferros::test_support::MaxMappedMemoryRegionBitSize, ferros::vspace::shared_status::Exclusive,>,
                local_cnode: &ferros::cap::LocalCap<ferros::cap::LocalCNode>,
                thread_authority: &ferros::cap::LocalCap<ferros::cap::ThreadPriorityAuthority>,
                user_image: &ferros::bootstrap::UserImage<ferros::cap::role::Local>
            ) -> (&'static str, ferros::test_support::TestOutcome) {
                fn under_test(ut: LocalCap<Untyped<U5>>, sl: LocalCNodeSlots<U4>) -> Result<(), SeL4Error> {
                    let r = ut.split(sl);
                    assert!(r.is_ok());
                    Ok(())
                }
                let outcome = {
                    let ut_buddy_instance = ferros::alloc::ut_buddy(untyped);
                    let ( _a0 , slots ) = slots.alloc();
                    let ( _a1 , untyped ) = ut_buddy_instance.alloc(_a0).unwrap ();
                    let ( _a2 , slots ) = slots.alloc();
                    match under_test(_a1, _a2) {
                        Ok(_) => ferros::test_support::TestOutcome::Success,
                        Err(_) => ferros::test_support::TestOutcome::Failure,
                    }
                };
                (concat!(module_path!(), "::", "original_target"), outcome)
            }
        };

        assert_eq!(
            expected.into_token_stream().to_string(),
            test.into_token_stream().to_string()
        );
    }
    #[test]
    fn happy_path_mapped_memory_region() {
        let fn_under_test = parse_quote! {
            fn original_target(mem: MappedMemoryRegion<U12, shared_status::Exclusive>) -> Result<(), SeL4Error> {
                Ok(())
            }
        };
        let model = TestModel {
            execution_context: TestExecutionContext::Local,
            fn_under_test,
            fn_under_test_output: UserTestFnOutput::Result,
            resources: vec![Param {
                original_ident: Ident::new("mem", Span::call_site()),
                kind: ParamKind::MappedMemoryRegion,
            }],
        };
        let test = model.generate_runnable_test(DummyIdGenerator {
            prefix: "_a",
            count: 0,
        });
        assert_eq!("original_target", &test.ident.to_string());

        let expected: ItemFn = parse_quote! {
            fn original_target(
                slots: ferros::cap::LocalCNodeSlots<ferros::test_support::MaxTestCNodeSlots>,
                untyped: ferros::cap::LocalCap<
                    ferros::cap::Untyped<ferros::test_support::MaxTestUntypedSize>>,
                asid_pool: ferros::cap::LocalCap<
                    ferros::cap::ASIDPool<ferros::test_support::MaxTestASIDPoolSize>>,
                scratch: &mut ferros::vspace::ScratchRegion,
                mapped_memory_region: ferros::vspace::MappedMemoryRegion<
                    ferros::test_support::MaxMappedMemoryRegionBitSize, ferros::vspace::shared_status::Exclusive,>,
                local_cnode: &ferros::cap::LocalCap<ferros::cap::LocalCNode>,
                thread_authority: &ferros::cap::LocalCap<ferros::cap::ThreadPriorityAuthority>,
                user_image: &ferros::bootstrap::UserImage<ferros::cap::role::Local>
            ) -> (&'static str, ferros::test_support::TestOutcome) {
                fn under_test(mem: MappedMemoryRegion<U12, shared_status::Exclusive>) -> Result<(), SeL4Error> {
                    Ok(())
                }
                let outcome = {
                    let ( _a0 , mapped_memory_region ) = mapped_memory_region.alloc();
                    match under_test(_a0) {
                        Ok(_) => ferros::test_support::TestOutcome::Success,
                        Err(_) => ferros::test_support::TestOutcome::Failure,
                    }
                };
                (concat!(module_path!(), "::", "original_target"), outcome)
            }
        };

        assert_eq!(
            expected.into_token_stream().to_string(),
            test.into_token_stream().to_string()
        );
    }
}
