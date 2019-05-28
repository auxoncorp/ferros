use core::marker::PhantomData;

use crate::cap::{CNodeRole, CapType};

struct PageDirectory<Role: CNodeRole> {
    _role: PhantomData<Role>,
}

impl<Role: CNodeRole> CapType for PageDirectory {}
