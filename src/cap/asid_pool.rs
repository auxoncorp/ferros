use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use crate::arch;
use crate::cap::{Cap, CapType, LocalCap, Movable};

#[derive(Debug)]
pub struct ASIDPool<FreeSlots: Unsigned> {
    pub(crate) id: usize,
    pub(crate) next_free_slot: usize,
    pub(crate) _free_slots: PhantomData<FreeSlots>,
}

impl<FreeSlots: Unsigned> CapType for ASIDPool<FreeSlots> {}

impl<FreeSlots: Unsigned> Movable for ASIDPool<FreeSlots> {}

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
                    asid: (self.cap_data.id << arch::ASIDPoolBits::USIZE)
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
