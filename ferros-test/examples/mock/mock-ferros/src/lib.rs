#[macro_export]
macro_rules! debug_println {
    ($fmt:expr) => {};
    ($fmt:expr, $($arg:tt)*) => {};
}

pub mod alloc {
    use super::cap::*;
    use core::marker::PhantomData;
    use typenum::*;

    pub fn ut_buddy<T: Unsigned>(ut: LocalCap<Untyped<T>>) -> UTBuddy<T> {
        UTBuddy(PhantomData)
    }

    pub struct UTBuddy<T: Unsigned>(PhantomData<T>);

    impl<T: Unsigned> UTBuddy<T> {
        pub fn alloc<BitSize: Unsigned>(
            self,
            slots: LocalCNodeSlots<U2>,
        ) -> Result<(LocalCap<Untyped<BitSize>>, UTBuddy<T>), SeL4Error> {
            Ok((LocalCap(PhantomData), UTBuddy(PhantomData)))
        }
    }
}
pub mod arch {
    pub struct PagingRoot;
}
pub mod bootstrap {
    use core::marker::PhantomData;
    pub struct UserImage<T>(pub PhantomData<T>);
}
pub mod userland {
    pub struct StackBitSize;
}
pub mod vspace {
    use core::marker::PhantomData;
    pub struct ScratchRegion<'a, 'b, T = ()>(pub PhantomData<&'a T>, pub PhantomData<&'b T>);
    pub struct MappedMemoryRegion<T, SS: SharedStatus>(PhantomData<T>, PhantomData<SS>);
    pub trait SharedStatus {}
    pub mod shared_status {
        pub struct Exclusive;
        impl super::SharedStatus for Exclusive {}
    }
}
pub mod cap {
    use core::marker::PhantomData;
    use core::ops::Sub;
    use typenum::*;
    pub struct LocalCNodeSlots<T>(pub PhantomData<T>);
    pub struct LocalCap<T>(pub PhantomData<T>);
    pub struct Untyped<T>(pub PhantomData<T>);
    pub struct ASIDPool<T>(pub PhantomData<T>);
    pub struct IRQControl;
    pub struct LocalCNode;
    pub struct ThreadPriorityAuthority;

    #[derive(Debug)]
    pub struct SeL4Error;

    pub mod role {
        pub struct Local;
    }

    impl<Size: Unsigned> LocalCNodeSlots<Size> {
        pub fn alloc<Count: Unsigned>(
            self,
        ) -> (LocalCNodeSlots<Count>, LocalCNodeSlots<Diff<Size, Count>>)
        where
            Size: Sub<Count>,
            Diff<Size, Count>: Unsigned,
        {
            (LocalCNodeSlots(PhantomData), LocalCNodeSlots(PhantomData))
        }
    }

    impl<FreeSlots: Unsigned> LocalCap<ASIDPool<FreeSlots>> {
        pub fn truncate<OutFreeSlots: Unsigned>(self) -> LocalCap<ASIDPool<OutFreeSlots>>
        where
            FreeSlots: IsGreaterOrEqual<OutFreeSlots, Output = True>,
        {
            LocalCap(PhantomData)
        }
    }
}

pub mod test_support {
    use typenum::*;
    pub type MaxTestUntypedSize = U27;
    pub type MaxTestCNodeSlots = U32768;
    pub type MaxTestASIDPoolSize = U1024;
    pub enum TestOutcome {
        Success,
        Failure,
    }
    pub type MaxMappedMemoryRegionBitSize = U20;
}
