pub mod descriptor;
pub mod ring;
pub mod ring_entry;

pub enum Rx {}
impl sealed::RxTx for Rx {}
pub enum Tx {}
impl sealed::RxTx for Tx {}
pub(crate) mod sealed {
    pub trait RxTx {}
}
