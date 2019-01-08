/// TODO - need a proper VKA abstration and implementation
use super::{Allocator, Error};
use super::cspacepath::CSpacePath;
use sel4_sys::*;

impl Allocator {
    /// Get the size (in bits) of the untyped memory required to create an
    /// object of the given size.
    ///
    /// TODO - see vka/object.h, not handling all cases yet (feature gating for
    /// RT/etc)
    /// TODO - move this once vka_object/vka works
    /// https://github.com/seL4/seL4_libs/blob/c7e4a85edc3048979fbc962ace2dbe657e8d0b3c/libsel4vka/arch_include/arm/vka/arch/kobject_t.h#L50
    pub fn vka_get_object_size(&self, obj_type: seL4_Word, obj_size_bits: usize) -> usize {
        #[allow(non_upper_case_globals)]
        match obj_type {
            api_object_seL4_UntypedObject => obj_size_bits as _,
            api_object_seL4_TCBObject => seL4_TCBBits as _,
            api_object_seL4_EndpointObject => seL4_EndpointBits as _,
            api_object_seL4_NotificationObject => seL4_NotificationBits as _,
            api_object_seL4_CapTableObject => (seL4_SlotBits as usize + obj_size_bits),
            //seL4_KernelImageObject => seL4_KernelImageBits,
            _ => self.vka_arch_get_object_size(obj_type),
        }
    }

    /// Get the size (in bits) of the untyped memory required to create an
    /// object of the given size.
    /// TODO - feature gate for arm, SMMU
    pub fn vka_arch_get_object_size(&self, obj_type: seL4_Word) -> usize {
        #[allow(non_upper_case_globals)]
        match obj_type {
            _object_seL4_ARM_SmallPageObject => seL4_PageBits as _,
            _object_seL4_ARM_LargePageObject => seL4_LargePageBits as _,
            _object_seL4_ARM_PageTableObject => seL4_PageTableBits as _,
            _object_seL4_ARM_PageDirectoryObject => seL4_PageDirBits as _,
            _ => self.vka_arm_mode_get_object_size(obj_type),
        }
    }

    /// Get the size (in bits) of the untyped memory required to create an
    /// object of the given size.
    /// TODO - feature gate for aarch32/aarch64
    pub fn vka_arm_mode_get_object_size(&self, obj_type: seL4_Word) -> usize {
        #[allow(non_upper_case_globals)]
        match obj_type {
            _object_seL4_ARM_SectionObject => seL4_SectionBits as _,
            _object_seL4_ARM_SuperSectionObject => seL4_SuperSectionBits as _,
            seL4_ARM_VCPUObject => seL4_ARM_VCPUBits as _,
            _ => panic!("Unknown object type"),
        }
    }

    pub fn vka_cspace_alloc(&mut self) -> Result<seL4_CPtr, Error> {
        self.alloc_cslot()
    }

    pub fn vka_cspace_free(&mut self, slot: seL4_CPtr) {
        self.free_cslot(slot)
    }

    pub fn vka_cspace_make_path(&self, slot: seL4_CPtr) -> CSpacePath {
        CSpacePath {
            cap_ptr: slot,
            cap_depth: 32,
            root: self.root_cnode,
            dest: self.root_cnode,
            dest_depth: self.root_cnode_depth,
            offset: slot,
            window: 1,
        }
    }

    pub fn vka_utspace_alloc(
        &mut self,
        dest: &CSpacePath,
        item_type: seL4_Word,
        size_bits: usize,
    ) -> Result<seL4_CPtr, Error> {
        self.utspace_alloc(dest, item_type, size_bits, None, false)
    }

    pub fn vka_utspace_alloc_at(
        &mut self,
        dest: &CSpacePath,
        item_type: seL4_Word,
        size_bits: usize,
        paddr: seL4_Word,
        can_use_dev: bool,
    ) -> Result<seL4_CPtr, Error> {
        self.utspace_alloc(dest, item_type, size_bits, Some(paddr), can_use_dev)
    }

    fn utspace_alloc(
        &mut self,
        dest: &CSpacePath,
        item_type: seL4_Word,
        size_bits: usize,
        paddr: Option<seL4_Word>,
        can_use_dev: bool,
    ) -> Result<seL4_CPtr, Error> {
        let ut_size_bits = self.vka_get_object_size(item_type, size_bits);

        // allocate untyped memory the size we want
        let untyped_memory = self.alloc_untyped(ut_size_bits, paddr, can_use_dev)?;

        let err = unsafe {
            seL4_Untyped_Retype(
                untyped_memory,
                item_type,
                size_bits as _,
                seL4_CapInitThreadCNode,
                self.root_cnode,
                self.root_cnode_depth,
                dest.cap_ptr,
                1,
            )
        };

        if err == 0 {
            Ok(untyped_memory)
        } else {
            Err(Error::ResourceExhausted)
        }
    }
}
