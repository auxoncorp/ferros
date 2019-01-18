use core::marker::PhantomData;
use core::mem::size_of;
use core::ops::Sub;
use crate::userland::{
    role, ASIDControl, ASIDPool, AssignedPageDirectory, CNode, Cap, Error, LocalCap, MappedPage,
    PhantomCap, ThreadControlBlock, UnassignedPageDirectory, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1, U1024, U12};

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps. But there are also a bunch
// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(_bootinfo: &'static seL4_BootInfo) -> LocalCap<CNode<U1024, role::Local>> {
    Cap {
        cptr: seL4_CapInitThreadCNode as usize,
        _role: PhantomData,
        cap_data: CNode {
            radix: 19,
            next_free_slot: 1000, // TODO: look at the bootinfo to determine the real value
            _free_slots: PhantomData,
            _role: PhantomData,
        },
    }
}

pub struct BootInfo {
    pub page_directory: Cap<AssignedPageDirectory, role::Local>,
    pub tcb: LocalCap<ThreadControlBlock>,
    pub asid_pool: LocalCap<ASIDPool>,
    user_image_frames_start: usize,
    user_image_frames_end: usize,
}

impl BootInfo {
    pub fn wrap<FreeSlots: Unsigned>(
        bootinfo: &'static seL4_BootInfo,
        asid_pool_ut: LocalCap<Untyped<U12>>,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> (BootInfo, LocalCap<CNode<Sub1<FreeSlots>, role::Local>>)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        // asid pool
        let asid_control = Cap::wrap_cptr(seL4_CapASIDControl as usize);

        let (asid_pool, dest_cnode): (Cap<ASIDPool, _>, _) = asid_pool_ut
            .retype_asid_pool(asid_control, dest_cnode)
            .expect("retype asid pool");

        (
            BootInfo {
                page_directory: Cap::wrap_cptr(seL4_CapInitThreadVSpace as usize),
                tcb: Cap::wrap_cptr(seL4_CapInitThreadTCB as usize),
                asid_pool: asid_pool,
                user_image_frames_start: bootinfo.userImageFrames.start,
                user_image_frames_end: bootinfo.userImageFrames.end,
            },
            dest_cnode,
        )
    }

    // TODO this doesn't enforce the aliasing constraints we want at the type
    // level. This can be modeled as an array (or other sized thing) once we
    // know how big the user image is.
    pub fn user_image_pages_iter(&self) -> impl Iterator<Item = Cap<MappedPage, role::Local>> {
        (self.user_image_frames_start..self.user_image_frames_end)
            .map(|cptr| Cap::<MappedPage, role::Local>::wrap_cptr(cptr as usize))
    }
}

// The ASID pool needs an untyped of exactly 4k
impl LocalCap<Untyped<U12>> {
    pub fn retype_asid_pool<FreeSlots: Unsigned>(
        self,
        asid_control: LocalCap<ASIDControl>,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<ASIDPool>,
            LocalCap<CNode<Sub1<FreeSlots>, role::Local>>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                asid_control.cptr,              // _service
                self.cptr,                      // untyped
                dest_slot.cptr,                 // root
                dest_slot.offset,               // index
                (8 * size_of::<usize>()) as u8, // depth
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }
}

impl Cap<ASIDPool, role::Local> {
    pub fn assign(
        &mut self,
        vspace: Cap<UnassignedPageDirectory, role::Local>,
    ) -> Result<Cap<AssignedPageDirectory, role::Local>, Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, vspace.cptr) };

        if err != 0 {
            return Err(Error::ASIDPoolAssign(err));
        }

        Ok(Cap {
            cptr: vspace.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}
