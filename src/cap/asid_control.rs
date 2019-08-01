use core::marker::PhantomData;
use core::mem;
use core::ops::Sub;

use typenum::*;

use crate::arch;
use crate::cap::{memory_kind, ASIDPool, CapType, LocalCNodeSlot, LocalCap, PhantomCap, Untyped};
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
        mut self,
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
        let pool = self.make_asid_pool_without_consuming_control_pool(ut12, dest_slot)?;
        Ok((pool, unsafe { mem::transmute(self) }))
    }
}
