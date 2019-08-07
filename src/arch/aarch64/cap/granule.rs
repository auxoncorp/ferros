use selfe_sys::*;

use crate::cap::{granule_state, Granule, LocalCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

impl LocalCap<Granule<granule_state::Unmapped>> {
    pub(crate) unsafe fn unchecked_map(
        &self,
        addr: usize,
        root: &mut LocalCap<crate::arch::PagingRoot>,
        rights: CapRights,
        vm_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), SeL4Error> {
        seL4_ARM_Page_Map(
            self.cptr,
            root.cptr,
            addr,
            seL4_CapRights_t::from(rights),
            vm_attributes,
        )
        .as_result()
        .map_err(|e| SeL4Error::PageMap(e))
    }
}

impl LocalCap<Granule<granule_state::Mapped>> {
    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Granule<granule_state::Unmapped>>, SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Granule {
                    type_id: self.cap_data.type_id,
                    size_bits: self.cap_data.size_bits,
                    state: granule_state::Unmapped {},
                },
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
        }
    }
}

impl DirectRetype for Granule<State
