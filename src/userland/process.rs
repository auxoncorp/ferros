use core::cmp;
use core::marker::PhantomData;
use core::mem::{self, size_of};
use core::ops::Sub;
use core::ptr;
use crate::userland::{
    address_space, paging, role, ASIDPool, AssignedPageDirectory, BootInfo, CNode, Cap, CapRights,
    FaultSource, LocalCap, MappedPage, MappedPageTable, SeL4Error, ThreadControlBlock,
    UnassignedPageDirectory, UnmappedPage, UnmappedPageTable, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U128, U16, U256};

impl Cap<ThreadControlBlock, role::Local> {
    fn configure<CNodeFreeSlots: Unsigned, PageDirFreeSlots: Unsigned>(
        &mut self,
        cspace_root: LocalCap<CNode<CNodeFreeSlots, role::Child>>,
        fault_source: Option<FaultSource<role::Child>>,
        // cspace_root_data: usize, // set the guard bits here
        // TODO make a marker trait for VSpace?
        vspace_root: LocalCap<AssignedPageDirectory<PageDirFreeSlots>>,
        // vspace_root_data: usize, // always 0
        ipc_buffer_addr: usize,
        ipc_buffer: LocalCap<MappedPage>,
    ) -> Result<(), SeL4Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                                   // guard
                seL4_WordBits - cspace_root.cap_data.radix as usize, // guard size in bits
            )
        }
        .words[0];

        let tcb_err = unsafe {
            seL4_TCB_Configure(
                self.cptr,
                fault_source.map_or(seL4_CapNull as usize, |source| source.endpoint.cptr), // fault_ep.cptr,
                cspace_root.cptr,
                cspace_root_data,
                vspace_root.cptr,
                seL4_NilData as usize,
                ipc_buffer_addr, // buffer address
                ipc_buffer.cptr, // bufferFrame capability
            )
        };

        if tcb_err != 0 {
            Err(SeL4Error::TCBConfigure(tcb_err))
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

// TODO - consider renaming for clarity
pub trait RetypeForSetup: Sized {
    type Output;
}

type SetupVer<X> = <X as RetypeForSetup>::Output;

pub fn spawn<
    T: RetypeForSetup,
    FreeSlots: Unsigned,
    RootCNodeFreeSlots: Unsigned,
    PageDirFreeSlots: Unsigned,
    ScratchPageTableSlots: Unsigned,
>(
    // process-related
    function_descriptor: extern "C" fn(T) -> (),
    process_parameter: SetupVer<T>,
    child_cnode: LocalCap<CNode<RootCNodeFreeSlots, role::Child>>,
    priority: u8,
    fault_source: Option<FaultSource<role::Child>>,

    // context-related
    ut16: LocalCap<Untyped<U16>>,
    boot_info: &mut BootInfo<PageDirFreeSlots>,
    scratch_page_table: &mut LocalCap<MappedPageTable<ScratchPageTableSlots>>,
    local_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
) -> Result<LocalCap<CNode<Diff<FreeSlots, U256>, role::Local>>, SeL4Error>
where
    FreeSlots: Sub<U256>,
    Diff<FreeSlots, U256>: Unsigned,
    T: core::marker::Sized,
    ScratchPageTableSlots: Sub<B1>,
    Sub1<ScratchPageTableSlots>: Unsigned,
{
    // TODO can we somehow make this a static assertion? both of these should be const
    assert!(size_of::<SetupVer<T>>() == size_of::<T>());

    // this significantly cleans up the type constraints above
    let (cnode, local_cnode) = local_cnode.reserve_region::<U256>();

    let (ut14, page_dir_ut, ut14b, _, cnode) = ut16.quarter(cnode)?;

    // 12 bits
    let (ut12, stack_page_ut, _, _, cnode) = ut14.quarter(cnode)?;
    let (ut12b, ipc_buffer_ut, _, _, cnode) = ut14b.quarter(cnode)?;

    // 10 bits
    let (ut10, stack_page_table_ut, code_page_table_ut, tcb_ut, cnode) = ut12.quarter(cnode)?;
    let (ipc_buffer_page_table_ut, _, _, _, cnode) = ut12b.quarter(cnode)?;

    let (ut8, _, _, _, cnode) = ut10.quarter(cnode)?;
    let (ut6, _, _, _, cnode) = ut8.quarter(cnode)?;
    let (_, _, _, _, cnode) = ut6.quarter(cnode)?;

    // Process address space layout
    let stack_base = 0x10000000;
    let stack_top = stack_base + 0x1000;
    let ipc_buffer_addr = stack_base - 0x2000; // this must be 512-byte aligned, per the seL4 manual

    let (code_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
        code_page_table_ut.retype_local(cnode)?;

    let (page_dir, cnode): (Cap<UnassignedPageDirectory, _>, _) =
        page_dir_ut.retype_local(cnode)?;

    let (code_page_table, mut page_dir) = boot_info.asid_pool.assign(code_page_table, page_dir)?;

    // set up ipc buffer
    let (ipc_buffer_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
        ipc_buffer_page_table_ut.retype_local(cnode)?;
    let (ipc_buffer_page, cnode): (Cap<UnmappedPage, _>, _) = ipc_buffer_ut.retype_local(cnode)?;

    let (ipc_buffer_page_table, mut page_dir) = page_dir.map_page_table(ipc_buffer_page_table)?;
    let (ipc_buffer_page, ipc_buffer_page_table) =
        ipc_buffer_page_table.map_page(ipc_buffer_page, &mut page_dir)?;

    // Set up a single page for the child's stack (4k)
    // let (stack_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
    //     stack_page_table_ut.retype_local(cnode)?;
    // let (stack_page, cnode): (Cap<UnmappedPage, _>, _) = stack_page_ut.retype_local(cnode)?;

    // map the child stack into local memory so we can set it up

    // borrow_page maps a page temporarily
    let (stack_page, cnode): (Cap<UnmappedPage, _>, _) = stack_page_ut.retype_local(cnode)?;
    let (mut regs, stack_page) = scratch_page_table.temporarily_map_page(
        stack_page,
        &mut boot_info.page_directory,
        |mapped_page| unsafe {
            setup_initial_stack_and_regs(
                &process_parameter as *const SetupVer<T> as *const usize,
                size_of::<SetupVer<T>>(),
                (mapped_page.cap_data.vaddr + 1 << paging::PageBits::USIZE) as *mut usize,
            )
        },
    )?;

    // let stack_page_table = boot_info
    //     .page_directory
    //     .map_page_table(stack_page_table, stack_base)?;
    // let stack_page = boot_info.page_directory.map_page(stack_page, stack_base)?;

    // let mut regs = unsafe {
    //     setup_initial_stack_and_regs(
    //         &process_parameter as *const SetupVer<T> as *const usize,
    //         size_of::<SetupVer<T>>(),
    //         stack_top as *mut usize,
    //     )
    // };

    // // unmap the stack pages
    // let stack_page = stack_page.unmap()?;
    // let stack_page_table = stack_page_table.unmap()?;

    // map the stack to the target address space
    let (stack_page_table, cnode): (Cap<UnmappedPageTable, _>, _) =
        stack_page_table_ut.retype_local(cnode)?;
    let (stack_page_table, mut page_dir) = page_dir.map_page_table(stack_page_table)?;
    let (stack_page, stack_page_table) = stack_page_table.map_page(stack_page, &mut page_dir)?;

    // map in the user image

    // TODO: the number of pages we reserve here needs to be checked against the
    // size of the binary.
    let (dest_reservation_iter, cnode) = cnode.reservation_iter::<U128>();
    let (code_page_table_reservation_iter, code_page_table) =
        code_page_table.reservation_iter::<U128>();

    for ((page_cap, slot_cnode), slot_page_table) in boot_info
        .user_image_pages_iter()
        .zip(dest_reservation_iter)
        .zip(code_page_table_reservation_iter)
    {
        let (copied_page_cap, _) = page_cap.copy(&local_cnode, slot_cnode, CapRights::W)?;
        let (_slot_page_table, _mapped_page) =
            slot_page_table.map_page(copied_page_cap, &mut page_dir)?;

        // let _mapped_page_cap = page_dir.map_page(copied_page_cap, page_cap.cap_data.vaddr)?;
    }

    let (mut tcb, _cnode): (Cap<ThreadControlBlock, _>, _) = tcb_ut.retype_local(cnode)?;
    tcb.configure(
        child_cnode,
        fault_source,
        page_dir,
        ipc_buffer_addr,
        ipc_buffer_page,
    )?;

    regs.pc = function_descriptor as seL4_Word;
    regs.r14 = (yield_forever as *const fn() -> ()) as seL4_Word;

    let err = unsafe {
        seL4_TCB_WriteRegisters(
            tcb.cptr,
            0,
            0,
            // all the regs
            size_of::<seL4_UserContext>() / size_of::<seL4_Word>(),
            &mut regs,
        )
    };

    if err != 0 {
        return Err(SeL4Error::TCBWriteRegisters(err));
    }

    let err = unsafe { seL4_TCB_SetPriority(tcb.cptr, boot_info.tcb.cptr, priority as usize) };

    if err != 0 {
        return Err(SeL4Error::TCBSetPriority(err));
    }

    let err = unsafe { seL4_TCB_Resume(tcb.cptr) };

    if err != 0 {
        return Err(SeL4Error::TCBResume(err));
    }

    Ok(local_cnode)
}

// This is used in only in spawn
impl Cap<ASIDPool, role::Local> {
    pub fn assign(
        &mut self,
        code_page_table: LocalCap<UnmappedPageTable>,
        vspace: Cap<UnassignedPageDirectory, role::Local>,
    ) -> Result<
        (
            LocalCap<MappedPageTable<paging::BasePageTableFreeSlots>>,
            LocalCap<AssignedPageDirectory<Sub1<paging::BasePageDirFreeSlots>>>,
        ),
        SeL4Error,
    > {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, vspace.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        let page_dir = Cap {
            cptr: vspace.cptr,
            _role: PhantomData,
            cap_data: AssignedPageDirectory::<paging::BasePageDirFreeSlots> {
                next_free_slot: 0,
                _free_slots: PhantomData,
            },
        };

        // Do this immediately after assigning the page directory because it has
        // to be the first page table; that's the portion of the address space
        // where the program expects to find itself.
        //
        // TODO: This limits us to a megabyte of code. If we want to allow more,
        // we need more than one page table here.
        let (code_page_table, page_dir) = page_dir.map_page_table(code_page_table)?;

        Ok((code_page_table, page_dir))
    }
}

/// Set up the target registers and stack to pass the parameter. See
/// http://infocenter.arm.com/help/topic/com.arm.doc.ihi0042f/IHI0042F_aapcs.pdf
/// "Procedure Call Standard for the ARM Architecture", Section 5.5
unsafe fn setup_initial_stack_and_regs(
    param: *const usize,
    param_size: usize,
    stack_top: *mut usize,
) -> seL4_UserContext {
    let word_size = size_of::<usize>();

    // The 'tail' is the part of the parameter that doesn't fit in the
    // word-aligned part.
    let tail_size = param_size % word_size;

    // The parameter must be zero-padded, at the end, to a word boundary
    let padding_size = if tail_size == 0 {
        0
    } else {
        word_size - tail_size
    };
    let padded_param_size = param_size + padding_size;

    // 4 words are stored in registers, so only the remainder needs to go on the
    // stack
    let param_size_on_stack =
        cmp::max(0, padded_param_size as isize - (4 * word_size) as isize) as usize;

    let mut regs: seL4_UserContext = mem::zeroed();
    regs.sp = stack_top as usize;

    // The cursor pointer to traverse the parameter data word one word at a
    // time
    let mut p = param;

    // This is the pointer to the start of the tail.
    let tail = (p as *const u8).add(param_size).sub(tail_size);

    // Compute the tail word ahead of time, for easy use below.
    let mut tail_word = 0usize;
    if tail_size >= 1 {
        tail_word |= *tail.add(0) as usize;
    }

    if tail_size >= 2 {
        tail_word |= (*tail.add(1) as usize) << 8;
    }

    if tail_size >= 3 {
        tail_word |= (*tail.add(2) as usize) << 16;
    }

    // Fill up r0 - r3 with the first 4 words.

    if p < tail as *const usize {
        // If we've got a whole word worth of data, put the whole thing in
        // the register.
        regs.r0 = *p;
        p = p.add(1);
    } else {
        // If not, store the pre-computed tail word here and be done.
        regs.r0 = tail_word;
        return regs;
    }

    if p < tail as *const usize {
        regs.r1 = *p;
        p = p.add(1);
    } else {
        regs.r1 = tail_word;
        return regs;
    }

    if p < tail as *const usize {
        regs.r2 = *p;
        p = p.add(1);
    } else {
        regs.r2 = tail_word;
        return regs;
    }

    if p < tail as *const usize {
        regs.r3 = *p;
        p = p.add(1);
    } else {
        regs.r3 = tail_word;
        return regs;
    }

    // The rest of the data goes on the stack.
    if param_size_on_stack > 0 {
        let sp = (stack_top as *mut u8).sub(param_size_on_stack);
        ptr::copy_nonoverlapping(p as *const u8, sp, param_size_on_stack);

        // TODO: stack pointer is supposed to be 8-byte aligned on ARM 32
        regs.sp = sp as usize;
    }

    regs
}

#[cfg(feature = "test")]
pub mod test {
    use super::*;
    use proptest::test_runner::TestError;

    #[cfg(feature = "test")]
    fn check_equal(name: &str, expected: usize, actual: usize) -> Result<(), TestError<()>> {
        if (expected != actual) {
            Err(TestError::Fail(
                format!(
                    "{} didn't match. Expected: {:08x}, actual: {:08x}",
                    name, expected, actual
                )
                .into(),
                (),
            ))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "test")]
    fn test_stack_setup_case<T: Sized>(
        param: T,
        r0: usize,
        r1: usize,
        r2: usize,
        r3: usize,
        stack0: usize,
        sp_offset: usize,
    ) -> Result<(), TestError<()>> {
        use core::mem::size_of_val;
        let mut fake_stack = [0usize; 1024];

        let param_size = size_of_val(&param);

        let regs = unsafe {
            setup_initial_stack_and_regs(
                &param as *const T as *const usize,
                param_size,
                (&mut fake_stack[0] as *mut usize).add(1024),
            )
        };

        check_equal("r0", r0, regs.r0)?;
        check_equal("r1", r1, regs.r1)?;
        check_equal("r2", r2, regs.r2)?;
        check_equal("r3", r3, regs.r3)?;
        check_equal("top stack word", stack0, fake_stack[1023])?;
        check_equal(
            "sp offset",
            sp_offset,
            unsafe { (&fake_stack[0] as *const usize).add(1024) as usize } - regs.sp,
        )?;

        Ok(())
    }

    #[cfg(feature = "test")]
    #[rustfmt::skip]
    pub fn test_stack_setup() -> Result<(), TestError<()>> {
        test_stack_setup_case(42u8,
                              42, 0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8],
                              2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8],
                              3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8, 5u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                                                           5, // r1
                              0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              8 << 24 | 7 << 16 | 6 << 8 | 5, // r1
                                                           9, // r2
                              0, 0, 0)?;

        test_stack_setup_case([ 1u8,  2u8,  3u8,  4u8,  5u8, 6u8, 7u8, 8u8,
                                9u8, 10u8, 11u8, 12u8, 13u8],
                                4 << 24 |  3 << 16 |  2 << 8 |  1,  // r0
                                8 << 24 |  7 << 16 |  6 << 8 |  5,  // r1
                               12 << 24 | 11 << 16 | 10 << 8 |  9,  // r2
                                                               13,  // r3
                              0, 0)?;

        test_stack_setup_case([ 1u8,  2u8,  3u8,  4u8,  5u8,  6u8,  7u8,  8u8,
                                9u8, 10u8, 11u8, 12u8, 13u8, 14u8, 15u8, 16u8,
                               17u8, 18u8],
                                4 << 24 |  3 << 16 |  2 << 8 |  1,   // r0
                                8 << 24 |  7 << 16 |  6 << 8 |  5,   // r1
                               12 << 24 | 11 << 16 | 10 << 8 |  9,   // r2
                               16 << 24 | 15 << 16 | 14 << 8 | 13,   // r3
                                                     18 << 8 | 17,   // stack top
                                                                4)?; // sp offset

        Ok(())
    }

}
