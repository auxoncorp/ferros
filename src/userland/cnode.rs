use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{role, CNodeRole, Cap, CapRights, ChildCap, LocalCap, SeL4Error};
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::*;

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<Role: CNodeRole> {
    pub(super) radix: u8,
    pub(super) _role: PhantomData<Role>,
}

pub type LocalCNode = CNode<role::Local>;
pub type ChildCNode = CNode<role::Child>;

#[derive(Debug)]
pub struct CNodeSlots<Size: Unsigned, Role: CNodeRole> {
    pub(super) cptr: usize,
    pub(super) offset: usize,
    pub(super) _size: PhantomData<Size>,
    pub(super) _role: PhantomData<Role>,
}

impl<Size: Unsigned, Role: CNodeRole> CNodeSlots<Size, Role> {
    pub fn elim(self) -> (usize, usize, usize) {
        (self.cptr, self.offset, Size::USIZE)
    }

    pub fn alloc<Count: Unsigned>(
        self,
    ) -> (CNodeSlots<Count, Role>, CNodeSlots<Diff<Size, Count>, Role>)
    where
        Size: Sub<Count>,
        Diff<Size, Count>: Unsigned,
    {
        (
            CNodeSlots {
                cptr: self.cptr,
                offset: self.offset,
                _size: PhantomData,
                _role: PhantomData,
            },
            CNodeSlots {
                cptr: self.cptr,
                offset: self.offset + Count::USIZE,
                _size: PhantomData,
                _role: PhantomData,
            },
        )
    }

    pub fn iter(self) -> impl Iterator<Item = CNodeSlot<Role>> {
        let cptr = self.cptr;
        let offset = self.offset;
        (0..Size::USIZE).map(move |n| CNodeSlots {
            cptr: cptr,
            offset: offset + n,
            _size: PhantomData,
            _role: PhantomData,
        })
    }
}

pub type LocalCNodeSlots<Size> = CNodeSlots<Size, role::Local>;
pub type ChildCNodeSlots<Size> = CNodeSlots<Size, role::Child>;

pub type CNodeSlot<Role> = CNodeSlots<U1, Role>;
pub type LocalCNodeSlot = CNodeSlot<role::Local>;
pub type ChildCNodeSlot = CNodeSlot<role::Child>;

impl LocalCap<ChildCNode> {
    // The first returned cap goes in the child process params struct. The
    // second one goes in the TCB when starting the child process.
    pub fn generate_self_reference(
        &self,
        parent_cnode: &LocalCap<LocalCNode>,
        dest_slot: ChildCNodeSlot,
    ) -> Result<ChildCap<ChildCNode>, SeL4Error> {
        let err = unsafe {
            seL4_CNode_Copy(
                dest_slot.cptr,       // _service
                dest_slot.offset,     // index
                seL4_WordBits as u8,  // depth
                parent_cnode.cptr,    // src_root
                self.cptr,            // src_index
                seL4_WordBits as u8,  // src_depth
                CapRights::RW.into(), // rights
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeCopy(err))
        } else {
            Ok(Cap {
                cptr: dest_slot.offset,
                _role: PhantomData,
                cap_data: CNode {
                    radix: self.cap_data.radix,
                    _role: PhantomData,
                },
            })
        }
    }
}
