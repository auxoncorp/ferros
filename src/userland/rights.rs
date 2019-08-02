pub use selfe_wrap::CapRights;

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
