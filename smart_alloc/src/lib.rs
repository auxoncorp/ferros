extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::TokenStreamExt;
use std::fmt::{Display, Formatter};
use syn::export::TokenStream2;
use syn::fold::Fold;
use syn::parse_macro_input::parse as syn_parse;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    parse_quote, ArgCaptured, Block, Error as SynError, Expr, ExprClosure, FnArg, GenericArgument,
    Ident, Item as SynItem, ItemMacro, Pat, PathArguments, ReturnType, Stmt, Type,
};
use uuid::Uuid;

const RESOURCE_TYPE_HINT_CSLOTS: &str = "CNodeSlots";
const RESOURCE_TYPE_HINT_UNTYPED: &str = "UntypedBuddy";

const EXPECTED_LAYOUT_MESSAGE: &str = r"smart_alloc expects to be invoked like:
smart_alloc!(  |cs: cslots, ut: untypeds| {
    let id_that_will_leak = something_requiring_slots(cs);
    op_requiring_memory(ut);
    top_fn(cs, nested_fn(cs, ut));
});

When a resource is declared in the closure-signature-like header,
it should be in one of the following forms:
* `request_id: resource_id`
* `request_id: resource_id<ResourceKind>`

where ResourceKind is one of CNodeSlots or UntypedBuddy.";

#[proc_macro]
pub fn smart_alloc(tokens: TokenStream) -> TokenStream {
    smart_alloc_impl(TokenStream2::from(tokens))
        .unwrap_or_else(|e| panic!("{}", e))
        .into()
}

fn smart_alloc_impl(tokens: TokenStream2) -> Result<TokenStream2, Error> {
    let mut output_tokens = TokenStream2::new();
    output_tokens.append_all(smart_alloc_structured(tokens)?);
    Ok(output_tokens)
}

fn smart_alloc_structured(tokens: TokenStream2) -> Result<Vec<Stmt>, Error> {
    let closure: ExprClosure = syn_parse(tokens.into())?;
    if closure.output != ReturnType::Default {
        return Err(Error::NoReturnTypeAllowed);
    }
    let header = parse_closure_header(&closure.inputs)?;
    let mut block = match *closure.body {
        Expr::Block(expr_block) => expr_block.block,
        _ => return Err(Error::ContentMustBeABlock),
    };

    // Find all requests for allocations, replace the relevant target-ids with unique ids,
    // and construct an allocation plan for each site for later code generation
    let mut id_tracker = IdTracker::from(&header);
    block = id_tracker.fold_block(block);

    let resource_ids = ResolvedResourceIds::resolve(&header, &id_tracker.planned_allocs).unwrap();
    let mut output_stmts = materialize_alloc_statements(&id_tracker.planned_allocs, resource_ids);
    let user_statements = block.stmts;
    output_stmts.extend(user_statements);
    Ok(output_stmts)
}

fn materialize_alloc_statements(
    planned_allocs: &[PlannedAlloc],
    resource_ids: ResolvedResourceIds,
) -> Vec<Stmt> {
    let ResolvedResourceIds {
        cslots_resource,
        untyped_resource,
    } = resource_ids;
    let mut output_stmts = Vec::new();
    for plan in planned_allocs {
        match plan {
            PlannedAlloc::CSlot(id) => {
                let alloc_cslot: Stmt = parse_quote! {
                    let (#id, #cslots_resource) = #cslots_resource.alloc();
                };
                output_stmts.push(alloc_cslot);
            }
            PlannedAlloc::Untyped { ut, cslot } => {
                let alloc_both: Block = parse_quote! { {
                    let (#cslot, #cslots_resource) = #cslots_resource.alloc();
                    let (#ut, #untyped_resource) = #untyped_resource.alloc(#cslot)?;
                }};
                output_stmts.extend(alloc_both.stmts);
            }
        }
    }
    output_stmts
}

#[derive(Debug)]
enum Error {
    NoReturnTypeAllowed,
    ContentMustBeABlock,
    InvalidResourceFormat { msg: String },
    InvalidResourceKind { found: String },
    MissingRequiredResourceKind { msg: String },
    AmbiguousResourceId { id: String },
    AmbiguousRequestId { id: String },
    InvalidRequestId { id: String },
    MissingResourceId { request_id: String },
    TooManyResources,
    SynParse(SynError),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        let s = match self {
            Error::NoReturnTypeAllowed => format!(
                "{}\nbut a spurious return type was supplied for the closure-like header",
                EXPECTED_LAYOUT_MESSAGE
            ),
            Error::ContentMustBeABlock => format!(
                "{}\nbut the contents following the closure-like header were not a block",
                EXPECTED_LAYOUT_MESSAGE
            ),
            Error::InvalidResourceFormat { msg } => {
                format!("{}\nbut {}", EXPECTED_LAYOUT_MESSAGE, msg)
            }
            Error::InvalidResourceKind { found } => format!(
                "{}\nbut an invalid resource kind {} was found",
                EXPECTED_LAYOUT_MESSAGE, found
            ),
            Error::MissingRequiredResourceKind { msg } => format!(
                "{}\nbut a required resource kind was missing: {}",
                EXPECTED_LAYOUT_MESSAGE, msg
            ),
            Error::AmbiguousResourceId { id } => format!(
                "{}\nbut an ambiguous resource id was found: {}",
                EXPECTED_LAYOUT_MESSAGE, id
            ),
            Error::AmbiguousRequestId { id } => format!(
                "{}\nbut an ambiguous alloc request id was found: {}",
                EXPECTED_LAYOUT_MESSAGE, id
            ),
            Error::InvalidRequestId { id } => format!(
                "{}\nbut an invalid request id in the signature was found: {}",
                EXPECTED_LAYOUT_MESSAGE, id
            ),
            Error::MissingResourceId { request_id } => format!(
                "{}\nbut no resource was supplied for request id {}",
                EXPECTED_LAYOUT_MESSAGE, request_id
            ),
            Error::TooManyResources => format!(
                "{}\nbut more than two resources without explicit resource kinds were requested",
                EXPECTED_LAYOUT_MESSAGE
            ),
            Error::SynParse(se) => se.to_compile_error().to_string(),
        };
        f.write_str(&s)
    }
}

impl From<SynError> for Error {
    fn from(se: SynError) -> Self {
        Error::SynParse(se)
    }
}

fn parse_closure_header(inputs: &Punctuated<FnArg, Comma>) -> Result<Header, Error> {
    let mut intermediates = Vec::new();
    for arg in inputs.iter() {
        intermediates.push(parse_fnarg_to_intermediate(arg)?);
    }
    header_from_intermediate_resources(&intermediates)
}

fn parse_fnarg_to_intermediate(arg: &FnArg) -> Result<IntermediateResource, Error> {
    let (request_pat, resource_ty) = match arg {
        FnArg::Captured(ArgCaptured { pat, ty, .. }) => (pat, ty),
        FnArg::Inferred(inf) => {
            return Err(Error::MissingResourceId {
                request_id: format!("{:?}", inf),
            })
        }
        FnArg::SelfRef(_) | FnArg::SelfValue(_) => {
            return Err(Error::InvalidRequestId {
                id: "self".to_string(),
            })
        }
        FnArg::Ignored(_) => {
            return Err(Error::InvalidRequestId {
                id: "_".to_string(),
            })
        }
    };

    let request_id = {
        if let Pat::Ident(pi) = request_pat {
            pi.ident.clone()
        } else {
            return Err(Error::InvalidRequestId {
                id: format!("{:?}", request_pat),
            });
        }
    };

    let (resource_id, res_kind_id) = {
        if let Type::Path(tp) = resource_ty {
            let seg = tp
                .path
                .segments
                .last()
                .ok_or_else(|| Error::InvalidResourceFormat {
                    msg: format!(
                        "{:?} was associated with an nonexistent resource name",
                        request_id
                    ),
                })?
                .into_value();
            if seg.arguments.is_empty() {
                (seg.ident.clone(), None)
            } else if let PathArguments::AngleBracketed(abga) = &seg.arguments {
                let gen_arg = abga
                    .args
                    .first()
                    .ok_or_else(|| Error::InvalidResourceFormat {
                        msg: format!(
                            "{:?} was associated with a resource with an empty resource kind",
                            request_id
                        ),
                    })?
                    .into_value();
                if let GenericArgument::Type(Type::Path(res_kind_ty)) = gen_arg {
                    let res_kind_seg = res_kind_ty.path.segments.last().ok_or_else(|| Error::InvalidResourceFormat {  msg: format!("{:?} was associated with a resource with a resource kind with a nonexistent name", request_id )})?.into_value();
                    (seg.ident.clone(), Some(res_kind_seg.ident.clone()))
                } else {
                    return Err(Error::InvalidResourceFormat {
                        msg: format!(
                            "{:?} was associated with a complex invalid resource kind",
                            request_id
                        ),
                    });
                }
            } else {
                return Err(Error::InvalidResourceFormat {
                    msg: format!(
                        "{:?} was associated with a non-angle-bracketed resource kind",
                        request_id
                    ),
                });
            }
        } else {
            return Err(Error::InvalidResourceFormat {
                msg: format!(
                    "{:?} was not associated with a simple type-like resource id",
                    request_id
                ),
            });
        }
    };

    let kind = match res_kind_id {
        Some(k) => Some(parse_resource_kind(k)?),
        None => None,
    };

    Ok(IntermediateResource {
        resource_id,
        request_id,
        kind,
    })
}

fn header_from_intermediate_resources(resources: &[IntermediateResource]) -> Result<Header, Error> {
    match resources.len() {
        0 => Err(Error::MissingRequiredResourceKind {
            msg: RESOURCE_TYPE_HINT_CSLOTS.to_string(),
        }),
        1 => Header::from_single_resource(&resources[0]),
        2 => Header::from_resource_pair(&resources[0], &resources[1]),
        _ => Err(Error::TooManyResources),
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

fn parse_resource_kind(ident: Ident) -> Result<ResKind, Error> {
    let raw_kind = ident.to_string();
    match raw_kind.as_ref() {
        RESOURCE_TYPE_HINT_CSLOTS => Ok(ResKind::CNodeSlots),
        RESOURCE_TYPE_HINT_UNTYPED => Ok(ResKind::Untyped),
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
}

#[derive(Debug, PartialEq)]
struct Header {
    pub(crate) cnode_slots: ResourceRequest,
    pub(crate) untypeds: Option<ResourceRequest>,
}

struct ResolvedResourceIds {
    cslots_resource: Ident,
    untyped_resource: Ident,
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
        Ok(ResolvedResourceIds {
            cslots_resource,
            untyped_resource,
        })
    }
}

impl Header {
    fn from_single_resource(first: &IntermediateResource) -> Result<Header, Error> {
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
                resource_id: first.resource_id.clone(),
                request_id: first.request_id.clone(),
            },
            untypeds: None,
        })
    }

    fn from_resource_pair(
        first: &IntermediateResource,
        second: &IntermediateResource,
    ) -> Result<Header, Error> {
        match (first.kind.as_ref(), second.kind.as_ref()) {
            (None, None) => Header::from_known_kinds_resource_pair(first, second),
            (Some(fk), None) => match fk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(first, second),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(second, first),
            },
            (None, Some(sk)) => match sk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(second, first),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(first, second),
            },
            (Some(fk), Some(_sk)) => match fk {
                ResKind::CNodeSlots => Header::from_known_kinds_resource_pair(first, second),
                ResKind::Untyped => Header::from_known_kinds_resource_pair(second, first),
            },
        }
    }

    fn from_known_kinds_resource_pair(
        cnode_slots: &IntermediateResource,
        untypeds: &IntermediateResource,
    ) -> Result<Header, Error> {
        // Check for duplicates
        if cnode_slots.resource_id == untypeds.resource_id {
            return Err(Error::AmbiguousResourceId {
                id: cnode_slots.resource_id.to_string(),
            });
        }
        if cnode_slots.request_id == untypeds.request_id {
            return Err(Error::AmbiguousRequestId {
                id: cnode_slots.request_id.to_string(),
            });
        }

        // Confirm assumptions about resource kinds
        if !(cnode_slots.kind == None || cnode_slots.kind == Some(ResKind::CNodeSlots)) {
            return Err(Error::MissingRequiredResourceKind {
                msg: RESOURCE_TYPE_HINT_CSLOTS.to_string(),
            });
        };
        if !(untypeds.kind == None || untypeds.kind == Some(ResKind::Untyped)) {
            return Err(Error::MissingRequiredResourceKind {
                msg: RESOURCE_TYPE_HINT_UNTYPED.to_string(),
            });
        };
        Ok(Header {
            cnode_slots: ResourceRequest {
                resource_id: cnode_slots.resource_id.clone(),
                request_id: cnode_slots.request_id.clone(),
            },
            untypeds: Some(ResourceRequest {
                resource_id: untypeds.resource_id.clone(),
                request_id: untypeds.request_id.clone(),
            }),
        })
    }
}

#[derive(Debug, PartialEq)]
struct ResourceRequest {
    pub(crate) resource_id: syn::Ident,
    pub(crate) request_id: syn::Ident,
}

struct IdTracker {
    cslot_request_id: Ident,
    untyped_request_id: Option<Ident>,
    planned_allocs: Vec<PlannedAlloc>,
}

enum PlannedAlloc {
    CSlot(Ident),
    Untyped { ut: Ident, cslot: Ident },
}

impl From<&Header> for IdTracker {
    fn from(h: &Header) -> Self {
        IdTracker::new(
            Ident::new(&h.cnode_slots.request_id.to_string(), Span::call_site()),
            h.untypeds
                .as_ref()
                .map(|rr| Ident::new(&rr.request_id.to_string(), Span::call_site())),
        )
    }
}

impl IdTracker {
    fn new(cslot_request_id: Ident, untyped_request_id: Option<Ident>) -> Self {
        IdTracker {
            cslot_request_id,
            untyped_request_id,
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
    fn fold_block(&mut self, i: Block) -> Block {
        let brace_token = i.brace_token;
        let visitor = self;
        let expand = |st: Stmt| -> Vec<Stmt> {
            match st {
                s @ Stmt::Local(_) => vec![visitor.fold_stmt(s)],
                s @ Stmt::Expr(_) => vec![visitor.fold_stmt(s)],
                s @ Stmt::Semi(_, _) => vec![visitor.fold_stmt(s)],
                Stmt::Item(i) => {
                    if let SynItem::Macro(item_macro) = i {
                        // Manually invoke nested smart_alloc invocations
                        // rather than relying on normal Rust compiler detection
                        // in order to run id-replacement processing on their outputs
                        // so that we can support the use of higher-level allocation requests
                        // in nested contexts.
                        if macro_name_matches(&item_macro, "smart_alloc") {
                            // TODO - consider passing down pre-existing header-defined request ids
                            // in order to detect and error out in the case of accidental shadowing
                            let nested_stmts = smart_alloc_structured(item_macro.mac.tts).unwrap();
                            let mut out_stmts = Vec::new();
                            for stmt in nested_stmts {
                                out_stmts.push(visitor.fold_stmt(stmt));
                            }
                            out_stmts
                        } else {
                            vec![visitor.fold_stmt(Stmt::Item(SynItem::Macro(item_macro)))]
                        }
                    } else {
                        vec![visitor.fold_stmt(Stmt::Item(i))]
                    }
                }
            }
        };
        let stmts = i.stmts.into_iter().map(expand).flatten().collect();
        Block { brace_token, stmts }
    }

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
        node
    }
}

fn macro_name_matches(item_macro: &ItemMacro, target: &'static str) -> bool {
    if let Some(end) = item_macro.mac.path.segments.last() {
        end.value().ident == Ident::new(target, Span::call_site())
    } else {
        false
    }
}
