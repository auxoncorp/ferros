use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::{Error as SynError, Ident, ItemFn};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SynContent {
    pub(crate) context_attr: Option<Ident>,
    pub(crate) fn_under_test: ItemFn,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Model {
    pub(crate) execution_context: TestExecutionContext,
    pub(crate) fn_under_test: ItemFn,
    pub(crate) fn_under_test_output: UserTestFnOutput,
    pub(crate) resources: Vec<Param>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TestExecutionContext {
    /// Runs in the test harness' local scope (often the root task)
    Local,
    /// Runs in an isolated child process
    Process,
    /// Runs in a distinct thread in the test harness' virtual address space (often the root task process)
    Thread,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum UserTestFnOutput {
    Unit,
    TestOutcome,
    Result,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Param {
    pub(crate) original_ident: Ident,
    pub(crate) kind: ParamKind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParamKind {
    CNodeSlots { count: usize },
    Untyped { bits: usize },
    ASIDPool { count: usize },
    CNode,
    ThreadPriorityAuthority,
    UserImage,
}

#[derive(Debug, Clone)]
pub(crate) enum ParseError {
    InvalidArgumentType { msg: String, span: Span },
    InvalidTestAttribute { span: Span },
    InvalidTestFn { span: Span },
    InvalidReturnType { span: Span },
    TestsReturningUnitMustRunInAChildContext { span: Span },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let s = match self {
            ParseError::InvalidArgumentType { msg, .. } => &msg,
            ParseError::InvalidTestAttribute { .. } => "Invalid test attribute found. Try `#[ferros_test]` or `#[ferros_test(process)]` or `#[ferros_test(local)]`",
            ParseError::InvalidTestFn { .. } => "Test function could not be parsed as a fn item",
            ParseError::InvalidReturnType { .. } => "Invalid return type, prefer returning either TestOutcome or a Result<T, E> type",
            ParseError::TestsReturningUnitMustRunInAChildContext { .. } => "Tests that return nothing (unit, a.k.a. `()`) must run in a child thread or process. Use `#[ferros_test(process)]`",
        };
        f.write_str(s)
    }
}

impl ParseError {
    fn span(&self) -> Span {
        match self {
            ParseError::InvalidArgumentType { span, .. } => *span,
            ParseError::InvalidTestAttribute { span, .. } => *span,
            ParseError::InvalidTestFn { span, .. } => *span,
            ParseError::InvalidReturnType { span, .. } => *span,
            ParseError::TestsReturningUnitMustRunInAChildContext { span, .. } => *span,
        }
    }

    pub(crate) fn to_compile_error(&self) -> TokenStream2 {
        SynError::new(self.span(), self).to_compile_error()
    }
}
#[derive(Debug, Clone)]
pub(crate) enum CodegenError {
    Unexpected { span: Span },
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let s = match self {
            CodegenError::Unexpected { .. } => {
                "Did not expect any sort of error in the codegen phase"
            }
        };
        f.write_str(s)
    }
}

impl CodegenError {
    fn span(&self) -> Span {
        match self {
            CodegenError::Unexpected { span } => *span,
        }
    }
    pub(crate) fn to_compile_error(&self) -> TokenStream2 {
        SynError::new(self.span(), self).to_compile_error()
    }
}
