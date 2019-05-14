use super::Error;
use syn::spanned::Spanned;
use syn::{FnArg, GenericArgument, Ident, Pat, PathArguments, PathSegment, Type, TypePath};

pub(crate) fn extract_expected_resources<'a>(
    args: impl IntoIterator<Item = &'a FnArg>,
) -> Result<Vec<Param>, Error> {
    args.into_iter().map(parse_param).collect()
}

fn parse_param(arg: &FnArg) -> Result<Param, Error> {
    const SIMPLE_ARGUMENTS_ONLY: &str =
        "test function arguments must be of explicit format `identifier: Type`";
    let ac = if let FnArg::Captured(ac) = &arg {
        ac
    } else {
        return Err(Error::InvalidArgumentType {
            msg: SIMPLE_ARGUMENTS_ONLY.to_string(),
            span: arg.span(),
        });
    };
    let ident = match &ac.pat {
        Pat::Ident(pi) if pi.by_ref.is_none() && pi.subpat.is_none() => pi.ident.clone(),
        _ => {
            return Err(Error::InvalidArgumentType {
                msg: SIMPLE_ARGUMENTS_ONLY.to_string(),
                span: arg.span(),
            })
        }
    };
    let kind = match &ac.ty {
        Type::Path(type_path) => parse_param_kind(type_path)?,
        // TODO - optional convenience to support passing references as well as fully owned instances
        // Type::Reference(type_reference) => unimplemented!(),
        _ => {
            return Err(Error::InvalidArgumentType {
                msg: SIMPLE_ARGUMENTS_ONLY.to_string(),
                span: arg.span(),
            })
        }
    };
    Ok(Param {
        original_ident: ident,
        kind,
    })
}

fn parse_param_kind(type_path: &TypePath) -> Result<ParamKind, Error> {
    let segment = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| Error::InvalidArgumentType {
            msg: "test function argument must not be empty".to_string(),
            span: type_path.span(),
        })?
        .into_value();
    let kind = match segment.ident.to_string().as_ref() {
        "LocalCNodeSlots" => ParamKind::CNodeSlots {
            count: extract_first_argument_as_unsigned(&segment.arguments)?,
        },
        "LocalCap" => parse_localcap_param_kind(&segment.arguments)?,
        "UserImage" => {
            let seg_name = extract_first_arg_type_path_last_segment(&segment.arguments)?
                .ident
                .to_string();
            if &seg_name == "Local" {
                ParamKind::UserImage
            } else {
                return Err(Error::InvalidArgumentType {
                    msg: "The only supported test function variant of UserImage is UserImage<ferros::userland::role::Local>".to_string(),
                    span: segment.span(),
                });
            }
        }
        // TODO - as a convenience, support CNodeSlots<Size, role::Local>
        "CNodeSlots" => {
            // TODO - enforce that Role must be local, CNodeSlots<Size, Role>
            ParamKind::CNodeSlots {
                count: extract_first_argument_as_unsigned(&segment.arguments)?,
            }
        }
        // TODO - as a convenience, support Cap<T, role::Local>
        "Cap" => unimplemented!(),
        t => {
            return Err(Error::InvalidArgumentType {
                msg: format!("test function argument type was not recognized: {}", t),
                span: segment.span(),
            })
        }
    };
    Ok(kind)
}
fn parse_localcap_param_kind(arguments: &PathArguments) -> Result<ParamKind, Error> {
    let segment = extract_first_arg_type_path_last_segment(arguments)?;
    let type_name = segment.ident.to_string();
    match type_name.as_ref() {
        "Untyped" => Ok(ParamKind::Untyped {
            bits: extract_first_argument_as_unsigned(&segment.arguments)?,
        }),
        "ASIDPool" => Ok(ParamKind::ASIDPool {
            count: extract_first_argument_as_unsigned(&segment.arguments)?,
        }),
        "ThreadPriorityAuthority" => Ok(ParamKind::ThreadPriorityAuthority),
        "LocalCNode" => Ok(ParamKind::CNode),
        // TODO - expand the set of convenience aliases
        // "CNode" => unimplemented!(),
        // "CNodeSlotsData" => unimplemented!(),
        _ => Err(Error::InvalidArgumentType {
            msg: format!(
                "Found an unsupported LocalCap type parameter, {}",
                &type_name
            ),
            span: arguments.span(),
        }),
    }
}

/// Given PathArguments like `<a::b::T<Foo>, U, V>`, extracts `T<Foo>`
fn extract_first_arg_type_path_last_segment(
    arguments: &PathArguments,
) -> Result<&PathSegment, Error> {
    // TODO - consider iterating on the error message usefulness
    const EXPECTED: &str =
        "Expected a ferros type argument (e.g. `Unsigned<U5>`, `ASIDPool<U1024>`)";
    if let PathArguments::AngleBracketed(abga) = arguments {
        let gen_arg = abga
            .args
            .first()
            .ok_or_else(|| Error::InvalidArgumentType {
                msg: "test function argument's generic parameter must not be empty".to_string(),
                span: abga.span(),
            })?
            .into_value();
        if let GenericArgument::Type(Type::Path(type_path)) = gen_arg {
            Ok(type_path
                .path
                .segments
                .last()
                .ok_or_else(|| Error::InvalidArgumentType {
                    msg: "test function argument's generic parameter must not be empty".to_string(),
                    span: type_path.span(),
                })?
                .into_value())
        } else {
            Err(Error::InvalidArgumentType {
                msg: EXPECTED.to_string(),
                span: arguments.span(),
            })
        }
    } else {
        Err(Error::InvalidArgumentType {
            msg: EXPECTED.to_string(),
            span: arguments.span(),
        })
    }
}

/// Meant to take PathArguments like <typenum::U5> and turn it into 5usize
fn extract_first_argument_as_unsigned(arguments: &PathArguments) -> Result<usize, Error> {
    let segment = extract_first_arg_type_path_last_segment(arguments)?;
    parse_typenum_unsigned(&segment.ident)
}

/// Parse "U5" into `5usize`
fn parse_typenum_unsigned(ident: &Ident) -> Result<usize, Error> {
    let err_fn = || {
        Error::InvalidArgumentType {
        msg: "Could not parse type argument as a typenum unsigned type-level literal (e.g. U5, U1024)".to_string(),
        span: ident.span(),
    }
    };
    let mut s = ident.to_string();
    if s.len() < 2 {
        return Err(err_fn());
    }
    let unsigned = s.split_off(1);
    if &s != "U" {
        return Err(err_fn());
    }
    unsigned.parse().map_err(|_e| err_fn())
}
pub(crate) struct Param {
    original_ident: Ident,
    kind: ParamKind,
}

pub(crate) enum ParamKind {
    CNodeSlots { count: usize },
    Untyped { bits: usize },
    ASIDPool { count: usize },
    CNode,
    ThreadPriorityAuthority,
    UserImage,
}
