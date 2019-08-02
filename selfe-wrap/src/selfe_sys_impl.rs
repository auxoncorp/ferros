use super::error::{APIError, APIMethod, CNodeMethod, ErrorExt};
use super::primitives::{CapRights, FullyQualifiedCptr};
use super::{CNodeKernel, Kernel, SyscallKernel};
use selfe_sys::{seL4_CNode_Copy, seL4_WordBits, seL4_Yield};

#[derive(Debug, Clone, Default)]
pub struct SelfeKernel;

impl Kernel for SelfeKernel {}

impl SyscallKernel for SelfeKernel {
    fn yield_execution() {
        unsafe { seL4_Yield() }
    }
}

impl CNodeKernel for SelfeKernel {
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
                // Since src_cnode is restricted to CSpace Local Root, the cptr must
                // actually be the slot index
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
}
