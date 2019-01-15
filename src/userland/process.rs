use crate::userland::{
    role, ASIDPool, AssignedPageDirectory, CNode, Cap, Endpoint, Error, MappedPage,
    ThreadControlBlock, UnassignedPageDirectory, UnmappedPage, UnmappedPageTable, Untyped,
};
use core::mem::{self, size_of};
use core::ops::Sub;
use core::ptr;
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::{Unsigned, U128, U16, U256};

impl Cap<ThreadControlBlock, role::Local> {
    pub fn configure<FreeSlots: Unsigned>(
        &mut self,
        // fault_ep: Cap<Endpoint>,
        cspace_root: CNode<FreeSlots, role::Child>,
        // cspace_root_data: usize, // set the guard bits here
        vspace_root: Cap<AssignedPageDirectory, role::Local>, // TODO make a marker trait for VSpace?
                                                              // vspace_root_data: usize, // always 0
                                                              // buffer: usize,
                                                              // buffer_frame: Cap<Frame>,
    ) -> Result<(), Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                          // guard
                seL4_WordBits - cspace_root.radix as usize, // guard size in bits
            )
        }
        .words[0];

        let tcb_err = unsafe {
            seL4_TCB_Configure(
                self.cptr,
                seL4_CapNull as usize, // fault_ep.cptr,
                cspace_root.cptr,
                cspace_root_data,
                vspace_root.cptr,
                seL4_NilData as usize,
                0,
                0,
            )
        };

        if tcb_err != 0 {
            Err(Error::TCBConfigure(tcb_err))
        } else {
            Ok(())
        }
    }
}

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub trait RetypeForSetup {
    type Output;
}

type SetupVer<X> = <X as RetypeForSetup>::Output;

pub fn spawn<
    T: RetypeForSetup,
    FreeSlots: Unsigned,
    RootCNodeFreeSlots: Unsigned,
    UserImagePagesIter: Iterator<Item = Cap<MappedPage, role::Local>>,
    StackSize: Unsigned,
>(
    // process-related
    function_descriptor: extern "C" fn(&T) -> (),
    process_parameter: SetupVer<T>,
    child_cnode: CNode<RootCNodeFreeSlots, role::Child>,
    priority: u8,
    stack_ut: Cap<Untyped<StackSize>, role::Local>,

    // context-related
    ut16: Cap<Untyped<U16>, role::Local>,
    asid_pool: &mut Cap<ASIDPool, role::Local>,
    local_page_directory: &mut Cap<AssignedPageDirectory, role::Local>,
    user_image_pages_iter: UserImagePagesIter,
    local_tcb: Cap<ThreadControlBlock, role::Local>,
    local_cnode: CNode<FreeSlots, role::Local>,
) -> Result<CNode<Diff<FreeSlots, U256>, role::Local>, Error>
where
    FreeSlots: Sub<U256>,
    Diff<FreeSlots, U256>: Unsigned,
{
    // TODO can we somehow make this a static assertion? both of these should be const
    assert!(size_of::<SetupVer<T>>() == size_of::<T>());

    // this significantly cleans up the type constraints above
    let (cnode, local_cnode) = local_cnode.reserve_region::<U256>();

    let (ut14, page_dir_ut, _, _, cnode) = ut16.quarter(cnode)?;
    let (ut12, stack_page_ut, _, _, cnode) = ut14.quarter(cnode)?;
    let (ut10, stack_page_table_ut, code_page_table_ut, tcb_ut, cnode) = ut12.quarter(cnode)?;
    let (ut8, _, _, _, cnode) = ut10.quarter(cnode)?;
    let (ut6, _, _, _, cnode) = ut8.quarter(cnode)?;
    let (fault_endpoint_ut, _, _, _, cnode) = ut6.quarter(cnode)?;

    // TODO: Need to duplicate this endpoint into the child cnode
    let (fault_endpoint, cnode): (Cap<Endpoint, _>, _) = fault_endpoint_ut.retype_local(cnode)?;

    // Set up a single 4k page for the child's stack
    // TODO: Variable stack size
    let stack_base = 0x10000000;
    let stack_top = stack_base + 0x1000;

    let (page_dir, cnode): (Cap<UnassignedPageDirectory, _>, _) =
        page_dir_ut.retype_local(cnode)?;
    let mut page_dir = asid_pool.assign(page_dir)?;

    let (stack_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
        stack_page_table_ut.retype_local(cnode)?;
    let (stack_page, cnode): (Cap<UnmappedPage, _>, _) = stack_page_ut.retype_local(cnode)?;

    // map the child stack into local memory so we can set it up
    let stack_page_table = local_page_directory.map_page_table(stack_page_table, stack_base)?;
    let stack_page = local_page_directory.map_page(stack_page, stack_base)?;

    // put the parameter struct on the stack
    let param_target_addr = (stack_top - size_of::<T>());
    assert!(param_target_addr >= stack_base);

    unsafe {
        ptr::copy_nonoverlapping(
            &process_parameter as *const SetupVer<T>,
            param_target_addr as *mut SetupVer<T>,
            1,
        )
    }

    let sp = param_target_addr;

    // unmap the stack pages
    let stack_page = stack_page.unmap()?;
    let stack_page_table = stack_page_table.unmap()?;

    // map the stack to the target address space
    let stack_page_table = page_dir.map_page_table(stack_page_table, stack_base)?;
    let stack_page = page_dir.map_page(stack_page, stack_base)?;

    // map in the user image
    let program_vaddr_start = 0x00010000;
    let program_vaddr_end = program_vaddr_start + 0x00060000;

    // TODO: map enough page tables for larger images? Ideally, find out the
    // image size from the build linker, somehow.
    let (code_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
        code_page_table_ut.retype_local(cnode)?;
    let code_page_table = page_dir.map_page_table(code_page_table, program_vaddr_start)?;

    // TODO: the number of pages we reserve here needs to be checked against the
    // size of the binary.
    let (dest_reservation_iter, cnode) = cnode.reservation_iter::<U128>();
    let vaddr_iter = (program_vaddr_start..program_vaddr_end).step_by(0x1000);

    for ((page_cap, slot_cnode), vaddr) in user_image_pages_iter
        .zip(dest_reservation_iter)
        .zip(vaddr_iter)
    {
        let (copied_page_cap, _) = page_cap.copy_local(
            &local_cnode,
            slot_cnode,
            // TODO encapsulate caprights
            unsafe { seL4_CapRights_new(0, 1, 0) },
        )?;

        let _mapped_page_cap = page_dir.map_page(copied_page_cap, vaddr)?;
    }

    let (mut tcb, cnode): (Cap<ThreadControlBlock, _>, _) = tcb_ut.retype_local(cnode)?;
    tcb.configure(child_cnode, page_dir)?;

    // TODO: stack pointer is supposed to be 8-byte aligned on ARM
    let mut regs: seL4_UserContext = unsafe { mem::zeroed() };
    regs.pc = function_descriptor as seL4_Word;
    regs.sp = sp;
    regs.r0 = param_target_addr;
    regs.r14 = (yield_forever as *const fn() -> ()) as seL4_Word;

    let err = unsafe {
        seL4_TCB_WriteRegisters(
            tcb.cptr,
            0,
            0,
            // all the regs
            (size_of::<seL4_UserContext>() / size_of::<seL4_Word>()),
            &mut regs,
        )
    };

    if err != 0 {
        return Err(Error::TCBWriteRegisters(err));
    }

    let err = unsafe { seL4_TCB_SetPriority(tcb.cptr, local_tcb.cptr, priority as usize) };

    if err != 0 {
        return Err(Error::TCBSetPriority(err));
    }

    let err = unsafe { seL4_TCB_Resume(tcb.cptr) };

    if err != 0 {
        return Err(Error::TCBResume(err));
    }

    Ok(local_cnode)
}
