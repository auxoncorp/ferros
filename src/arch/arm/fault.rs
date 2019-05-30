use crate::cap::Badge;
use crate::userland::MessageInfo;
use selfe_sys::*;

#[derive(Debug)]
pub struct VMFault {
    pub sender: Badge,
    pub program_counter: usize,
    pub address: usize,
    pub is_instruction_fault: bool,
    pub fault_status_register: usize,
}
#[derive(Debug)]
pub struct UnknownSyscall {
    pub sender: Badge,
    pub r0: usize,
    pub r1: usize,
    pub r2: usize,
    pub r3: usize,
    pub r4: usize,
    pub r5: usize,
    pub r6: usize,
    pub r7: usize,
    pub program_counter: usize,
    pub stack_pointer: usize,
    pub list_register: usize,
    pub current_program_status_register: usize,
    pub syscall: usize,
}
#[derive(Debug)]
pub struct UserException {
    pub sender: Badge,
    pub program_counter: usize,
    pub stack_pointer: usize,
    pub current_program_status_register: usize,
    pub number: usize,
    pub code: usize,
}
#[derive(Debug)]
pub struct NullFault {
    pub sender: Badge,
}
#[derive(Debug)]
pub struct CapFault {
    pub sender: Badge,
    pub in_receive_phase: bool,
    pub cap_address: usize,
}
/// Grab bag for faults that don't fit the regular classification
#[derive(Debug)]
pub struct UnidentifiedFault {
    pub sender: Badge,
}

#[cfg(KernelArmHypervisorSupport)]
#[derive(Debug)]
pub struct VGICMaintenanceFault {
    pub sender: Badge,
    pub index: usize,
}

#[cfg(KernelArmHypervisorSupport)]
#[derive(Debug)]
pub struct VCPUFault {
    pub sender: Badge,
    pub hyp_syndrome_register: usize,
}

#[derive(Debug)]
pub enum Fault {
    VMFault(VMFault),
    UnknownSyscall(UnknownSyscall),
    UserException(UserException),
    NullFault(NullFault),
    CapFault(CapFault),
    UnidentifiedFault(UnidentifiedFault),
    #[cfg(KernelArmHypervisorSupport)]
    VGICMaintenanceFault(VGICMaintenanceFault),
    #[cfg(KernelArmHypervisorSupport)]
    VCPUFault(VCPUFault),
}

impl Fault {
    pub fn sender(&self) -> Badge {
        match self {
            Fault::VMFault(f) => f.sender,
            Fault::UnknownSyscall(f) => f.sender,
            Fault::UserException(f) => f.sender,
            Fault::NullFault(f) => f.sender,
            Fault::CapFault(f) => f.sender,
            Fault::UnidentifiedFault(f) => f.sender,
            #[cfg(KernelArmHypervisorSupport)]
            Fault::VGICMaintenanceFault(f) => f.sender,
            #[cfg(KernelArmHypervisorSupport)]
            Fault::VCPUFault(f) => f.sender,
        }
    }
}

impl From<(MessageInfo, Badge)> for Fault {
    fn from(info_and_sender: (MessageInfo, Badge)) -> Self {
        let (info, sender) = info_and_sender;
        let buffer: &mut seL4_IPCBuffer = unsafe { &mut *seL4_GetIPCBuffer() };
        const VM_FAULT: usize = seL4_Fault_tag_seL4_Fault_VMFault as usize;
        const UNKNOWN_SYSCALL: usize = seL4_Fault_tag_seL4_Fault_UnknownSyscall as usize;
        const USER_EXCEPTION: usize = seL4_Fault_tag_seL4_Fault_UserException as usize;
        const NULL_FAULT: usize = seL4_Fault_tag_seL4_Fault_NullFault as usize;
        const CAP_FAULT: usize = seL4_Fault_tag_seL4_Fault_CapFault as usize;
        #[cfg(KernelArmHypervisorSupport)]
        const VGIC_MAINTENANCE_FAULT: usize = seL4_Fault_tag_seL4_Fault_VGICMaintenance as usize;
        #[cfg(KernelArmHypervisorSupport)]
        const VCPU_FAULT: usize = seL4_Fault_tag_seL4_Fault_VCPUFault as usize;
        match info.label() {
            NULL_FAULT => Fault::NullFault(NullFault { sender }),
            VM_FAULT => Fault::VMFault(VMFault {
                sender,
                program_counter: buffer.msg[seL4_VMFault_IP as usize],
                address: buffer.msg[seL4_VMFault_Addr as usize],
                is_instruction_fault: 1 == buffer.msg[seL4_VMFault_PrefetchFault as usize],
                fault_status_register: buffer.msg[seL4_VMFault_FSR as usize],
            }),
            UNKNOWN_SYSCALL => Fault::UnknownSyscall(UnknownSyscall {
                sender,
                r0: buffer.msg[seL4_UnknownSyscall_R0 as usize],
                r1: buffer.msg[seL4_UnknownSyscall_R1 as usize],
                r2: buffer.msg[seL4_UnknownSyscall_R2 as usize],
                r3: buffer.msg[seL4_UnknownSyscall_R3 as usize],
                r4: buffer.msg[seL4_UnknownSyscall_R4 as usize],
                r5: buffer.msg[seL4_UnknownSyscall_R5 as usize],
                r6: buffer.msg[seL4_UnknownSyscall_R6 as usize],
                r7: buffer.msg[seL4_UnknownSyscall_R7 as usize],
                program_counter: buffer.msg[seL4_UnknownSyscall_FaultIP as usize],
                stack_pointer: buffer.msg[seL4_UnknownSyscall_SP as usize],
                list_register: buffer.msg[seL4_UnknownSyscall_LR as usize],
                current_program_status_register: buffer.msg[seL4_UnknownSyscall_CPSR as usize],
                syscall: buffer.msg[seL4_UnknownSyscall_Syscall as usize],
            }),
            USER_EXCEPTION => Fault::UserException(UserException {
                sender,
                program_counter: buffer.msg[seL4_UserException_FaultIP as usize],
                stack_pointer: buffer.msg[seL4_UserException_SP as usize],
                current_program_status_register: buffer.msg[seL4_UserException_CPSR as usize],
                number: buffer.msg[seL4_UserException_Number as usize],
                code: buffer.msg[seL4_UserException_Code as usize],
            }),
            CAP_FAULT => Fault::CapFault(CapFault {
                sender,
                cap_address: buffer.msg[seL4_CapFault_Addr as usize],
                in_receive_phase: 1 == buffer.msg[seL4_CapFault_InRecvPhase as usize],
            }),
            #[cfg(KernelArmHypervisorSupport)]
            VGIC_MAINTENANCE_FAULT => Fault::VGICMaintenanceFault(VGICMaintenanceFault {
                sender,
                index: buffer.msg[seL4_VGICMaintenance_IDX as usize],
            }),
            #[cfg(KernelArmHypervisorSupport)]
            VCPU_FAULT => Fault::VCPUFault(VCPUFault {
                sender,
                hyp_syndrome_register: buffer.msg[seL4_VCPUFault_HSR as usize],
            }),
            _ => Fault::UnidentifiedFault(UnidentifiedFault { sender }),
        }
    }
}
