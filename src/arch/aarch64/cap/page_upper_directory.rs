use core::marker::PhantomData;

use crate::cap::{CNodeRole, CapType};

struct PageUpperDirectory<Role: CNodeRole> {
    _role: PhantomData<Role>,
}

impl<Role: CNodeRole> CapType for PageUpperDirectory {}
