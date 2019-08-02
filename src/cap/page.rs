// TODO: Move this to arch
use crate::arch::PageBytes;
use crate::cap::{CapRangeDataReconstruction, CapType, CopyAliasable, Movable};

#[derive(Clone, Debug)]
pub struct Page {}

impl CapType for Page {}

impl CopyAliasable for Page {
    type CopyOutput = Page;
}
impl Movable for Page {}

impl<'a> From<&'a Page> for Page {
    fn from(_val: &'a Page) -> Self {
        Page {}
    }
}

impl CapRangeDataReconstruction for Page {
    fn reconstruct(_index: usize, _seed_cap_data: &Self) -> Self {
        Page {}
    }
}

impl PhantomCap for Page {
    fn phantom_instance() -> Self {
        Page {}
    }
}
