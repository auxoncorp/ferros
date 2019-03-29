#![feature(extern_crate_item_prelude)]

extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree};
use proc_macro2::Span;
use quote::quote;
use syn::fold::Fold;
use syn::{parse_macro_input, Block, Ident};
use uuid::Uuid;

const EXPECTED_LAYOUT_MESSAGE: &'static str = r"smart_alloc expects to be invoked like:
smart_alloc! { |cslots as cs, untypeds as ut| {
    let id_that_will_leak = something_requiring_slots(cs);
    op_requiring_memory(ut);
    top_fn(cslots, nested_fn(cs, ut));
}}";

fn assert_tt_ident(maybe_tt: Option<TokenTree>) -> proc_macro::Ident {
    let id = maybe_tt.expect(EXPECTED_LAYOUT_MESSAGE);
    if let TokenTree::Ident(i) = id {
        i
    } else {
        panic!(EXPECTED_LAYOUT_MESSAGE);
    }
}

fn assert_tt_ident_named(maybe_tt: Option<TokenTree>, name: &'static str) -> proc_macro::Ident {
    let id = maybe_tt.expect(EXPECTED_LAYOUT_MESSAGE);
    if let TokenTree::Ident(i) = id {
        if &i.to_string() == name {
            i
        } else {
            panic!(EXPECTED_LAYOUT_MESSAGE);
        }
    } else {
        panic!(EXPECTED_LAYOUT_MESSAGE);
    }
}

fn assert_tt_punct_named(maybe_tt: Option<TokenTree>, name: char) -> proc_macro::Punct {
    let tt = maybe_tt.expect(EXPECTED_LAYOUT_MESSAGE);
    if let TokenTree::Punct(p) = tt {
        if p.as_char() == name {
            p
        } else {
            panic!(EXPECTED_LAYOUT_MESSAGE);
        }
    } else {
        panic!(EXPECTED_LAYOUT_MESSAGE);
    }
}

#[proc_macro]
pub fn smart_alloc(tokens: TokenStream) -> TokenStream {
    let mut tok_iter = tokens.into_iter();

    let _ = assert_tt_punct_named(tok_iter.next(), '|'); // open-delim
    let raw_cnode_id = assert_tt_ident(tok_iter.next()); // cnode resource
    let _ = assert_tt_ident_named(tok_iter.next(), "as"); // as
    let cslot_magic_target = assert_tt_ident(tok_iter.next()); // cslot magic name
    let _ = assert_tt_punct_named(tok_iter.next(), ','); // comma
    let raw_utbuddy_id = assert_tt_ident(tok_iter.next()); // untyped resource
    let _ = assert_tt_ident_named(tok_iter.next(), "as"); // as
    let untyped_magic_target = assert_tt_ident(tok_iter.next()); // untyped magic name
    let _ = assert_tt_punct_named(tok_iter.next(), '|'); // open-delim

    let content_block = tok_iter.collect();
    let mut block = parse_macro_input!(content_block as Block);

    let cnode_id = Ident::new(&raw_cnode_id.to_string(), Span::from(raw_cnode_id.span()));
    let untyped_id = Ident::new(
        &raw_utbuddy_id.to_string(),
        Span::from(raw_utbuddy_id.span()),
    );
    let mut id_tracker = IdTracker::new(
        Ident::new(&cslot_magic_target.to_string(), Span::call_site()),
        Ident::new(&untyped_magic_target.to_string(), Span::call_site()),
    );

    block = id_tracker.fold_block(block);

    let mut allocating_tokens = TokenStream::new();

    for plan in id_tracker.planned_allocs {
        match plan {
            PlannedAlloc::CSlot(id) => {
                let alloc_cslot = quote! {
                    let (#id, #cnode_id) = #cnode_id.alloc();
                };
                allocating_tokens.extend(TokenStream::from(alloc_cslot));
            }
            PlannedAlloc::UntypedBackedByCSlot { ut, cslot } => {
                let alloc_both = quote! {
                    let (#cslot, #cnode_id) = #cnode_id.alloc();
                    let (#ut, #untyped_id) = #untyped_id.alloc(#cslot);
                };
                allocating_tokens.extend(TokenStream::from(alloc_both));
            }
        }
    }
    let user_statements = block.stmts;
    let user_statement_tokens = quote! {
        #(#user_statements)*
    };
    allocating_tokens.extend(TokenStream::from(user_statement_tokens));
    allocating_tokens
}

struct IdTracker {
    target_cslot_ident: Ident,
    target_untyped_ident: Ident,
    planned_allocs: Vec<PlannedAlloc>,
}

enum PlannedAlloc {
    CSlot(Ident),
    UntypedBackedByCSlot { ut: Ident, cslot: Ident },
}

impl IdTracker {
    fn new(target_cslot_ident: Ident, target_untyped_ident: Ident) -> Self {
        IdTracker {
            target_cslot_ident,
            target_untyped_ident,
            planned_allocs: vec![],
        }
    }
}

fn make_ident(uuid: Uuid, name_hint: &'static str) -> Ident {
    syn::Ident::new(
        &format!("__smart_alloc_{}_{}", name_hint, uuid.to_simple()),
        Span::call_site(),
    )
}

impl Fold for IdTracker {
    fn fold_ident(&mut self, node: Ident) -> Ident {
        if node == self.target_cslot_ident {
            let fresh_id = make_ident(Uuid::new_v4(), "cslots");
            self.planned_allocs
                .push(PlannedAlloc::CSlot(fresh_id.clone()));
            fresh_id
        } else if node == self.target_untyped_ident {
            let fresh_id = make_ident(Uuid::new_v4(), "untyped");
            self.planned_allocs
                .push(PlannedAlloc::UntypedBackedByCSlot {
                    ut: fresh_id.clone(),
                    cslot: make_ident(Uuid::new_v4(), "cslots_for_untyped"),
                });
            fresh_id
        } else {
            node
        }
    }
}
