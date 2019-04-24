use crate::userland::{role, CNodeRole, Cap, CapRights, ChildCap, LocalCap, SeL4Error};
use core::marker::PhantomData;
use core::ops::{Add, Sub};
use selfe_sys::*;
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
pub struct CNodeSlotsData<Size: Unsigned, Role: CNodeRole> {
    offset: usize,
    _size: PhantomData<Size>,
    _role: PhantomData<Role>,
}

pub type CNodeSlots<Size, Role> = LocalCap<CNodeSlotsData<Size, Role>>;
pub type LocalCNodeSlots<Size> = CNodeSlots<Size, role::Local>;
pub type ChildCNodeSlots<Size> = CNodeSlots<Size, role::Child>;

pub type CNodeSlot<Role> = CNodeSlots<U1, Role>;
pub type LocalCNodeSlot = CNodeSlot<role::Local>;
pub type ChildCNodeSlot = CNodeSlot<role::Child>;

impl<Size: Unsigned, CapRole: CNodeRole, Role: CNodeRole> Cap<CNodeSlotsData<Size, Role>, CapRole> {
    /// A private constructor
    pub(super) fn internal_new(
        cptr: usize,
        offset: usize,
    ) -> Cap<CNodeSlotsData<Size, Role>, CapRole> {
        Cap {
            cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset,
                _size: PhantomData,
                _role: PhantomData,
            },
        }
    }
}

impl<Size: Unsigned, Role: CNodeRole> CNodeSlots<Size, Role> {
    pub fn elim(self) -> (usize, usize, usize) {
        (self.cptr, self.cap_data.offset, Size::USIZE)
    }

    pub fn alloc<Count: Unsigned>(
        self,
    ) -> (CNodeSlots<Count, Role>, CNodeSlots<Diff<Size, Count>, Role>)
    where
        Size: Sub<Count>,
        Diff<Size, Count>: Unsigned,
    {
        let (cptr, offset, _) = self.elim();

        (
            CNodeSlots::internal_new(cptr, offset),
            CNodeSlots::internal_new(cptr, offset + Count::USIZE),
        )
    }

    pub fn iter(self) -> impl Iterator<Item = CNodeSlot<Role>> {
        let cptr = self.cptr;
        let offset = self.cap_data.offset;
        (0..Size::USIZE).map(move |n| Cap {
            cptr: cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset: offset + n,
                _size: PhantomData,
                _role: PhantomData,
            },
        })
    }
}

impl LocalCap<ChildCNode> {
    pub fn generate_self_reference<SlotsForChild: Unsigned>(
        &self,
        parent_cnode: &LocalCap<LocalCNode>,
        dest_slots: LocalCap<CNodeSlotsData<op! {SlotsForChild + U1}, role::Child>>,
    ) -> Result<
        (
            ChildCap<ChildCNode>,
            ChildCap<CNodeSlotsData<SlotsForChild, role::Child>>,
        ),
        SeL4Error,
    >
    where
        SlotsForChild: Add<U1>,
        op! {SlotsForChild +  U1}: Unsigned,
    {
        let (dest_cptr, dest_offset, _) = dest_slots.elim();

        let err = unsafe {
            seL4_CNode_Copy(
                dest_cptr,            // _service
                dest_offset,          // index
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
            Ok((
                Cap {
                    cptr: dest_offset,
                    _role: PhantomData,
                    cap_data: CNode {
                        radix: self.cap_data.radix,
                        _role: PhantomData,
                    },
                },
                Cap::internal_new(dest_offset, dest_offset + 1),
            ))
        }
    }
}
