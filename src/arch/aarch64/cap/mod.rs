use selfe_sys::seL4_ARM_Page_Unmap;

use crate::error::SeL4Error;

mod asid;
mod asid_control;
mod huge_page;
mod large_page;
mod page;
mod page_directory;
mod page_global_directory;
mod page_table;
mod page_upper_directory;
#[cfg(KernelArmHypervisorSupport)]
mod vcpu;

pub use asid::*;
pub use asid_control::*;
pub use huge_page::*;
pub use large_page::*;
pub use page::*;
pub use page_directory::*;
pub use page_global_directory::*;
pub use page_table::*;
pub use page_upper_directory::*;
#[cfg(KernelArmHypervisorSupport)]
pub use vcpu::*;

pub trait UnmappablePage: private::SealedUnmappablePage {
    fn unmap(self) -> Result<(), SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(()),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
        }
    }
}

mod private {
    use crate::cap::LocalCap;

    pub trait SealedUnmappablePage {}

    impl SealedUnmappablePage for LocalCap<super::LargePage> {}
    impl SealedUnmappablePage for LocalCap<super::HugePage> {}
}
