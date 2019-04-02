#![feature(extern_crate_item_prelude)]

extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenTree};
use quote::quote;
use syn::export::TokenStream2;
use syn::fold::Fold;
use syn::parse_macro_input::parse as syn_parse;
use syn::{Error as SynError, Ident};
use uuid::Uuid;

const RESOURCE_TYPE_HINT_CSLOTS: &str = "CNodeSlots";
const RESOURCE_TYPE_HINT_UNTYPED: &str = "UntypedBuddy";
const RESOURCE_TYPE_HINT_ADDR: &str = "AddressBuddy";

const EXPECTED_LAYOUT_MESSAGE: &str = r"smart_alloc expects to be invoked like:
smart_alloc! { |cs from cslots, ut from untypeds, ad from address_buddy | {
    let id_that_will_leak = something_requiring_slots(cs);
    op_requiring_memory(ut);
    top_fn(cs, nested_fn(cs, ut));
}}";
const RESOURCE_DECLARATION_LAYOUT_MESSAGE: &str =
    r"When a resource is declared, it should be in one of the following forms:
* `request_id from resource_id`
* `request_id from resource_id: ResourceKind`

where ResourceKind is one of CNodeSlots, UntypedBuddy, or AddressBuddy";

#[proc_macro]
pub fn smart_alloc(tokens: TokenStream) -> TokenStream {
    smart_alloc_impl(TokenStream2::from(tokens))
        .unwrap_or_else(|e| match e {
            Error::BlockParse(se) => se.to_compile_error(),
            _ => panic!(EXPECTED_LAYOUT_MESSAGE), // TODO - fill out other branches with proper error messages
        })
        .into()
}

fn smart_alloc_impl(tokens: TokenStream2) -> Result<TokenStream2, Error> {
    let (header, stream_remainder) = parse_header(tokens)?;
    let content_block: TokenStream = stream_remainder.into();
    let mut block = syn_parse(content_block)?;
    // Find all requests for allocations, replace the relevant target-ids with unique ids,
    // and construct an allocation plan for each site for later code generation
    let mut id_tracker = IdTracker::from(&header);
    block = id_tracker.fold_block(block);

    let ResolvedResourceIds {
        cslots_resource,
        untyped_resource,
        address_resource,
    } = ResolvedResourceIds::resolve(&header, &id_tracker.planned_allocs).unwrap();

    let mut output_tokens = TokenStream2::new();
    for plan in id_tracker.planned_allocs {
        match plan {
            PlannedAlloc::CSlot(id) => {
                let alloc_cslot = quote! {
                    let (#id, #cslots_resource) = #cslots_resource.alloc();
                };
                output_tokens.extend(alloc_cslot);
            }
            PlannedAlloc::Untyped { ut, cslot } => {
                let alloc_both = quote! {
                    let (#cslot, #cslots_resource) = #cslots_resource.alloc();
                    let (#ut, #untyped_resource) = #untyped_resource.alloc(#cslot)?;
                };
                output_tokens.extend(alloc_both);
            }
            PlannedAlloc::AddressRange {
                addr,
                cslot_for_addr,
                ut,
                cslot_for_ut,
            } => {
                let alloc_addr = quote! {
                    let (#cslot_for_ut, #cslots_resource) = #cslots_resource.alloc();
                    let (#ut, #untyped_resource) = #untyped_resource.alloc(#cslot_for_ut)?;
                    let (#cslot_for_addr, #cslots_resource) = #cslots_resource.alloc();
                    let (#addr, #address_resource) = #address_resource.alloc(#cslot_for_addr, #ut)?;
                };
                output_tokens.extend(alloc_addr);
            }
        }
    }
    let user_statements = block.stmts;
    let user_statement_tokens = quote! {
        #(#user_statements)*
    };
    output_tokens.extend(user_statement_tokens);
    Ok(output_tokens)
}

fn assert_tt_ident(maybe_tt: Option<TokenTree>) -> Result<proc_macro2::Ident, Error> {
    let tt = maybe_tt.ok_or_else(|| Error::NotEnoughTokens)?;
    if let TokenTree::Ident(i) = tt {
        Ok(i)
    } else {
        Err(Error::IncorrectExpectedTokenTreeVariant {
            found: tt.to_string(),
            expected: "identifier",
        })
    }
}

fn assert_tt_ident_named(
    maybe_tt: Option<TokenTree>,
    name: &'static str,
) -> Result<proc_macro2::Ident, Error> {
    let tt = assert_tt_ident(maybe_tt)?;
    let found = tt.to_string();
    if found == name {
        Ok(tt)
    } else {
        Err(Error::IncorrectExpectedTokenContent {
            found: tt.to_string(),
            expected: name.to_string(),
        })
    }
}

fn assert_tt_punct(maybe_tt: Option<TokenTree>) -> Result<proc_macro2::Punct, Error> {
    let tt = maybe_tt.ok_or_else(|| Error::NotEnoughTokens)?;
    if let TokenTree::Punct(p) = tt {
        Ok(p)
    } else {
        Err(Error::IncorrectExpectedTokenTreeVariant {
            found: tt.to_string(),
            expected: "punctuation",
        })
    }
}

fn assert_tt_punct_named(
    maybe_tt: Option<TokenTree>,
    name: char,
) -> Result<proc_macro2::Punct, Error> {
    let tt = maybe_tt.ok_or_else(|| Error::NotEnoughTokens)?;
    if let TokenTree::Punct(p) = tt {
        if p.as_char() == name {
            Ok(p)
        } else {
            Err(Error::IncorrectExpectedTokenContent {
                found: p.as_char().to_string(),
                expected: name.to_string(),
            })
        }
    } else {
        Err(Error::IncorrectExpectedTokenTreeVariant {
            found: tt.to_string(),
            expected: "punctuation",
        })
    }
}

#[derive(Debug)]
enum Error {
    NotEnoughTokens,
    IncorrectExpectedTokenTreeVariant {
        found: String,
        expected: &'static str,
    },
    IncorrectExpectedTokenContent {
        found: String,
        expected: String,
    },
    InvalidResourceKind {
        found: String,
    },
    MissingRequiredResourceKind {
        msg: String,
    },
    BlockParse(SynError),
}

impl From<SynError> for Error {
    fn from(se: SynError) -> Self {
        Error::BlockParse(se)
    }
}

fn parse_header(ts2: TokenStream2) -> Result<(Header, TokenStream2), Error> {
    let mut tok_iter = ts2.into_iter();
    let _ = assert_tt_punct_named(tok_iter.next(), '|')?; // open-delim
    let (first_resource, shall_we_continue) = parse_intermediate_resource(&mut tok_iter)?;
    let optional_resources =
        if ResourceParseContinuation::ExpectAnotherResource == shall_we_continue {
            let (second_resource, shall_we_continue) = parse_intermediate_resource(&mut tok_iter)?;
            if ResourceParseContinuation::ExpectAnotherResource == shall_we_continue {
                let (third_resource, _) = parse_intermediate_resource(&mut tok_iter)?;
                (Some(second_resource), Some(third_resource))
            } else {
                (Some(second_resource), None)
            }
        } else {
            (None, None)
        };

    Ok((
        header_from_resources(first_resource, optional_resources)?,
        tok_iter.collect(),
    ))
}

fn header_from_resources(
    first: IntermediateResource,
    optional_resources: (Option<IntermediateResource>, Option<IntermediateResource>),
) -> Result<Header, Error> {
    match optional_resources {
        (None, None) => Header::from_single_resource(first),
        (Some(second), None) => Header::from_resource_pair(first, second),
        (Some(second), Some(third)) => {
            match (first.kind.as_ref(), second.kind.as_ref(), third.kind.as_ref()) {
                (None, None, None) => Header::from_known_triple(first, second, third),
                (Some(ResKind::CNodeSlots), Some(ResKind::Untyped), Some(ResKind::AddressRange)) => Header::from_known_triple(first, second, third),
                (Some(ResKind::CNodeSlots), Some(ResKind::AddressRange), Some(ResKind::Untyped)) => Header::from_known_triple(first, third, second),
                (Some(ResKind::Untyped), Some(ResKind::CNodeSlots), Some(ResKind::AddressRange)) => Header::from_known_triple(second, first, third),
                (Some(ResKind::AddressRange), Some(ResKind::CNodeSlots), Some(ResKind::Untyped)) => Header::from_known_triple(second, third, first),
                (Some(ResKind::AddressRange), Some(ResKind::Untyped), Some(ResKind::CNodeSlots)) => Header::from_known_triple(third, second, first),
                (Some(ResKind::Untyped), Some(ResKind::AddressRange), Some(ResKind::CNodeSlots)) => Header::from_known_triple(third, first, second),
                _ => panic!("{}\nbut when there are three resources to allocate from, their kinds must either be entirely positionally determined ({}, {}, {}) or entirely explicit and unique", EXPECTED_LAYOUT_MESSAGE, RESOURCE_TYPE_HINT_CSLOTS, RESOURCE_TYPE_HINT_UNTYPED, RESOURCE_TYPE_HINT_ADDR)

            }
        }
        (None, Some(_)) => unreachable!(),
    }
}

#[derive(PartialEq)]
enum ResourceParseContinuation {
    DoneWithAllResources,
    ExpectAnotherResource,
}

impl From<char> for ResourceParseContinuation {
    fn from(c: char) -> Self {
        match c {
            '|'=> ResourceParseContinuation::DoneWithAllResources,
            ',' => ResourceParseContinuation::ExpectAnotherResource,
            _ => panic!("{}\nbut after a resource declaration, punctuation other than '|' or ',' was found {}", EXPECTED_LAYOUT_MESSAGE, c),
        }
    }
}

fn parse_intermediate_resource(
    tok_iter: &mut impl Iterator<Item = TokenTree>,
) -> Result<(IntermediateResource, ResourceParseContinuation), Error> {
    let request_id = assert_tt_ident(tok_iter.next())?;
    let _ = assert_tt_ident_named(tok_iter.next(), "from")?;
    let resource_id = assert_tt_ident(tok_iter.next())?;

    match tok_iter.next().expect(EXPECTED_LAYOUT_MESSAGE) {
        TokenTree::Group(_) => panic!(EXPECTED_LAYOUT_MESSAGE),
        TokenTree::Ident(_) => panic!(RESOURCE_DECLARATION_LAYOUT_MESSAGE),
        TokenTree::Punct(p) => match p.as_char() {
            '|' | ',' => Ok((
                IntermediateResource {
                    resource_id,
                    request_id,
                    kind: None,
                },
                p.as_char().into(),
            )),
            ':' => {
                let k = parse_resource_kind(tok_iter)?;
                Ok((
                    IntermediateResource {
                        resource_id,
                        request_id,
                        kind: Some(k),
                    },
                    assert_tt_punct(tok_iter.next())?.as_char().into(),
                ))
            }
            _ => panic!(EXPECTED_LAYOUT_MESSAGE),
        },
        TokenTree::Literal(_) => panic!(EXPECTED_LAYOUT_MESSAGE),
    }
}

fn parse_resource_kind(tok_iter: &mut impl Iterator<Item = TokenTree>) -> Result<ResKind, Error> {
    let raw_kind = assert_tt_ident(tok_iter.next())?.to_string();
    match raw_kind.as_ref() {
        RESOURCE_TYPE_HINT_CSLOTS => Ok(ResKind::CNodeSlots),
        RESOURCE_TYPE_HINT_UNTYPED => Ok(ResKind::Untyped),
        RESOURCE_TYPE_HINT_ADDR => Ok(ResKind::AddressRange),
        _ => Err(Error::InvalidResourceKind { found: raw_kind }),
    }
}

struct IntermediateResource {
    resource_id: Ident,
    request_id: Ident,
    kind: Option<ResKind>,
}

#[derive(PartialEq)]
enum ResKind {
    CNodeSlots,
    Untyped,
    AddressRange,
}

#[derive(Debug, PartialEq)]
struct Header {
    pub(crate) cnode_slots: ResourceRequest,
    pub(crate) untypeds: Option<ResourceRequest>,
    pub(crate) address_ranges: Option<ResourceRequest>,
}

struct ResolvedResourceIds {
    cslots_resource: Ident,
    untyped_resource: Ident,
    address_resource: Ident,
}

impl ResolvedResourceIds {
    fn resolve(
        header: &Header,
        planned_allocs: &[PlannedAlloc],
    ) -> Result<ResolvedResourceIds, String> {
        let cslots_resource = Ident::new(
            &header.cnode_slots.resource_id.to_string(),
            header.cnode_slots.resource_id.span(),
        );
        if planned_allocs.iter().any(|p| match p {
            PlannedAlloc::Untyped { .. } => true,
            _ => false,
        }) && header.untypeds == None
        {
            return Err(format!("{}\nbut untyped allocations were requested and the {} resource not provided to smart_alloc", EXPECTED_LAYOUT_MESSAGE, RESOURCE_TYPE_HINT_UNTYPED));
        }
        let untyped_resource = header
            .untypeds
            .as_ref()
            .map(|ut_rr| Ident::new(&ut_rr.resource_id.to_string(), ut_rr.resource_id.span()))
            .unwrap_or_else(|| {
                Ident::new("untyped_buddy_not_provided_to_macro", Span::call_site())
            });
        if planned_allocs.iter().any(|p| match p {
            PlannedAlloc::AddressRange { .. } => true,
            _ => false,
        }) && header.address_ranges == None
        {
            return Err(format!("{}\nbut address allocations were requested and the {} resource not provided to smart_alloc", EXPECTED_LAYOUT_MESSAGE, RESOURCE_TYPE_HINT_ADDR));
        }
        let address_resource = header
            .address_ranges
            .as_ref()
            .map(|ar_rr| Ident::new(&ar_rr.resource_id.to_string(), ar_rr.resource_id.span()))
            .unwrap_or_else(|| {
                Ident::new("address_buddy_not_provided_to_macro", Span::call_site())
            });
        Ok(ResolvedResourceIds {
            cslots_resource,
            untyped_resource,
            address_resource,
        })
    }
}

impl Header {
    fn from_single_resource(first: IntermediateResource) -> Result<Header, Error> {
        // There is only one resource defined, so it better be a cnode slots resource
        match first.kind {
            None => (),
            Some(ResKind::CNodeSlots) => (),
            _ => {
                return Err(Error::MissingRequiredResourceKind {
                    msg: format!(
                        "The only resource declared was not the required kind, {}",
                        RESOURCE_TYPE_HINT_CSLOTS
                    ),
                })
            }
        };
        Ok(Header {
            cnode_slots: ResourceRequest {
                resource_id: first.resource_id,
                request_id: first.request_id,
            },
            untypeds: None,
            address_ranges: None,
        })
    }

    fn from_resource_pair(
        first: IntermediateResource,
        second: IntermediateResource,
    ) -> Result<Header, Error> {
        let error = format!("Addresses can only be smart-allocated with access to {}, an {}, and an {}, but only two such resources were supplied", RESOURCE_TYPE_HINT_CSLOTS, RESOURCE_TYPE_HINT_UNTYPED, RESOURCE_TYPE_HINT_ADDR);
        match (first.kind.as_ref(), second.kind.as_ref()) {
            (None, None) => Header::from_known_kinds_resource_pair(first, second),
            (Some(fk), None) => match fk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(first, second),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(second, first),
                ResKind::AddressRange => Err(Error::MissingRequiredResourceKind { msg: error }),
            },
            (None, Some(sk)) => match sk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(second, first),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(first, second),
                ResKind::AddressRange => Err(Error::MissingRequiredResourceKind { msg: error }),
            },
            (Some(fk), Some(_sk)) => match fk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(first, second),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(second, first),
                ResKind::AddressRange => Err(Error::MissingRequiredResourceKind { msg: error }),
            },
        }
    }

    fn from_known_kinds_resource_pair(
        cnode_slots: IntermediateResource,
        untypeds: IntermediateResource,
    ) -> Result<Header, Error> {
        // TODO - better error message here
        if !(cnode_slots.kind == None || cnode_slots.kind == Some(ResKind::CNodeSlots)) {
            return Err(Error::MissingRequiredResourceKind {
                msg: EXPECTED_LAYOUT_MESSAGE.to_string(),
            });
        };
        if !(untypeds.kind == None || untypeds.kind == Some(ResKind::Untyped)) {
            return Err(Error::MissingRequiredResourceKind {
                msg: EXPECTED_LAYOUT_MESSAGE.to_string(),
            });
        };
        Ok(Header {
            cnode_slots: ResourceRequest {
                resource_id: cnode_slots.resource_id,
                request_id: cnode_slots.request_id,
            },
            untypeds: Some(ResourceRequest {
                resource_id: untypeds.resource_id,
                request_id: untypeds.request_id,
            }),
            address_ranges: None,
        })
    }
    fn from_known_triple(
        cnode_slots: IntermediateResource,
        untypeds: IntermediateResource,
        addrs: IntermediateResource,
    ) -> Result<Header, Error> {
        // TODO - convert to error-return
        assert!(
            cnode_slots.kind == None || cnode_slots.kind == Some(ResKind::CNodeSlots),
            EXPECTED_LAYOUT_MESSAGE
        );
        assert!(
            untypeds.kind == None || untypeds.kind == Some(ResKind::Untyped),
            EXPECTED_LAYOUT_MESSAGE
        );
        assert!(
            addrs.kind == None || addrs.kind == Some(ResKind::AddressRange),
            EXPECTED_LAYOUT_MESSAGE
        );
        Ok(Header {
            cnode_slots: ResourceRequest {
                resource_id: cnode_slots.resource_id,
                request_id: cnode_slots.request_id,
            },
            untypeds: Some(ResourceRequest {
                resource_id: untypeds.resource_id,
                request_id: untypeds.request_id,
            }),
            address_ranges: Some(ResourceRequest {
                resource_id: addrs.resource_id,
                request_id: addrs.request_id,
            }),
        })
    }
}

#[derive(Debug, PartialEq)]
struct ResourceRequest {
    pub(crate) resource_id: proc_macro2::Ident,
    pub(crate) request_id: proc_macro2::Ident,
}

struct IdTracker {
    cslot_request_id: Ident,
    untyped_request_id: Option<Ident>,
    address_request_id: Option<Ident>,
    planned_allocs: Vec<PlannedAlloc>,
}

enum PlannedAlloc {
    CSlot(Ident),
    Untyped {
        ut: Ident,
        cslot: Ident,
    },
    AddressRange {
        addr: Ident,
        cslot_for_addr: Ident,
        ut: Ident,
        cslot_for_ut: Ident,
    },
}

impl From<&Header> for IdTracker {
    fn from(h: &Header) -> Self {
        IdTracker::new(
            Ident::new(&h.cnode_slots.request_id.to_string(), Span::call_site()),
            h.untypeds
                .as_ref()
                .map(|rr| Ident::new(&rr.request_id.to_string(), Span::call_site())),
            h.address_ranges
                .as_ref()
                .map(|rr| Ident::new(&rr.request_id.to_string(), Span::call_site())),
        )
    }
}

impl IdTracker {
    fn new(
        cslot_request_id: Ident,
        untyped_request_id: Option<Ident>,
        address_request_id: Option<Ident>,
    ) -> Self {
        IdTracker {
            cslot_request_id,
            untyped_request_id,
            address_request_id,
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
        if node == self.cslot_request_id {
            let fresh_id = make_ident(Uuid::new_v4(), "cslots");
            self.planned_allocs
                .push(PlannedAlloc::CSlot(fresh_id.clone()));
            return fresh_id;
        }

        if let Some(ut_request_id) = &self.untyped_request_id {
            if node == *ut_request_id {
                let fresh_id = make_ident(Uuid::new_v4(), "untyped");
                self.planned_allocs.push(PlannedAlloc::Untyped {
                    ut: fresh_id.clone(),
                    cslot: make_ident(Uuid::new_v4(), "cslots_for_untyped"),
                });
                return fresh_id;
            }
        }

        if let Some(addr_request_id) = &self.address_request_id {
            if node == *addr_request_id {
                let fresh_id = make_ident(Uuid::new_v4(), "address_range");
                self.planned_allocs.push(PlannedAlloc::AddressRange {
                    addr: fresh_id.clone(),
                    cslot_for_addr: make_ident(Uuid::new_v4(), "cslots_for_address_range"),
                    ut: make_ident(Uuid::new_v4(), "untyped_for_address_range"),
                    cslot_for_ut: make_ident(
                        Uuid::new_v4(),
                        "cslots_for_untyped_for_address_range",
                    ),
                });
                return fresh_id;
            }
        }

        node
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_header, Error};
    use std::str::FromStr;
    use syn::export::TokenStream2;

    #[test]
    fn happy_path_parse_header_no_remainder() -> Result<(), Error> {
        let ts2: TokenStream2 = TokenStream2::from_str("| cs from cslots |").unwrap();
        let (h, remaining) = parse_header(ts2)?;
        match remaining.into_iter().next() {
            Some(_) => panic!("Expected the remainder to be empty"),
            _ => (),
        };
        assert_eq!("cs", h.cnode_slots.request_id.to_string());
        assert_eq!("cslots", h.cnode_slots.resource_id.to_string());
        assert_eq!(None, h.untypeds);
        assert_eq!(None, h.address_ranges);
        Ok(())
    }

    #[test]
    fn untyped_only_header_is_an_error() {
        let ts2: TokenStream2 = TokenStream2::from_str("| ut from uts: UntypedBuddy |").unwrap();
        match parse_header(ts2) {
            Ok(_) => panic!("Expected an error"),
            Err(_) => (), // TODO - precise error kind
        }
    }
}
