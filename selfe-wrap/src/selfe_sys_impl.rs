use super::error::{APIError, APIMethod, CNodeMethod, ErrorExt};
use super::primitives::{Badge, CapRights, FullyQualifiedCptr};
use super::{CNodeKernel, Kernel, SyscallKernel};
use selfe_sys::{
    seL4_CNode_Copy, seL4_CNode_Delete, seL4_CNode_Mint, seL4_CNode_Move, seL4_CNode_Revoke,
    seL4_CNode_SaveCaller, seL4_WordBits, seL4_Yield,
};

#[derive(Debug, Clone, Default)]
pub struct SelfeKernel;

impl Kernel for SelfeKernel {}

impl SyscallKernel for SelfeKernel {
    fn yield_execution() {
        unsafe { seL4_Yield() }
    }
}

impl CNodeKernel for SelfeKernel {
    fn cnode_move(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
    ) -> Result<FullyQualifiedCptr, APIError> {
        match unsafe {
            seL4_CNode_Move(
                destination.cnode.into(), // _service
                destination.index.into(), // index
                seL4_WordBits as u8,      // depth
                // Since source.cnode is restricted to CSpace Local, the cptr must
                // actually be a slot index
                source.cnode.into(), // src_root
                source.index.into(), // src_index
                seL4_WordBits as u8, // src_depth
            )
        }
        .as_result()
        {
            Err(e) => Err(APIError::new(APIMethod::CNode(CNodeMethod::Copy), e)),
            Ok(_) => Ok(destination),
        }
    }
    fn cnode_copy(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        rights: CapRights,
    ) -> Result<FullyQualifiedCptr, APIError> {
        match unsafe {
            seL4_CNode_Copy(
                destination.cnode.into(), // _service
                destination.index.into(), // index
                seL4_WordBits as u8,      // depth
                // Since source.cnode is restricted to CSpace Local, the cptr must
                // actually be a slot index
                source.cnode.into(), // src_root
                source.index.into(), // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
            )
        }
        .as_result()
        {
            Err(e) => Err(APIError::new(APIMethod::CNode(CNodeMethod::Copy), e)),
            Ok(_) => Ok(destination),
        }
    }

    fn cnode_mint(
        source: &FullyQualifiedCptr,
        destination: FullyQualifiedCptr,
        rights: CapRights,
        badge: Badge,
    ) -> Result<FullyQualifiedCptr, APIError> {
        match unsafe {
            seL4_CNode_Mint(
                destination.cnode.into(), // _service
                destination.index.into(), // dest index
                seL4_WordBits as u8,      // dest depth
                // Since source.cnode is restricted to CSpace Local, the cptr must
                // actually be a slot index
                source.cnode.into(), // src_root
                source.index.into(), // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
                badge.into(),        // badge
            )
        }
        .as_result()
        {
            Err(e) => Err(APIError::new(APIMethod::CNode(CNodeMethod::Mint), e)),
            Ok(_) => Ok(destination),
        }
    }
    fn cnode_delete(target: FullyQualifiedCptr) -> Result<(), APIError> {
        unsafe {
            seL4_CNode_Delete(
                target.cnode.into(), // _service
                target.index.into(), // index
                seL4_WordBits as u8, // depth
            )
        }
        .as_result()
        .map_err(|e| APIError::new(APIMethod::CNode(CNodeMethod::Delete), e))
    }

    fn cnode_revoke(target: FullyQualifiedCptr) -> Result<(), APIError> {
        unsafe {
            seL4_CNode_Revoke(
                target.cnode.into(), // _service
                target.index.into(), // index
                seL4_WordBits as u8, // depth
            )
        }
        .as_result()
        .map_err(|e| APIError::new(APIMethod::CNode(CNodeMethod::Revoke), e))
    }

    fn save_caller(destination: FullyQualifiedCptr) -> Result<FullyQualifiedCptr, APIError> {
        unsafe {
            seL4_CNode_SaveCaller(
                destination.cnode.into(), // _service
                destination.index.into(), // index
                seL4_WordBits as u8,      // depth
            )
        }
        .as_result()
        .map(|_| destination)
        .map_err(|e| APIError::new(APIMethod::CNode(CNodeMethod::SaveCaller), e))
    }
}
