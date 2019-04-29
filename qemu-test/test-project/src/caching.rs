use super::TopLevelError;

use core::ptr;
use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::userland::{
    retype, retype_cnode, role, root_cnode, yield_forever, BootInfo, CapRights, LocalCap,
    RetypeForSetup, Untyped, VSpace,
};
use ferros_hal::dma_cache_op::{DmaCacheOp, DmaCacheOpExt};
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

        // Map two pages to a single memory region
        // The first page will be mapped without cacheability attributes,
        // and the second page (pagec) will be mapped with them
        let unmapped_page = retype(ut, slots)?;
        let unmapped_pagec = unmapped_page
            .copy(&root_cnode, slots, CapRights::RW)?;

        let (mapped_page, proc_vspace) = proc_vspace.map_dma_page(unmapped_page)?;
        let (mapped_pagec, proc_vspace) = proc_vspace.map_page(unmapped_pagec)?;

        let page_vaddr = mapped_page.virtual_address();
        let page_paddr = mapped_page.physical_address()?;

        let pagec_vaddr = mapped_pagec.virtual_address();
        let pagec_paddr = mapped_pagec.physical_address()?;

        debug_println!("Page vaddr {:#010X} paddr {:#010X}", page_vaddr, page_paddr);
        debug_println!("PageC vaddr {:#010X} paddr {:#010X}", pagec_vaddr, pagec_paddr);

        let proc_params = ProcParams {
            // TODO - this won't work yet
            // likley will evolve into a CapRange for the pages
            //cache_op_token: proc_vspace.leak_page_dir_cap(),
            cache_op_token: 0,
            page_vaddr,
            page_paddr,
            pagec_vaddr,
            pagec_paddr,
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

pub struct ProcParams {
    pub cache_op_token: usize,
    // Page without cacheability attributes
    pub page_vaddr: usize,
    pub page_paddr: usize,
    // Page with cacheability attributes
    pub pagec_vaddr: usize,
    pub pagec_paddr: usize,
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

pub extern "C" fn test_page_directory_flush(p: ProcParams) {
    // NOTE: until VSpace gives typenums, this has an assumption
    // on the vaddr/paddr being constant, this might break if
    // the underlying device tree, memory split, or ut ordering changes

    // Both pages are mapped to the same region, so they
    // should have the same paddr and size
    assert_eq!(p.page_paddr, ExpectedPaddr::USIZE);
    assert_eq!(p.pagec_paddr, ExpectedPaddr::USIZE);

    // They should be mapped contiguously, so vaddr is offset by a page
    type PageCVaddr = Sum<ExpectedVaddr, PageSize>;
    assert_eq!(p.page_vaddr, ExpectedVaddr::USIZE);
    assert_eq!(p.pagec_vaddr, PageCVaddr::USIZE);

    // Create two memory regions over the pages
    let mut mem = MemoryRegion::new::<ExpectedVaddr, ExpectedPaddr, PageSize>();
    let mut memc =
        MemoryRegion::new_with_token::<PageCVaddr, ExpectedPaddr, PageSize>(p.cache_op_token);

    debug_println!("Non-cache mem:\n{}", mem);
    debug_println!("Cacheable mem:\n{}", memc);

    // Clean makes data observable to non-cached page
    unsafe { ptr::write_volatile(mem.as_mut_ptr::<u32>().unwrap(), 0xC0FFEE) };
    unsafe { ptr::write_volatile(memc.as_mut_ptr::<u32>().unwrap(), 0xDEADBEEF) };
    assert_eq!(
        unsafe { ptr::read_volatile(mem.as_mut_ptr::<u32>().unwrap()) },
        0xC0FFEE
    );
    assert_eq!(
        unsafe { ptr::read_volatile(memc.as_mut_ptr::<u32>().unwrap()) },
        0xDEADBEEF
    );
    memc.dma_cache_op(DmaCacheOp::Clean, memc.vaddr(), memc.size())
        .unwrap();
    assert_eq!(
        unsafe { ptr::read_volatile(mem.as_mut_ptr::<u32>().unwrap()) },
        0xDEADBEEF
    );
    assert_eq!(
        unsafe { ptr::read_volatile(memc.as_mut_ptr::<u32>().unwrap()) },
        0xDEADBEEF
    );

    // TODO

    debug_println!("All done");

    unsafe { yield_forever() };
}
