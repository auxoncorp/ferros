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
    /// Move a capability from a source into a destination slot.
    /// If successful, return the destination slot pointer
    fn cnode_move(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
    ) -> Result<FullyQualifiedCptr, error::APIError>;

    /// Move a capability from a source into a destination slot,
    /// while also setting the badge of the moved capability.
    /// If successful, return the destination slot pointer
    fn cnode_mutate(
        source: FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        badge_or_guard: BadgeOrGuard,
    ) -> Result<FullyQualifiedCptr, error::APIError>;

    /// Copy a capability from a source into a destination slot.
    /// If successful, return the destination slot pointer
    fn cnode_copy(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        rights: CapRights,
    ) -> Result<FullyQualifiedCptr, error::APIError>;

    /// Copy a capability from a source into a destination slot,
    /// while also setting the badge of the fresh capability.
    /// If successful, return the destination slot pointer
    fn cnode_mint(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        rights: CapRights,
        badge_or_guard: BadgeOrGuard,
    ) -> Result<FullyQualifiedCptr, error::APIError>;

    /// If successful, return the now-empty slot pointer
    fn cnode_delete(target: FullyQualifiedCptr) -> Result<FullyQualifiedCptr, error::APIError>;

    /// If successful, return the target pointer
    fn cnode_revoke(target: FullyQualifiedCptr) -> Result<FullyQualifiedCptr, error::APIError>;

    fn save_caller(destination: FullyQualifiedCptr) -> Result<FullyQualifiedCptr, error::APIError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
