#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheOp {
    CleanData,
    InvalidateData,
    CleanInvalidateData,
}

pub trait CacheableMemory {
    // Might be able to remove this with type level bits
    type Error;

    fn cache_op(&mut self, op: CacheOp, addr: usize, size: usize) -> Result<(), Self::Error>;

    fn clean_data(&mut self, start_addr: usize, end_addr: usize) -> Result<(), Self::Error>;

    fn invalidate_data(&mut self, start_addr: usize, end_addr: usize) -> Result<(), Self::Error>;

    fn clean_invalidate_data(
        &mut self,
        start_addr: usize,
        end_addr: usize,
    ) -> Result<(), Self::Error>;
}
