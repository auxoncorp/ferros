use super::ParseError;
use crate::model::*;
use proc_macro2::TokenStream as TokenStream2;
use syn::spanned::Spanned;
use syn::{
    FnArg, GenericArgument, Ident, Pat, PathArguments, PathSegment, ReturnType, Type, TypePath,
};

impl SynContent {
    pub(crate) fn parse(attr: TokenStream2, item: TokenStream2) -> Result<Self, ParseError> {
        let attr_span = attr.span();
        let context_attr = syn::parse_macro_input::parse(attr.into())
            .map_err(|_e| ParseError::InvalidTestAttribute { span: attr_span })?;
        let item_span = item.span();
        let fn_under_test = syn::parse_macro_input::parse(item.into())
            .map_err(|_e| ParseError::InvalidTestFn { span: item_span })?;
        Ok(SynContent {
            context_attr,
            fn_under_test,
        })
    }
}

impl Model {
    pub(crate) fn parse(syn_content: SynContent) -> Result<Model, ParseError> {
        let SynContent {
            context_attr,
            fn_under_test,
        } = syn_content;

        let execution_context = if let Some(ident) = context_attr {
            TestExecutionContext::parse(ident)?
        } else {
            // TODO - switch default to execution in a child process
            TestExecutionContext::Local
        };
        let fn_under_test_output = UserTestFnOutput::parse(&fn_under_test.decl.output)?;
        // TODO - RESTORE
        //if fn_under_test_output == UserTestFnOutput::Unit
        //    && execution_context == TestExecutionContext::Local
        //{
        //    return Err(ParseError::TestsReturningUnitMustRunInAChildContext {
        //        span: fn_under_test.decl.output.span(),
        //    });
        //}
        let resources = extract_expected_resources(&fn_under_test.decl.inputs)?;
        Ok(Model {
            execution_context,
            fn_under_test,
            fn_under_test_output,
            resources,
        })
    }
}

impl TestExecutionContext {
    fn parse(ident: Ident) -> Result<Self, ParseError> {
        let found = ident.to_string().to_lowercase();
        match found.as_ref() {
            "local" => Ok(TestExecutionContext::Local),
            "process" => Ok(TestExecutionContext::Process),
            _ => Err(ParseError::InvalidTestAttribute { span: ident.span() }),
        }
    }
}

fn extract_expected_resources<'a>(
    args: impl IntoIterator<Item = &'a FnArg>,
) -> Result<Vec<Param>, ParseError> {
    args.into_iter().map(Param::parse).collect()
}

impl Param {
    fn parse(arg: &FnArg) -> Result<Param, ParseError> {
        const SIMPLE_ARGUMENTS_ONLY: &str =
            "test function arguments must be of explicit format `identifier: Type`";
        let ac = if let FnArg::Captured(ac) = &arg {
            ac
        } else {
            return Err(ParseError::InvalidArgumentType {
                msg: SIMPLE_ARGUMENTS_ONLY.to_string(),
                span: arg.span(),
            });
        };
        let ident = match &ac.pat {
            Pat::Ident(pi) if pi.by_ref.is_none() && pi.subpat.is_none() => pi.ident.clone(),
            _ => {
                return Err(ParseError::InvalidArgumentType {
                    msg: SIMPLE_ARGUMENTS_ONLY.to_string(),
                    span: arg.span(),
                })
            }
        };
        let kind = match &ac.ty {
            Type::Path(type_path) => ParamKind::parse(type_path, ArgKind::Owned)?,
            Type::Reference(type_ref) => {
                let arg_kind = if type_ref.mutability.is_some() {
                    ArgKind::RefMut
                } else {
                    ArgKind::Ref
                };
                match type_ref.elem.as_ref() {
                    Type::Path(type_path) => ParamKind::parse(type_path, arg_kind)?,
                    _ => return Err(ParseError::InvalidArgumentType {
                        msg: "test function arguments passed by reference must be of explicit format `identifier: &Test`".to_string(),
                        span: arg.span()
                    })
                }
            }
            _ => {
                return Err(ParseError::InvalidArgumentType {
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
}

impl ParamKind {
    fn parse(type_path: &TypePath, arg_kind: ArgKind) -> Result<ParamKind, ParseError> {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| ParseError::InvalidArgumentType {
                msg: "test function argument must not be empty".to_string(),
                span: type_path.span(),
            })?
            .into_value();
        let kind = match segment.ident.to_string().as_ref() {
            "LocalCNodeSlots" => ParamKind::CNodeSlots {
                count: extract_first_argument_as_unsigned(&segment.arguments)?,
            },
            "LocalCap" => parse_localcap_param_kind(&segment.arguments, arg_kind)?,
            "UserImage" => {
                let seg_name = extract_first_arg_type_path_last_segment(&segment.arguments)?
                    .ident
                    .to_string();
                if &seg_name == "Local" && arg_kind == ArgKind::Ref {
                    ParamKind::UserImage
                } else {
                    return Err(ParseError::InvalidArgumentType {
                        msg: "The only supported test function argument for UserImage is &UserImage<ferros::userland::role::Local>".to_string(),
                        span: segment.span(),
                    });
                }
            }
            "VSpaceScratchSlice" => {
                let seg_name = extract_first_arg_type_path_last_segment(&segment.arguments)?
                    .ident
                    .to_string();
                if &seg_name == "Local" && arg_kind == ArgKind::RefMut {
                    ParamKind::VSpaceScratch
                } else {
                    return Err(ParseError::InvalidArgumentType {
                        msg: "The only supported test function argument for VSpaceScratchSlice is &mut VSpaceScratchSlice<ferros::userland::role::Local>".to_string(),
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
                return Err(ParseError::InvalidArgumentType {
                    msg: format!("test function argument type was not recognized: {}", t),
                    span: segment.span(),
                })
            }
        };
        Ok(kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArgKind {
    Owned,
    Ref,
    RefMut,
}

fn parse_localcap_param_kind(
    arguments: &PathArguments,
    arg_kind: ArgKind,
) -> Result<ParamKind, ParseError> {
    let segment = extract_first_arg_type_path_last_segment(arguments)?;
    let type_name = segment.ident.to_string();
    match type_name.as_ref() {
        "Untyped" => Ok(ParamKind::Untyped {
            bits: extract_first_argument_as_unsigned(&segment.arguments)?,
        }),
        "ASIDPool" => Ok(ParamKind::ASIDPool {
            count: extract_first_argument_as_unsigned(&segment.arguments)?,
        }),
        "ThreadPriorityAuthority" => {
            if arg_kind == ArgKind::Ref {
                Ok(ParamKind::ThreadPriorityAuthority)
            } else {
                Err(ParseError::InvalidArgumentType {msg: format!("{} is only available as a type parameter of &LocalCap<>, not an owned LocalCap<>", &type_name),
            span: segment.span() })
            }
        }
        "LocalCNode" => {
            if arg_kind == ArgKind::Ref {
                Ok(ParamKind::CNode)
            } else {
                Err(ParseError::InvalidArgumentType {msg: format!("{} is only available as a type parameter of &LocalCap<>, not an owned LocalCap<>", &type_name),
                span: segment.span() })
            }
        }
        // TODO - expand the set of convenience aliases
        // "CNode" => unimplemented!(),
        // "CNodeSlotsData" => unimplemented!(),
        _ => Err(ParseError::InvalidArgumentType {
            msg: format!(
                "Found an unsupported LocalCap type parameter, {}",
                &type_name
            ),
            span: segment.span(),
        }),
    }
}

/// Given PathArguments like `<a::b::T<Foo>, U, V>`, extracts `T<Foo>`
fn extract_first_arg_type_path_last_segment(
    arguments: &PathArguments,
) -> Result<&PathSegment, ParseError> {
    // TODO - consider iterating on the error message usefulness
    const EXPECTED: &str =
        "Expected a ferros type argument (e.g. `Unsigned<U5>`, `ASIDPool<U1024>`)";
    if let PathArguments::AngleBracketed(abga) = arguments {
        let gen_arg = abga
            .args
            .first()
            .ok_or_else(|| ParseError::InvalidArgumentType {
                msg: "test function argument's generic parameter must not be empty".to_string(),
                span: abga.span(),
            })?
            .into_value();
        if let GenericArgument::Type(Type::Path(type_path)) = gen_arg {
            Ok(type_path
                .path
                .segments
                .last()
                .ok_or_else(|| ParseError::InvalidArgumentType {
                    msg: "test function argument's generic parameter must not be empty".to_string(),
                    span: type_path.span(),
                })?
                .into_value())
        } else {
            Err(ParseError::InvalidArgumentType {
                msg: EXPECTED.to_string(),
                span: arguments.span(),
            })
        }
    } else {
        Err(ParseError::InvalidArgumentType {
            msg: EXPECTED.to_string(),
            span: arguments.span(),
        })
    }
}

/// Meant to take PathArguments like <typenum::U5> and turn it into 5usize
fn extract_first_argument_as_unsigned(arguments: &PathArguments) -> Result<usize, ParseError> {
    let segment = extract_first_arg_type_path_last_segment(arguments)?;
    parse_typenum_unsigned(&segment.ident)
}

/// Parse "U5" into `5usize`
fn parse_typenum_unsigned(ident: &Ident) -> Result<usize, ParseError> {
    let err_fn = || {
        ParseError::InvalidArgumentType {
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

impl UserTestFnOutput {
    fn parse(return_type: &ReturnType) -> Result<Self, ParseError> {
        match return_type {
            ReturnType::Default => Ok(UserTestFnOutput::Unit),
            ReturnType::Type(_, box_ty) => {
                match box_ty.as_ref() {
                    Type::Tuple(tuple) => {
                        // allow the explicit unit tuple, `()` case
                        if tuple.elems.is_empty() {
                            Ok(UserTestFnOutput::Unit)
                        } else {
                            Err(ParseError::InvalidReturnType { span: tuple.span() })
                        }
                    }
                    Type::Path(type_path) => {
                        let segment = type_path
                            .path
                            .segments
                            .last()
                            .ok_or_else(|| ParseError::InvalidReturnType {
                                span: type_path.span(),
                            })?
                            .into_value();
                        match segment.ident.to_string().as_ref() {
                            "Result" => Ok(UserTestFnOutput::Result),
                            "TestOutcome" => Ok(UserTestFnOutput::TestOutcome),
                            _ => Err(ParseError::InvalidReturnType {
                                span: type_path.span(),
                            }),
                        }
                    }
                    _ => Err(ParseError::InvalidReturnType {
                        span: return_type.span(),
                    }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::{Ident, Span};
    #[test]
    fn test_execution_context_parse() {
        assert_eq!(
            TestExecutionContext::Process,
            TestExecutionContext::parse(Ident::new("process", Span::call_site())).unwrap()
        );
        assert_eq!(
            TestExecutionContext::Process,
            TestExecutionContext::parse(Ident::new("Process", Span::call_site())).unwrap()
        );
        assert_eq!(
            TestExecutionContext::Thread,
            TestExecutionContext::parse(Ident::new("thread", Span::call_site())).unwrap()
        );
        assert_eq!(
            TestExecutionContext::Local,
            TestExecutionContext::parse(Ident::new("local", Span::call_site())).unwrap()
        );
        assert!(TestExecutionContext::parse(Ident::new("whatever", Span::call_site())).is_err());
    }
}
