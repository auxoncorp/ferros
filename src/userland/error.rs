#[derive(Debug)]
pub enum Error {
    UntypedRetype(u32),
    TCBConfigure(u32),
    MapPageTable(u32),
    UnmapPageTable(u32),
    ASIDPoolAssign(u32),
    MapPage(u32),
    UnmapPage(u32),
    CNodeCopy(u32),
    TCBWriteRegisters(u32),
    TCBSetPriority(u32),
    TCBResume(u32),
}