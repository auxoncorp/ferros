use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{role, CNodeRole, Cap, LocalCap};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U1, U1024};

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

pub type LocalCNode<FreeSlots: Unsigned> = CNode<FreeSlots, role::Local>;
pub type ChildCNode<FreeSlots: Unsigned> = CNode<FreeSlots, role::Child>;

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
        // TODO - does this method need to change when for CNode<FreeSlots, role::Child> ??
        (
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                _cap_data: CNode {
                    radix: self._cap_data.radix,
                    next_free_slot: self._cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            CNodeSlot {
                cptr: self.cptr,
                offset: self._cap_data.next_free_slot,
            },
        )
    }

    /// Reserve Count slots. Return another node with the same cptr, but the
    /// requested capacity.
    /// TODO - Make this function private-only until we implement a safe way
    /// to expose aliased CNode objects.
    pub(super) fn reserve_region<Count: Unsigned>(
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
                _cap_data: CNode {
                    radix: self._cap_data.radix,
                    next_free_slot: self._cap_data.next_free_slot,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                _cap_data: CNode {
                    radix: self._cap_data.radix,
                    next_free_slot: self._cap_data.next_free_slot + Count::to_usize(),
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
        let iter_radix = self._cap_data.radix;
        let iter_cptr = self.cptr;
        (
            (self._cap_data.next_free_slot..self._cap_data.next_free_slot + Count::to_usize()).map(
                move |slot| Cap {
                    cptr: iter_cptr,
                    _role: PhantomData,
                    _cap_data: CNode {
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
                _cap_data: CNode {
                    radix: self._cap_data.radix,
                    next_free_slot: self._cap_data.next_free_slot + Count::to_usize(),
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        )
    }
}

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps. But there are also a bunch
// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(bootinfo: &'static seL4_BootInfo) -> LocalCap<CNode<U1024, role::Local>> {
    Cap {
        cptr: seL4_CapInitThreadCNode as usize,
        _role: PhantomData,
        _cap_data: CNode {
            radix: 19,
            next_free_slot: 1000, // TODO: look at the bootinfo to determine the real value
            _free_slots: PhantomData,
            _role: PhantomData,
        },
    }
}
