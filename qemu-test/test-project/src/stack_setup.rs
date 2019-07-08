use ferros::arch::userland::process::test::test_stack_setup;
#[ferros_test::ferros_test]
pub fn stack_setup() -> Result<(), super::TopLevelError> {
    match test_stack_setup() {
        Ok(_) => Ok(()),
        Err(e) => {
            debug_println!("{:?}", e);
            Err(super::TopLevelError::TestAssertionFailure("stack setup trouble"))
        }
    }
}
