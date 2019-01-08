use super::Allocator;
use sel4_sys::{seL4_BootInfo, seL4_CapInitThreadCNode, seL4_WordBits};

impl Allocator {
    /// Create an object allocator managing the root CNode's free slots.
    pub fn bootstrap(&mut self, bootinfo: &'static seL4_BootInfo) {
        // Create the allocator
        self.create(
            seL4_CapInitThreadCNode,
            seL4_WordBits as _,
            0,
            bootinfo.empty.start as _,
            (bootinfo.empty.end - bootinfo.empty.start) as _,
            &[],
        );

        // Give the allocator all of our free memory
        self.fill_allocator_with_resources(bootinfo);
    }

    /// Fill the given allocator with resources from the given
    /// bootinfo structure.
    fn fill_allocator_with_resources(&mut self, bootinfo: &'static seL4_BootInfo) {
        for i in 0..(bootinfo.untyped.end - bootinfo.untyped.start) {
            self.add_root_untyped_item(
                bootinfo.untyped.start + i,
                bootinfo.untypedList[i as usize].sizeBits as _,
                bootinfo.untypedList[i as usize].paddr,
                bootinfo.untypedList[i as usize].isDevice != 0,
            );
        }
    }
}
