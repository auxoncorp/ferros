/// See https://github.com/seL4/seL4_libs/blob/master/libsel4vka/include/vka/cspacepath_t.h
use sel4_sys::{seL4_CNode, seL4_CPtr, seL4_Word};

#[derive(Clone, Debug)]
pub struct CSpacePath {
    pub cap_ptr: seL4_CPtr,
    pub cap_depth: seL4_Word,
    pub root: seL4_CNode,
    pub dest: seL4_Word,
    pub dest_depth: seL4_Word,
    pub offset: seL4_Word,
    pub window: seL4_Word,
}

/*
impl CSpacePath {
}
*/
