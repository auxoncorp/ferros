use super::TopLevelError;

use core::ptr;
use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::userland::{
    retype, retype_cnode, role, root_cnode, BootInfo, CNode, CNodeRole, CNodeSlots, CNodeSlotsData,
    Cap, Endpoint, LocalCap, RetypeForSetup, SeL4Error, UnmappedPageTable, Untyped, VSpace,
};
use ferros_hal::memory_region::MemoryRegion;
use selfe_sys::*;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    let ut27 = allocator
        .get_untyped::<U27>()
        .expect("initial alloc failure");
    let uts = alloc::ut_buddy(ut27);

    smart_alloc!(|slots from local_slots, ut from uts| {
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        let unmapped_scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, boot_info) =
            boot_info.map_page_table(unmapped_scratch_page_table)?;

        let (proc_cnode, proc_slots) = retype_cnode::<U12>(ut, slots)?;
        let (proc_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;

        // Carve off 2 pages of memory
        let unmapped_page_a = retype(ut, slots)?;
        let (page_a, proc_vspace) = proc_vspace.map_page(unmapped_page_a)?;
        let unmapped_page_b = retype(ut, slots)?;
        let (page_b, proc_vspace) = proc_vspace.map_page(unmapped_page_b)?;

        let page_a_vaddr = page_a.virtual_address();
        let page_a_paddr = page_a.physical_address()?;

        let page_b_vaddr = page_b.virtual_address();
        let page_b_paddr = page_b.physical_address()?;

        debug_println!("Page A vaddr {:#010X} paddr {:#010X}", page_a_vaddr, page_a_paddr);
        debug_println!("Page B vaddr {:#010X} paddr {:#010X}", page_b_vaddr, page_b_paddr);

        let proc_params = ProcParams {
            page_a_vaddr,
            page_a_paddr,
            page_b_vaddr,
            page_b_paddr,
        };

        let (proc_thread, _) = proc_vspace.prepare_thread(
            test_page_directory_flush,
            proc_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        proc_thread.start(proc_cnode, None, &boot_info.tcb, 255)?;
    });

    Ok(())
}

// 0x0400_0000 = 67,108,864 = 2^26
type ExpectedVaddr = U67108864;
// 0x1800_2000 = 402,661,376 = 2^28 + 2^27 + 2^13
type ExpectedPaddr = Sum<Sum<U268435456, U134217728>, U8192>;

/// 4K page
type PageSize = U4096;

/// Two pages in the pool
type MemPoolSize = Sum<PageSize, PageSize>;

pub struct ProcParams {
    pub page_a_vaddr: usize,
    pub page_a_paddr: usize,
    pub page_b_vaddr: usize,
    pub page_b_paddr: usize,
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

pub extern "C" fn test_page_directory_flush(p: ProcParams) {
    // NOTE: until VSpace gives typenums, this has an assumption
    // on the vaddr/paddr being constant, this might break if
    // the underlying device tree, memory split, or ut ordering changes

    assert_eq!(p.page_a_vaddr, ExpectedVaddr::USIZE);
    assert_eq!(p.page_a_paddr, ExpectedPaddr::USIZE);

    type PageBVaddr = Sum<ExpectedVaddr, PageSize>;
    type PageBPaddr = Sum<ExpectedPaddr, PageSize>;
    assert_eq!(p.page_b_vaddr, PageBVaddr::USIZE);
    assert_eq!(p.page_b_paddr, PageBPaddr::USIZE);

    // Merge the two pages into a single MemoryRegion
    let mem_pool = MemoryRegion::new::<ExpectedVaddr, ExpectedPaddr, MemPoolSize>();

    debug_println!("{}", mem_pool);

    // Split the pool into two regions
    let (mut mem_a, mut mem_b) = mem_pool.split_off::<PageSize>();

    // Clean makes data observable to non-cached page
    unsafe { ptr::write_volatile(mem_a.as_mut_ptr::<u32>().unwrap(), 0xC0FFEE) };
    unsafe { ptr::write_volatile(mem_b.as_mut_ptr::<u32>().unwrap(), 0xDEADBEEF) };
    assert_eq!(
        unsafe { ptr::read_volatile(mem_a.as_mut_ptr::<u32>().unwrap()) },
        0xC0FFEE
    );
    assert_eq!(
        unsafe { ptr::read_volatile(mem_b.as_mut_ptr::<u32>().unwrap()) },
        0xDEADBEEF
    );

    debug_println!("All done");

    loop {
        // TODO
    }
}
