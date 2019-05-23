use crate::arch;
use crate::userland::{
    memory_kind, role, AssignedPageDirectory, Cap, CapType, LocalCNodeSlot, LocalCap, Movable,
    PhantomCap, SeL4Error, UnassignedPageDirectory, Untyped,
};
use core::marker::PhantomData;
use core::mem;
use core::ops::Sub;
use selfe_sys::*;
use typenum::*;

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

#[derive(Debug)]
pub struct ASIDPool<FreeSlots: Unsigned> {
    pub(crate) id: usize,
    pub(crate) next_free_slot: usize,
    pub(crate) _free_slots: PhantomData<FreeSlots>,
}

impl<FreeSlots: Unsigned> CapType for ASIDPool<FreeSlots> {}

impl<FreeSlots: Unsigned> Movable for ASIDPool<FreeSlots> {}

#[derive(Debug)]
pub struct UnassignedASID {
    pub(crate) asid: usize,
}

impl CapType for UnassignedASID {}

#[derive(Debug)]
pub struct AssignedASID<ThreadCount: Unsigned> {
    asid: u32,
    _thread_count: PhantomData<ThreadCount>,
}

impl<ThreadCount: Unsigned> CapType for AssignedASID<ThreadCount> {}

#[derive(Debug)]
pub struct ThreadID {
    id: u32,
}

impl<FreePools: Unsigned> LocalCap<ASIDControl<FreePools>> {
    pub fn allocate_asid_pool(
        self,
        ut12: LocalCap<Untyped<U12, memory_kind::General>>,
        dest_slot: LocalCNodeSlot,
    ) -> Result<
        (
            LocalCap<ASIDPool<arch::asid::PoolSize>>,
            LocalCap<ASIDControl<op! {FreePools - U1}>>,
        ),
        SeL4Error,
    >
    where
        FreePools: Sub<U1>,
        op! {FreePools - U1}: Unsigned,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                self.cptr,          // _service
                ut12.cptr,          // untyped
                dest_cptr,          // root
                dest_offset,        // index
                arch::WordBits::U8, // depth
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: ASIDPool {
                    id: (arch::asid::PoolCount::USIZE - FreePools::USIZE),
                    next_free_slot: 0,
                    _free_slots: PhantomData,
                },
                _role: PhantomData,
            },
            unsafe { mem::transmute(self) },
        ))
    }
}

impl<FreeSlots: Unsigned> LocalCap<ASIDPool<FreeSlots>> {
    pub fn alloc(
        self,
    ) -> (
        LocalCap<UnassignedASID>,
        LocalCap<ASIDPool<op! {FreeSlots - U1}>>,
    )
    where
        FreeSlots: Sub<U1>,
        op! {FreeSlots- U1}: Unsigned,
    {
        (
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: UnassignedASID {
                    asid: (self.cap_data.id << arch::asid::PoolBits::USIZE)
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

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<
        (
            LocalCap<AssignedASID<U0>>,
            LocalCap<AssignedPageDirectory<arch::paging::BasePageDirFreeSlots, role::Child>>,
        ),
        SeL4Error,
    > {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, page_dir.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        let page_dir = Cap {
            cptr: page_dir.cptr,
            _role: PhantomData,
            cap_data: AssignedPageDirectory {
                next_free_slot: 0,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        };

        Ok((unsafe { mem::transmute(self) }, page_dir))
    }
}
