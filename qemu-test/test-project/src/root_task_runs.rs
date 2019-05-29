use super::TopLevelError;

use ferros_test::ferros_test;
#[ferros_test]
pub fn test() -> Result<(), TopLevelError> {
    Ok(())
}
