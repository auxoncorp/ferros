use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{role, CNodeRole, Cap, CapRights, ChildCap, LocalCap, SeL4Error};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U0, U1};

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(super) radix: u8,
    pub(super) next_free_slot: usize,
    pub(super) _free_slots: PhantomData<FreeSlots>,
    pub(super) _role: PhantomData<Role>,
}

pub type LocalCNode<FreeSlots> = CNode<FreeSlots, role::Local>;
pub type ChildCNode<FreeSlots> = CNode<FreeSlots, role::Child>;

#[derive(Debug)]
pub(super) struct CNodeSlot {
    pub(super) cptr: usize,
    pub(super) offset: usize,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> LocalCap<CNode<FreeSlots, Role>> {
    // TODO: reverse these args to be consistent with everything else
    pub(super) fn consume_slot(self) -> (LocalCap<CNode<Sub1<FreeSlots>, Role>>, CNodeSlot)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        (
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: CNode {
                    radix: self.cap_data.radix,
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            CNodeSlot {
                cptr: self.cptr,
                offset: self.cap_data.next_free_slot,
            },
        )
    }

    /// Reserve Count slots. Return another node with the same cptr, but the
    /// requested capacity.
    /// TODO - Make this function private-only until we implement a safe way
    /// to expose aliased CNode objects.
    pub fn reserve_region<Count: Unsigned>(
        self,
    ) -> (
        LocalCap<CNode<Count, Role>>,
        LocalCap<CNode<Diff<FreeSlots, Count>, Role>>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        (
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: CNode {
                    radix: self.cap_data.radix,
                    next_free_slot: self.cap_data.next_free_slot,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: CNode {
                    radix: self.cap_data.radix,
                    next_free_slot: self.cap_data.next_free_slot + Count::to_usize(),
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        )
    }

    /// TODO - Make this function private-only until we implement a safe way
    /// to expose aliased CNode objects.
    pub(super) fn reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = LocalCap<CNode<U1, Role>>>,
        LocalCap<CNode<Diff<FreeSlots, Count>, Role>>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let iter_radix = self.cap_data.radix;
        let iter_cptr = self.cptr;
        (
            (self.cap_data.next_free_slot..self.cap_data.next_free_slot + Count::to_usize()).map(
                move |slot| Cap {
                    cptr: iter_cptr,
                    _role: PhantomData,
                    cap_data: CNode {
                        radix: iter_radix,
                        next_free_slot: slot,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            ),
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: CNode {
                    radix: self.cap_data.radix,
                    next_free_slot: self.cap_data.next_free_slot + Count::to_usize(),
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        )
    }
}

impl<FreeSlots: Unsigned> LocalCap<CNode<FreeSlots, role::Child>> {
    // The first returned cap goes in the child process params struct. The
    // second one goes in the TCB when starting the child process.
    pub fn generate_self_reference<ParentFreeSlots: Unsigned>(
        self,
        parent_cnode: &LocalCap<CNode<ParentFreeSlots, role::Local>>,
    ) -> Result<
        (
            ChildCap<CNode<Sub1<FreeSlots>, role::Child>>,
            LocalCap<CNode<U0, role::Child>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let (local_self, dest_slot) = self.consume_slot();

        let err = unsafe {
            seL4_CNode_Copy(
                dest_slot.cptr,       // _service
                dest_slot.offset,     // index
                seL4_WordBits as u8,  // depth
                parent_cnode.cptr,    // src_root
                local_self.cptr,      // src_index
                seL4_WordBits as u8,  // src_depth
                CapRights::RW.into(), // rights
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeCopy(err))
        } else {
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    _role: PhantomData,
                    cap_data: CNode {
                        radix: local_self.cap_data.radix,
                        next_free_slot: local_self.cap_data.next_free_slot + 1,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
                // Take this apart and put it back together to get it to the right type
                Cap {
                    cptr: local_self.cptr,
                    _role: PhantomData,
                    cap_data: CNode {
                        radix: local_self.cap_data.radix,
                        next_free_slot: core::usize::MAX,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            ))
        }
    }
}
