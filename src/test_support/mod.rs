use super::alloc::*;
use super::userland::*;
use crate::pow::Pow;
use core::marker::PhantomData;
use selfe_sys::*;
use typenum::*;

#[derive(Debug, Clone, Copy)]
pub enum TestOutcome {
    Success,
    Failure,
}

type MaxTestUntypedSize = U27;
type MaxTestCNodeSlots = Pow<U15>;
type RunTest = Fn(
    &LocalCap<LocalCNode>,
    LocalCNodeSlots<MaxTestCNodeSlots>,
    Untyped<MaxTestUntypedSize>,
    ASIDPool<super::arch::asid::PoolSize>,
    UserImage<role::Local>,
    VSpaceScratchSlice<role::Local>,
    LocalCap<ThreadPriorityAuthority>,
) -> TestOutcome;

/// Gain temporary access to some slots and memory for use in a function context.
/// When the passed function call is complete, all capabilities
/// in this range will be revoked and deleted and the memory reclaimed.
pub fn with_temporary_resources<SlotCount: Unsigned, BitSize: Unsigned, E, F>(
    slots: &mut LocalCNodeSlots<SlotCount>,
    untyped: &mut LocalCap<cap::Untyped<BitSize>>,
    asid_pool: &mut LocalCap<asid::ASIDPool<super::arch::asid::PoolSize>>,
    f: F,
) -> Result<Result<(), E>, SeL4Error>
where
    F: FnOnce(
        LocalCNodeSlots<SlotCount>,
        LocalCap<cap::Untyped<BitSize>>,
        LocalCap<asid::ASIDPool<super::arch::asid::PoolSize>>,
    ) -> Result<(), E>,
{
    // Call the function with an alias/copy of self
    let r = f(
        Cap::internal_new(slots.cptr, slots.cap_data.offset),
        Cap {
            cptr: untyped.cptr,
            cap_data: crate::userland::cap::Untyped {
                _bit_size: PhantomData,
                _kind: PhantomData,
            },
            _role: PhantomData,
        },
        Cap {
            cptr: asid_pool.cptr,
            _role: PhantomData,
            cap_data: ASIDPool {
                id: asid_pool.cap_data.id,
                next_free_slot: asid_pool.cap_data.next_free_slot,
                _free_slots: PhantomData,
            },
        },
    );
    unsafe { slots.revoke_in_reverse() }

    // Clean up any child/derived capabilities that may have been created from the memory
    // Because the slots and the untyped are both Local, the slots' parent CNode capability pointer
    // must be the same as the untyped's parent CNode
    let err = unsafe {
        seL4_CNode_Revoke(
            slots.cptr,          // _service
            untyped.cptr,        // index
            seL4_WordBits as u8, // depth
        )
    };
    if err != 0 {
        return Err(SeL4Error::CNodeRevoke(err));
    }
    Ok(r)
}
