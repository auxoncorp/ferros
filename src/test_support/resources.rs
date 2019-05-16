use crate::userland::*;
use selfe_sys::seL4_BootInfo;
use typenum::*;

pub struct Resources {
    pub(super) slots: LocalCNodeSlots<super::types::MaxTestCNodeSlots>,
    pub(super) untyped: LocalCap<Untyped<super::types::MaxTestUntypedSize>>,
    pub(super) asid_pool: LocalCap<ASIDPool<super::types::MaxTestASIDPoolSize>>,
    pub(super) scratch: VSpaceScratchSlice<role::Local>,
    pub(super) cnode: LocalCap<LocalCNode>,
    pub(super) thread_authority: LocalCap<ThreadPriorityAuthority>,
    pub(super) user_image: UserImage<role::Local>,
}

pub struct TestResourceRefs<'t> {
    pub(super) slots: &'t mut LocalCNodeSlots<super::types::MaxTestCNodeSlots>,
    pub(super) untyped: &'t mut LocalCap<Untyped<super::types::MaxTestUntypedSize>>,
    pub(super) asid_pool: &'t mut LocalCap<ASIDPool<super::types::MaxTestASIDPoolSize>>,
    pub(super) scratch: &'t mut VSpaceScratchSlice<role::Local>,
    pub(super) cnode: &'t LocalCap<LocalCNode>,
    pub(super) thread_authority: &'t LocalCap<ThreadPriorityAuthority>,
    pub(super) user_image: &'t UserImage<role::Local>,
}
impl Resources {
    pub fn with_debug_reporting(
        raw_boot_info: &'static seL4_BootInfo,
    ) -> Result<(Self, impl super::TestReporter), super::TestSetupError> {
        let BootInfo {
            root_page_directory,
            asid_control,
            user_image,
            root_tcb,
            ..
        } = BootInfo::wrap(&raw_boot_info);
        let mut allocator = crate::alloc::micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
        let (cnode, local_slots) = root_cnode(&raw_boot_info);
        let ut_for_scratch = allocator
            .get_untyped::<<UnmappedPageTable as DirectRetype>::SizeBits>()
            .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound {
                bit_size: <UnmappedPageTable as DirectRetype>::SizeBits::USIZE,
            })?;
        let (scratch_slots, local_slots) = local_slots.alloc();
        let (scratch, _root_page_directory) =
            VSpaceScratchSlice::from_parts(scratch_slots, ut_for_scratch, root_page_directory)?;
        let (asid_pool_slots, local_slots) = local_slots.alloc();
        let ut_for_asid_pool = allocator
            .get_untyped::<U12>()
            .ok_or_else(|| super::TestSetupError::InitialUntypedNotFound { bit_size: 12 })?;
        let (asid_pool, _asid_control) =
            asid_control.allocate_asid_pool(ut_for_asid_pool, asid_pool_slots)?;
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
                scratch,
                cnode,
                thread_authority: root_tcb.downgrade_to_thread_priority_authority(),
                user_image,
            },
            crate::debug::DebugOutHandle,
        ))
    }

    pub fn as_mut_ref(&'_ mut self) -> TestResourceRefs<'_> {
        TestResourceRefs {
            slots: &mut self.slots,
            untyped: &mut self.untyped,
            asid_pool: &mut self.asid_pool,
            scratch: &mut self.scratch,
            cnode: &self.cnode,
            thread_authority: &self.thread_authority,
            user_image: &self.user_image,
        }
    }
}
