use crate::cap::Badge;

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
}
#[derive(Debug)]
pub struct UserException {
    pub sender: Badge,
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
