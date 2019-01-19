use sel4_sys::{seL4_CapRights_new, seL4_CapRights_t};

pub enum CapRights {
    R,
    W,
    RW,
    RWG,
}

impl From<CapRights> for seL4_CapRights_t {
    fn from(cr: CapRights) -> Self {
        match cr {
            CapRights::R => unsafe { seL4_CapRights_new(0, 0, 1) },
            CapRights::W => unsafe { seL4_CapRights_new(0, 1, 0) },
            CapRights::RW => unsafe { seL4_CapRights_new(0, 1, 1) },
            CapRights::RWG => unsafe { seL4_CapRights_new(1, 1, 1) },
        }
    }
}


pub trait Rights {
    fn as_caprights() -> CapRights;
}

#[allow(unused)]
pub mod rights {
    use super::*;

    pub struct R {}
    pub struct W {}
    pub struct RW {}
    pub struct RWG {}

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

}
