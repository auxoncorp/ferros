use core::marker::PhantomData;

use selfe_sys::*;

use crate::cap::{Cap, CapType, DirectRetype, LocalCap, PhantomCap, ThreadControlBlock};
use crate::error::{ErrorExt, SeL4Error};

#[derive(Debug, Clone, Copy)]
pub enum VCpuRegister {
    // VM control registers EL1
    Sctlr = 0,
    Ttbr0 = 1,
    Ttbr1 = 2,
    Tcr = 3,
    Mair = 4,
    Amair = 5,
    Cidr = 6,

    // Other system registers EL1
    Actlr = 7,
    Cpacr = 8,

    // Exception handling registers EL1
    Afsr0 = 9,
    Afsr1 = 10,
    Esr = 11,
    Far = 12,
    Isr = 13,
    Vbar = 14,

    // Thread pointer/ID registers EL0/EL1
    TpidrEl0 = 15,
    TpidrEl1 = 16,
    TpidrroEl0 = 17,

    // Generic timer registers
    CntvCtl = 18,
    CntvTval = 19,
    CntvCval = 20,

    // General registers
    SpEl1 = 21,
    ElrEl1 = 22,
    SpsrEl1 = 23,
}

pub trait VCpuState: private::SealedVCpuState {}

pub mod vcpu_state {
    pub struct Bound;
    impl super::VCpuState for Bound {}

    pub struct Unbound;
    impl super::VCpuState for Unbound {}
}

#[derive(Debug)]
pub struct VCpu<State: VCpuState> {
    _state: PhantomData<State>,
}

impl LocalCap<VCpu<vcpu_state::Unbound>> {
    /// Bind a TCB to a virtual CPU.
    ///
    /// This consumes the unbound VCpu, returning
    /// a bound VCpu upon success.
    pub fn bind_tcb(
        self,
        tcb: &mut LocalCap<ThreadControlBlock>,
    ) -> Result<LocalCap<VCpu<vcpu_state::Bound>>, SeL4Error> {
        match unsafe { seL4_ARM_VCPU_SetTCB(self.cptr, tcb.cptr) }.as_result() {
            Ok(_) => Ok(Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: VCpu {
                    _state: PhantomData,
                },
            }),
            Err(e) => Err(SeL4Error::VCPUBindTcb(e)),
        }
    }
}

impl LocalCap<VCpu<vcpu_state::Bound>> {
    /// Inject an IRQ to a virtual CPU.
    pub fn inject_irq(
        &mut self,
        virq: u16,
        priority: u8,
        group: u8,
        index: u8,
    ) -> Result<(), SeL4Error> {
        match unsafe { seL4_ARM_VCPU_InjectIRQ(self.cptr, virq, priority, group, index) }
            .as_result()
        {
            Ok(_) => Ok(()),
            Err(e) => Err(SeL4Error::VCPUInjectIRQ(e)),
        }
    }

    /// Read a virtual CPU register.
    pub fn read_register(&self, reg: VCpuRegister) -> Result<usize, SeL4Error> {
        let result: seL4_ARM_VCPU_ReadRegs_t =
            unsafe { seL4_ARM_VCPU_ReadRegs(self.cptr, reg as usize) };

        // NOTE: error field is declared as int, but when inspecting
        // the kernel source we see it's populated with `seL4_MessageInfo_get_label()`
        // so we cast to unsigned/u32 like the other errors
        match (result.error as u32).as_result() {
            Ok(_) => Ok(result.value),
            Err(e) => Err(SeL4Error::VCPUReadRegisters(e)),
        }
    }

    /// Write a virtual CPU register.
    pub fn write_register(&mut self, reg: VCpuRegister, value: usize) -> Result<(), SeL4Error> {
        match unsafe { seL4_ARM_VCPU_WriteRegs(self.cptr, reg as usize, value) }.as_result() {
            Ok(_) => Ok(()),
            Err(e) => Err(SeL4Error::VCPUWriteRegisters(e)),
        }
    }
}

impl<State: VCpuState> CapType for VCpu<State> {}

impl DirectRetype for VCpu<vcpu_state::Unbound> {
    type SizeBits = super::super::ARMVCPUBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_VCPUObject as usize
    }
}

impl PhantomCap for VCpu<vcpu_state::Unbound> {
    fn phantom_instance() -> Self {
        Self {
            _state: PhantomData,
        }
    }
}

mod private {
    pub trait SealedVCpuState {}
    impl SealedVCpuState for super::vcpu_state::Unbound {}
    impl SealedVCpuState for super::vcpu_state::Bound {}
}
