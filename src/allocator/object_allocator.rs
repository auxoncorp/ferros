use super::{Allocator, Error};
use sel4_sys::{seL4_CPtr, seL4_Word};

impl Allocator {
    /// Allocate a single object of the given type.
    pub fn alloc_kobject(
        &mut self,
        item_type: seL4_Word,
        item_size: usize,
    ) -> Result<seL4_CPtr, Error> {
        let size_bits = self.vka_get_object_size(item_type, item_size);

        // Allocate an untyped memory item of the right size
        let untyped_mem = self.alloc_untyped(size_bits, None, false)?;

        // Allocate an object
        let cap_range = self.retype_untyped_memory(untyped_mem, item_type, item_size, 1)?;

        // We should have gotten either zero items (if we ran out of caps), or one
        // item (if everything went well). If we get more than one, we
        // miscalculated our sizes
        assert!((cap_range.count == 0) || (cap_range.count == 1));

        // Return the first item (or zero if none were allocated
        Ok(cap_range.first as _)
    }
}
