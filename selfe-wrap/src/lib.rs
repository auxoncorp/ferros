#![no_std]
pub mod error;
mod primitives;
mod selfe_sys_impl;

pub use primitives::*;
pub use selfe_sys_impl::SelfeKernel;

pub trait Kernel: SyscallKernel + CNodeKernel {}

pub trait SyscallKernel {
    fn yield_execution();
}

pub trait CNodeKernel {
    fn cnode_copy(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        rights: CapRights,
    ) -> Result<FullyQualifiedCptr, error::APIError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
