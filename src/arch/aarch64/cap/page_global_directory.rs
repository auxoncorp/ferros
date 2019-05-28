use core::marker::PhantomData;

use crate::cap::{CNodeRole, CapType};

struct PageGlobalDirectory<Role: CNodeRole> {
    _role: PhantomData<Role>,
}

impl<Role: CNodeRole> CapType for PageGlobalDirectory {}
