use typenum::consts::U256;
use typenum::{IsLess, True, Unsigned};

use registers::{ReadOnlyRegister, WriteOnlyRegister};

use crate::userland::{role, CNodeRole, InterruptConsumer, RetypeForSetup};

pub struct UartParams<IRQ: Unsigned + Sync + Send, Role: CNodeRole>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub base_ptr: usize,
    pub consumer: InterruptConsumer<IRQ, Role>,
}

impl<IRQ: Unsigned + Sync + Send> RetypeForSetup for UartParams<IRQ, role::Local>
where
    IRQ: IsLess<U256, Output = True>,
{
    type Output = UartParams<IRQ, role::Child>;
}
