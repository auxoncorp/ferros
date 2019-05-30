use super::TopLevelError;

use ferros_test::ferros_test;
#[ferros_test]
pub fn root_task_runs() -> Result<(), TopLevelError> {
    Ok(())
}
