use selfe_sys::seL4_BootInfo;
use typenum::*;

use crate::arch;
use crate::arch::cap::*;
use crate::bootstrap::*;
use crate::cap::*;
use crate::test_support::MaxMappedMemoryRegionBitSize;
use crate::vspace::*;

pub struct Resources {
    pub(super) slots: LocalCNodeSlots<super::types::MaxTestCNodeSlots>,
    pub(super) untyped: LocalCap<Untyped<super::types::MaxTestUntypedSize>>,
    pub(super) asid_pool: LocalCap<ASIDPool<super::types::MaxTestASIDPoolSize>>,
    pub(super) vspace: VSpace<vspace_state::Imaged, role::Local, vspace_mapping_mode::Auto>,
    pub(super) reserved_for_scratch:
        ReservedRegion<crate::userland::process::DefaultStackPageCount>,
    pub(super) mapped_memory_region: MappedMemoryRegion<
        super::types::MaxMappedMemoryRegionBitSize,
        crate::vspace::shared_status::Exclusive,
    >,
    pub(super) cnode: LocalCap<LocalCNode>,
    pub(super) thread_authority: LocalCap<ThreadPriorityAuthority>,
    pub(super) user_image: UserImage<role::Local>,
    pub(super) irq_control: LocalCap<IRQControl>,
}

pub struct TestResourceRefs<'t> {
    pub(super) slots: &'t mut LocalCNodeSlots<super::types::MaxTestCNodeSlots>,
    pub(super) untyped: &'t mut LocalCap<Untyped<super::types::MaxTestUntypedSize>>,
    pub(super) asid_pool: &'t mut LocalCap<ASIDPool<super::types::MaxTestASIDPoolSize>>,
    pub(super) scratch: ScratchRegion<'t, 't, crate::userland::process::DefaultStackPageCount>,
    pub(super) mapped_memory_region: &'t mut MappedMemoryRegion<
        super::types::MaxMappedMemoryRegionBitSize,
        crate::vspace::shared_status::Exclusive,
    >,
    pub(super) cnode: &'t LocalCap<LocalCNode>,
    pub(super) thread_authority: &'t LocalCap<ThreadPriorityAuthority>,
    pub(super) user_image: &'t UserImage<role::Local>,
    pub(super) irq_control: &'t mut LocalCap<IRQControl>,
}

type PageFallbackNextSize = Sum<U1, <Page<page_state::Unmapped> as DirectRetype>::SizeBits>;
type MappedMemoryRegionFallbackNextSize = Sum<U1, MaxMappedMemoryRegionBitSize>;

impl Resources {
    pub fn with_debug_reporting(
        raw_boot_info: &'static seL4_BootInfo,
        mut allocator: crate::alloc::micro_alloc::Allocator,
    ) -> Result<(Self, impl super::TestReporter), super::TestSetupError> {
        let (cnode, local_slots) = root_cnode(&raw_boot_info);
        // TODO - Refine sizes of VSpace untyped and slots
        let (vspace_slots, local_slots): (crate::cap::LocalCNodeSlots<U4096>, _) =
            local_slots.alloc();
        let BootInfo {
            root_vspace,
            asid_control,
            user_image,
            root_tcb,
            irq_control,
            ..
        } = BootInfo::wrap(
            &raw_boot_info,
            allocator
                .get_untyped::<U14>()
                .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound { bit_size: 14 })?,
            vspace_slots,
        );
        let mut root_vspace = root_vspace.to_auto();
        let (extra_scratch_slots, local_slots) = local_slots.alloc();
        let ut_for_scratch = {
            match allocator.get_untyped::<<Page<page_state::Unmapped> as DirectRetype>::SizeBits>()
            {
                Some(v) => v,
                None => {
                    let ut_plus_one =
                        allocator
                            .get_untyped::<PageFallbackNextSize>()
                            .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound {
                                bit_size: PageFallbackNextSize::USIZE,
                            })?;
                    let (ut_page, _) = ut_plus_one.split(extra_scratch_slots)?;
                    ut_page
                }
            }
        };
        let (scratch_slots, local_slots) = local_slots.alloc();
        let sacrificial_page = ut_for_scratch.retype(scratch_slots)?;
        let reserved_for_scratch = root_vspace.reserve(sacrificial_page)?;
        let (asid_pool_slots, local_slots) = local_slots.alloc();
        let (extra_pool_slots, local_slots) = local_slots.alloc();
        let ut_for_asid_pool = {
            match allocator.get_untyped::<U12>() {
                Some(v) => v,
                None => {
                    let ut13 = allocator.get_untyped::<U13>().ok_or_else(|| {
                        super::TestSetupError::InitialUntypedNotFound { bit_size: 13 }
                    })?;
                    let (ut12, _) = ut13.split(extra_pool_slots)?;
                    ut12
                }
            }
        };
        let (asid_pool, _asid_control) =
            asid_control.allocate_asid_pool(ut_for_asid_pool, asid_pool_slots)?;

        let (extra_memory_slots, local_slots) = local_slots.alloc();
        let memory_region_ut = match allocator.get_untyped::<MaxMappedMemoryRegionBitSize>() {
            Some(v) => v,
            None => {
                let ut_fallback = allocator
                    .get_untyped::<MappedMemoryRegionFallbackNextSize>()
                    .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound {
                        bit_size: MappedMemoryRegionFallbackNextSize::USIZE,
                    })?;
                let (ut_target, _) = ut_fallback.split(extra_memory_slots)?;
                ut_target
            }
        };

        let (memory_region_slots, local_slots) = local_slots.alloc();
        let unmapped_region: UnmappedMemoryRegion<
            MaxMappedMemoryRegionBitSize,
            shared_status::Exclusive,
        > = UnmappedMemoryRegion::new(memory_region_ut, memory_region_slots)?;
        let mapped_memory_region = root_vspace.map_region(
            unmapped_region,
            crate::userland::CapRights::RW,
            arch::vm_attributes::DEFAULT,
        )?;
        let (slots, _local_slots) = local_slots.alloc();
        Ok((
            Resources {
                slots,
                untyped: allocator
                    .get_untyped::<super::types::MaxTestUntypedSize>()
                    .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound {
                        bit_size: super::types::MaxTestUntypedSize::USIZE,
                    })?,
                asid_pool,
                vspace: root_vspace,
                reserved_for_scratch,
                mapped_memory_region,
                cnode,
                thread_authority: root_tcb.downgrade_to_thread_priority_authority(),
                user_image,
                irq_control,
            },
            crate::debug::DebugOutHandle,
        ))
    }

    pub fn as_mut_ref(&'_ mut self) -> TestResourceRefs<'_> {
        TestResourceRefs {
            slots: &mut self.slots,
            untyped: &mut self.untyped,
            asid_pool: &mut self.asid_pool,
            scratch: self
                .reserved_for_scratch
                .as_scratch(&mut self.vspace)
                .expect("Failed to use root VSpace in combination with the reserved region, likely an ASID mismatch."),
            mapped_memory_region: &mut self.mapped_memory_region,
            cnode: &self.cnode,
            thread_authority: &self.thread_authority,
            user_image: &self.user_image,
            irq_control: &mut self.irq_control
        }
    }
}
