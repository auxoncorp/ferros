use selfe_sys::{seL4_CapRights_new, seL4_CapRights_t};

#[derive(Clone, Copy)]
pub enum CapRights {
    R,
    W,
    RW,
    RWG,
    WG,
    /// Can Grant ReplY
    Y,
}

impl From<CapRights> for seL4_CapRights_t {
    fn from(cr: CapRights) -> Self {
        match cr {
            CapRights::R => unsafe { seL4_CapRights_new(0, 0, 1, 0) },
            CapRights::W => unsafe { seL4_CapRights_new(0, 0, 0, 1) },
            CapRights::RW => unsafe { seL4_CapRights_new(0, 0, 1, 1) },
            CapRights::RWG => unsafe { seL4_CapRights_new(0, 1, 1, 1) },
            CapRights::WG => unsafe { seL4_CapRights_new(0, 1, 0, 1) },
            CapRights::Y => unsafe { seL4_CapRights_new(1, 0, 0, 0) },
        }
    }
}

pub trait Rights: private::SealedRights {
    fn as_caprights() -> CapRights;
}

#[allow(unused)]
pub mod rights {
    use super::*;

    pub struct R {}
    pub struct W {}
    pub struct RW {}
    pub struct RWG {}
    pub struct WG {}

    impl Rights for R {
        fn as_caprights() -> CapRights {
            CapRights::R
        }
    }

    impl Rights for W {
        fn as_caprights() -> CapRights {
            CapRights::W
        }
    }

    impl Rights for RW {
        fn as_caprights() -> CapRights {
            CapRights::RW
        }
    }

    impl Rights for RWG {
        fn as_caprights() -> CapRights {
            CapRights::RWG
        }
    }

    impl Rights for WG {
        fn as_caprights() -> CapRights {
            CapRights::WG
        }
    }
}
mod private {
    use super::*;
    pub trait SealedRights {}
    impl SealedRights for rights::R {}
    impl SealedRights for rights::W {}
    impl SealedRights for rights::RW {}
    impl SealedRights for rights::RWG {}
    impl SealedRights for rights::WG {}
}
