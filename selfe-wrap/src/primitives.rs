use selfe_sys::{seL4_CapRights_new, seL4_CapRights_t};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CapRights {
    R,
    W,
    RW,
    RWG,
    WG,
    /// Can Grant ReplY
    Y,
}

impl From<CapRights> for seL4_CapRights_t {
    fn from(cr: CapRights) -> Self {
        match cr {
            CapRights::R => unsafe { seL4_CapRights_new(0, 0, 1, 0) },
            CapRights::W => unsafe { seL4_CapRights_new(0, 0, 0, 1) },
            CapRights::RW => unsafe { seL4_CapRights_new(0, 0, 1, 1) },
            CapRights::RWG => unsafe { seL4_CapRights_new(0, 1, 1, 1) },
            CapRights::WG => unsafe { seL4_CapRights_new(0, 1, 0, 1) },
            CapRights::Y => unsafe { seL4_CapRights_new(1, 0, 0, 0) },
        }
    }
}

/// An offset relative to a given CSpace
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CapIndex {
    Local(LocalCapIndex),
    Child(ChildCapIndex),
}

/// An offset relative to the CSpace of the currently executing thread
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalCapIndex(pub usize);
/// An offset within the CSpace of a child CNode
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildCapIndex(pub usize);

/// A pointer to a CNode capability, relative to the CSpace of the currently executing thread
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CNodeCptr(pub LocalCapIndex);

#[derive(Debug, Clone, PartialEq)]
pub struct FullyQualifiedCptr {
    pub cnode: CNodeCptr,
    pub index: CapIndex,
}

/// A continuous span of fully qualified capability pointers
/// starting from an initial cptr
#[derive(Debug, Clone, PartialEq)]
pub struct FullyQualifiedCptrSpan {
    pub cptr: FullyQualifiedCptr,
    // N.B. For future rigor, consider making this core::num::NonZeroUsize
    pub size: usize,
}

impl From<CNodeCptr> for usize {
    fn from(CNodeCptr(LocalCapIndex(v)): CNodeCptr) -> Self {
        v
    }
}

impl From<CapIndex> for usize {
    fn from(v: CapIndex) -> Self {
        match v {
            CapIndex::Local(LocalCapIndex(i)) => i,
            CapIndex::Child(ChildCapIndex(i)) => i,
        }
    }
}

impl From<usize> for LocalCapIndex {
    fn from(v: usize) -> Self {
        LocalCapIndex(v)
    }
}

impl From<usize> for ChildCapIndex {
    fn from(v: usize) -> Self {
        ChildCapIndex(v)
    }
}
