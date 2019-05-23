use ferros::test_support::*;
use ferros::userland::*;
use ferros_test::ferros_test;
use typenum::*;

#[ferros_test]
fn zero_parameters() {}

#[ferros_test]
fn zero_parameters_returns_testoutcome_success() -> TestOutcome {
    TestOutcome::Success
}

#[ferros_test]
fn zero_parameters_returns_testoutcome_failure() -> TestOutcome {
    TestOutcome::Failure
}

#[ferros_test]
fn zero_parameters_returns_result_ok() -> Result<(), ()> {
    Ok(())
}

#[ferros_test]
fn zero_parameters_returns_result_err() -> Result<(), ()> {
    Err(())
}

#[ferros_test]
fn zero_parameters_returns_unit() -> () {}

#[ferros_test]
fn localcap_slots_before_untyped_parameter(
    slots: LocalCNodeSlots<U5>,
    untyped: LocalCap<Untyped<U5>>,
) {
}

#[ferros_test]
fn localcap_untyped_parameter(untyped: LocalCap<Untyped<U5>>) {}

#[ferros_test]
fn localcnodeslots_parameter(slots: LocalCNodeSlots<U5>) {}

#[ferros_test]
fn localcap_asidpool_parameter(slots: LocalCap<ASIDPool<U1024>>) {}

#[ferros_test]
fn localcap_asidpool_smaller_than_max(slots: LocalCap<ASIDPool<U512>>) {}

#[ferros_test]
fn localcap_localcnode_parameter(node: &LocalCap<LocalCNode>) {}

#[ferros_test]
fn localcap_threadpriorityauthority_parameter(tpa: &LocalCap<ThreadPriorityAuthority>) {}

#[ferros_test]
fn userimage_parameter(image: &UserImage<ferros::userland::role::Local>) {}

#[ferros_test]
fn vspacescratch_parameter(scratch: &mut VSpaceScratchSlice<ferros::userland::role::Local>) {}

pub mod ferros {
    pub mod alloc {
        use super::userland::*;
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
    pub mod userland {
        use core::marker::PhantomData;
        use core::ops::Sub;
        use typenum::*;
        pub struct LocalCNodeSlots<T>(pub PhantomData<T>);
        pub struct LocalCap<T>(pub PhantomData<T>);
        pub struct Untyped<T>(pub PhantomData<T>);
        pub struct ASIDPool<T>(pub PhantomData<T>);
        pub struct UserImage<T>(pub PhantomData<T>);
        pub struct VSpaceScratchSlice<T>(pub PhantomData<T>);
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
    }
}
