use core::marker::PhantomData;
use core::mem;
use core::ops::Sub;

use typenum::*;

use selfe_sys::*;

use crate::arch;
use crate::cap::{
    memory_kind, ASIDPool, Cap, CapType, LocalCNodeSlot, LocalCap, PhantomCap, Untyped,
};
use crate::error::SeL4Error;

#[derive(Debug)]
pub struct ASIDControl<FreePools: Unsigned> {
    _free_pools: PhantomData<FreePools>,
}

impl<FreePools: Unsigned> CapType for ASIDControl<FreePools> {}

impl<FreePools: Unsigned> PhantomCap for ASIDControl<FreePools> {
    fn phantom_instance() -> Self {
        Self {
            _free_pools: PhantomData {},
        }
    }
}

impl<FreePools: Unsigned> LocalCap<ASIDControl<FreePools>> {
    pub fn allocate_asid_pool(
        self,
        ut12: LocalCap<Untyped<U12, memory_kind::General>>,
        dest_slot: LocalCNodeSlot,
    ) -> Result<
        (
            LocalCap<ASIDPool<arch::ASIDPoolSize>>,
            LocalCap<ASIDControl<op!(FreePools - U1)>>,
        ),
        SeL4Error,
    >
    where
        FreePools: Sub<U1>,
        op!(FreePools - U1): Unsigned,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                self.cptr,          // _service
                ut12.cptr,          // untyped
                dest_cptr,          // root
                dest_offset,        // index
                arch::WordSize::U8, // depth
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: ASIDPool {
                    id: (arch::ASIDPoolCount::USIZE - FreePools::USIZE),
                    next_free_slot: 0,
                    _free_slots: PhantomData,
                },
                _role: PhantomData,
            },
            unsafe { mem::transmute(self) },
        ))
    }
}
