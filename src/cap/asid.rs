use crate::cap::{CapType, InternalASID};

#[derive(Debug)]
pub struct UnassignedASID {
    #[allow(dead_code)]
    pub(crate) asid: InternalASID,
}

impl CapType for UnassignedASID {}

#[derive(Debug)]
pub struct AssignedASID {
    pub(crate) asid: InternalASID,
}

impl CapType for AssignedASID {}
