use crate::model::*;
use syn::{parse_quote, FnDecl, ItemFn};

impl Model {
    pub(crate) fn generate_runnable_test(self) -> Result<ItemFn, CodegenError> {
        // TODO - map original fn output to TestOutcome -> if original output is Default (unit), must run in a child thread/process
        let original_fn_name_literal =
            proc_macro2::Literal::string(&self.fn_under_test.ident.to_string());
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
            thread_authority:
                &ferros::userland::LocalCap<ferros::userland::ThreadPriorityAuthority>
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
            attrs: self.fn_under_test.attrs.clone(),
            vis: self.fn_under_test.vis.clone(),
            constness: None,
            unsafety: None,
            asyncness: None,
            abi: None,
            ident: self.fn_under_test.ident.clone(),
            decl: Box::new(run_test_decl),
            block: transformed_block,
        };
        Ok(parse_quote!(#transformed_fn))
    }
}
