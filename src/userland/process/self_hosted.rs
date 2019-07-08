use crate::cap::{LocalCap, ThreadControlBlock};

pub struct SelfHostedProcess {
    tcb: LocalCap<ThreadControlBlock>,
}
