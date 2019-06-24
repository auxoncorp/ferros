use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use crate::arch;
use crate::cap::{Cap, CapType, LocalCap};
use crate::error::SeL4Error;
use crate::userland::CapRights;

#[derive(Debug)]
pub struct ASIDPool<FreeSlots: Unsigned> {
    pub(crate) id: usize,
    pub(crate) next_free_slot: usize,
    pub(crate) _free_slots: PhantomData<FreeSlots>,
}

impl<FreeSlots: Unsigned> CapType for ASIDPool<FreeSlots> {}

impl<FreeSlots: Unsigned> LocalCap<ASIDPool<FreeSlots>> {
    pub fn alloc(
        self,
    ) -> (
        LocalCap<arch::cap::UnassignedASID>,
        LocalCap<ASIDPool<op!(FreeSlots - U1)>>,
    )
    where
        FreeSlots: Sub<U1>,
        op!(FreeSlots - U1): Unsigned,
    {
        (
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: arch::cap::UnassignedASID {
                    asid: (self.cap_data.id << arch::ASIDLowBits::USIZE)
                        | self.cap_data.next_free_slot,
                },
            },
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: ASIDPool {
                    id: self.cap_data.id,
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                },
            },
        )
    }

    #[cfg(feature = "test_support")]
    pub fn split<
        LeftRole: crate::cap::CNodeRole,
        RightRole: crate::cap::CNodeRole,
        LeftSlots: Unsigned,
        RightSlots: Unsigned,
    >(
        self,
        left_slot: crate::cap::CNodeSlot<LeftRole>,
        right_slot: crate::cap::CNodeSlot<RightRole>,
        src_cnode: &LocalCap<crate::cap::LocalCNode>,
    ) -> Result<
        (
            Cap<ASIDPool<LeftSlots>, LeftRole>,
            Cap<ASIDPool<RightSlots>, RightRole>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<U2>,
        op!(FreeSlots - U2): Unsigned,
        LeftSlots: core::ops::Add<RightSlots>,
        RightSlots: core::ops::Add<LeftSlots>,
        op!(LeftSlots + RightSlots): Unsigned,
        op!(LeftSlots + RightSlots): IsLessOrEqual<FreeSlots, Output = True>,
    {
        let left_offset = self.unchecked_copy(src_cnode, left_slot, CapRights::RWG)?;
        let right_offset = self.unchecked_copy(src_cnode, right_slot, CapRights::RWG)?;
        Ok((
            Cap {
                cptr: left_offset,
                _role: PhantomData,
                cap_data: ASIDPool {
                    id: self.cap_data.id,
                    next_free_slot: self.cap_data.next_free_slot,
                    _free_slots: PhantomData,
                },
            },
            Cap {
                cptr: right_offset,
                _role: PhantomData,
                cap_data: ASIDPool {
                    id: self.cap_data.id,
                    next_free_slot: self.cap_data.next_free_slot + LeftSlots::USIZE,
                    _free_slots: PhantomData,
                },
            },
        ))
    }

    pub fn truncate<OutFreeSlots: Unsigned>(self) -> LocalCap<ASIDPool<OutFreeSlots>>
    where
        FreeSlots: IsGreaterOrEqual<OutFreeSlots, Output = True>,
    {
        Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: ASIDPool {
                id: self.cap_data.id,
                next_free_slot: self.cap_data.next_free_slot
                    + (FreeSlots::USIZE - OutFreeSlots::USIZE),
                _free_slots: PhantomData,
            },
        }
    }
}
