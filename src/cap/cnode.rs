use core::marker::PhantomData;
use core::ops::{Add, Sub};

use selfe_sys::*;

use typenum::operator_aliases::Diff;
use typenum::*;

use crate::cap::{role, CNodeRole, Cap, CapType, ChildCap, LocalCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<Role: CNodeRole> {
    pub(crate) radix: u8,
    pub(crate) _role: PhantomData<Role>,
}

pub type LocalCNode = CNode<role::Local>;
pub type ChildCNode = CNode<role::Child>;

#[derive(Debug)]
pub struct CNodeSlotsData<Size: Unsigned, Role: CNodeRole> {
    pub(crate) offset: usize,
    pub(crate) _size: PhantomData<Size>,
    pub(crate) _role: PhantomData<Role>,
}

/// Can only represent CNode slots with capacity tracked at runtime
#[derive(Debug)]
pub struct WCNodeSlotsData<Role: CNodeRole> {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) _role: PhantomData<Role>,
}

impl<Role: CNodeRole> CapType for CNode<Role> {}

impl<Size: Unsigned, Role: CNodeRole> CapType for CNodeSlotsData<Size, Role> {}

pub type CNodeSlots<Size, Role> = LocalCap<CNodeSlotsData<Size, Role>>;
pub type LocalCNodeSlots<Size> = CNodeSlots<Size, role::Local>;
pub type ChildCNodeSlots<Size> = CNodeSlots<Size, role::Child>;

pub type CNodeSlot<Role> = CNodeSlots<U1, Role>;
pub type LocalCNodeSlot = CNodeSlot<role::Local>;
pub type ChildCNodeSlot = CNodeSlot<role::Child>;

impl<Role: CNodeRole> CapType for WCNodeSlotsData<Role> {}
pub type WCNodeSlots = LocalCap<WCNodeSlotsData<role::Local>>;

impl<Size: Unsigned, CapRole: CNodeRole, Role: CNodeRole> Cap<CNodeSlotsData<Size, Role>, CapRole> {
    /// A private constructor
    pub(crate) fn internal_new(
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

    /// weaken erases the state-tracking types on a set of CNode
    /// slots.
    pub fn weaken(self) -> Cap<WCNodeSlotsData<Role>, CapRole> {
        Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: WCNodeSlotsData {
                offset: self.cap_data.offset,
                size: Size::USIZE,
                _role: PhantomData,
            },
        }
    }
    pub fn alloc<Count: Unsigned>(
        self,
    ) -> (
        Cap<CNodeSlotsData<Count, Role>, CapRole>,
        Cap<CNodeSlotsData<Diff<Size, Count>, Role>, CapRole>,
    )
    where
        Size: Sub<Count>,
        Diff<Size, Count>: Unsigned,
    {
        let (cptr, offset, _) = self.elim();

        (
            Cap::<CNodeSlotsData<Count, Role>, CapRole>::internal_new(cptr, offset),
            Cap::<CNodeSlotsData<Diff<Size, Count>, Role>, CapRole>::internal_new(
                cptr,
                offset + Count::USIZE,
            ),
        )
    }

    pub(crate) fn elim(self) -> (usize, usize, usize) {
        (self.cptr, self.cap_data.offset, Size::USIZE)
    }
}

impl<Size: Unsigned, Role: CNodeRole> CNodeSlots<Size, Role> {
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

impl<Size: Unsigned> LocalCNodeSlots<Size> {
    /// Gain temporary access to some slots for use in a function context.
    /// When the passed function call is complete, all capabilities
    /// in this range will be revoked and deleted.
    pub fn with_temporary<E, F>(&mut self, f: F) -> Result<Result<(), E>, SeL4Error>
    where
        F: FnOnce(Self) -> Result<(), E>,
    {
        // Call the function with an alias/copy of self
        let r = f(Cap::internal_new(self.cptr, self.cap_data.offset));
        unsafe { self.revoke_in_reverse() }
        Ok(r)
    }

    /// Blindly attempt to revoke and delete the contents of the slots,
    /// (in reverse order) ignoring errors related to empty slots.
    pub(crate) unsafe fn revoke_in_reverse(&self) {
        for offset in (self.cap_data.offset..self.cap_data.offset + Size::USIZE).rev() {
            // Clean up any child/derived capabilities that may have been created.
            let _err = seL4_CNode_Revoke(
                self.cptr,           // _service
                offset,              // index
                seL4_WordBits as u8, // depth
            );

            // Clean out the slot itself
            let _err = seL4_CNode_Delete(
                self.cptr,           // _service
                offset,              // index
                seL4_WordBits as u8, // depth
            );
        }
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

        unsafe {
            seL4_CNode_Copy(
                dest_cptr,            // _service
                dest_offset,          // index
                seL4_WordBits as u8,  // depth
                parent_cnode.cptr,    // src_root
                self.cptr,            // src_index
                seL4_WordBits as u8,  // src_depth
                CapRights::RW.into(), // rights
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::CNodeCopy(e))?;
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

#[derive(Debug)]
pub enum CNodeSlotsError {
    NotEnoughSlots,
}

impl<Role: CNodeRole> LocalCap<WCNodeSlotsData<Role>> {
    pub(crate) fn size(&self) -> usize {
        self.cap_data.size
    }

    /// Allocate `count` and return them as weak cnode slots.
    pub fn alloc(
        &mut self,
        count: usize,
    ) -> Result<LocalCap<WCNodeSlotsData<Role>>, CNodeSlotsError> {
        if count > self.cap_data.size {
            return Err(CNodeSlotsError::NotEnoughSlots);
        }
        let offset = self.cap_data.offset;
        self.cap_data.offset += count;
        self.cap_data.size -= count;
        Ok(Cap {
            cptr: self.cptr,
            cap_data: WCNodeSlotsData {
                offset,
                size: count,
                _role: PhantomData,
            },
            _role: PhantomData,
        })
    }
    /// Allocate `Count` and return them as strengthened cnode slots.
    pub fn alloc_strong<Count: Unsigned>(
        &mut self,
    ) -> Result<LocalCap<CNodeSlotsData<Count, Role>>, CNodeSlotsError> {
        let cap = self.alloc(Count::USIZE)?;
        Ok(Cap {
            cptr: cap.cptr,
            cap_data: CNodeSlotsData {
                offset: cap.cap_data.offset,
                _size: PhantomData,
                _role: PhantomData,
            },
            _role: PhantomData,
        })
    }
}

impl WCNodeSlots {
    pub(crate) fn into_strong_iter(self) -> impl Iterator<Item = LocalCNodeSlot> {
        (0..self.cap_data.size).map(move |n| Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset: self.cap_data.offset + n,
                _size: PhantomData,
                _role: PhantomData,
            },
        })
    }
}

impl<Role: CNodeRole> LocalCap<WCNodeSlotsData<Role>> {
    /// Iterate through the available slots in the runtime-tracked collection of slots,
    /// consuming slots each iter step.
    /// TODO - a way better name that isn't iter_mut or mut_iter
    pub(crate) fn incrementally_consuming_iter(
        &mut self,
    ) -> impl Iterator<Item = CNodeSlot<Role>> + '_ {
        let original_offset = self.cap_data.offset;
        let original_size = self.cap_data.size;
        let cptr = self.cptr;
        (0..original_size).map(move |n| {
            self.cap_data.offset += 1;
            self.cap_data.size -= 1;
            Cap {
                cptr,
                _role: PhantomData,
                cap_data: CNodeSlotsData {
                    offset: original_offset + n,
                    _size: PhantomData,
                    _role: PhantomData,
                },
            }
        })
    }
}
