use super::{
    Allocator, CapRange, Error, UntypedItem, MAX_UNTYPED_ITEMS, MAX_UNTYPED_SIZE, MIN_UNTYPED_SIZE,
};
use core::mem;
use sel4_sys::{
    api_object_seL4_UntypedObject, seL4_CPtr, seL4_CapInitThreadCNode, seL4_Untyped_Retype,
    seL4_Word,
};

impl Allocator {
    /// TODO - don't need to zero, just do a normal construct
    pub fn new() -> Allocator {
        let alloc: Allocator = unsafe { mem::zeroed() };

        alloc
    }

    /// Initialise an allocator object at 'allocator'.
    ///
    /// The struct 'Allocator' is memory where we should construct the
    /// allocator. All state will be kept in this struct, allowing multiple
    /// independent allocators to co-exist.
    /// 'root_cnode', 'root_cnode_depth', 'first_slot' and 'num_slots' specify\
    /// a CNode containing a contiguous range of free cap slots that we will
    /// use for our allocations.
    ///
    /// 'items' and 'num_items' specify untyped memory items that we will
    /// allocate from.
    pub fn create(
        &mut self,
        root_cnode: seL4_CPtr,
        root_cnode_depth: usize,
        root_cnode_offset: usize,
        first_slot: usize,
        num_slots: usize,
        items: &[UntypedItem],
    ) {
        assert!(items.len() < MAX_UNTYPED_ITEMS);

        // Setup CNode information
        self.root_cnode = root_cnode;
        self.root_cnode_depth = root_cnode_depth as _;
        self.root_cnode_offset = root_cnode_offset as _;
        self.cslots.first = first_slot;
        self.cslots.count = num_slots;
        self.num_slots_used = 0;
        self.num_init_untyped_items = 0;

        // Setup all of our pools as empty
        for i in MIN_UNTYPED_SIZE..=MAX_UNTYPED_SIZE {
            self.untyped_items[i - MIN_UNTYPED_SIZE].first = 0;
            self.untyped_items[i - MIN_UNTYPED_SIZE].count = 0;
        }

        // Copy untyped items
        for i in 0..items.len() {
            self.add_root_untyped_item(
                items[i].cap,
                items[i].size_bits,
                items[i].paddr,
                items[i].is_device,
            );
        }
    }

    /// Permanently add additional untyped memory to the allocator.
    ///
    /// The allocator will permanently hold on to this memory
    /// and continue using it until `destroy()` is called,
    /// even if the allocator is reset.
    pub fn add_root_untyped_item(
        &mut self,
        cap: seL4_CPtr,
        size_bits: usize,
        paddr: seL4_Word,
        is_device: bool,
    ) {
        assert!(cap != 0);
        assert!(size_bits >= MIN_UNTYPED_SIZE);
        assert!(size_bits <= MAX_UNTYPED_SIZE);
        assert!(self.num_init_untyped_items < MAX_UNTYPED_ITEMS);

        let next_item = self.num_init_untyped_items;
        self.init_untyped_items[next_item].item.cap = cap;
        self.init_untyped_items[next_item].item.size_bits = size_bits;
        self.init_untyped_items[next_item].item.paddr = paddr;
        self.init_untyped_items[next_item].item.is_device = is_device;
        self.init_untyped_items[next_item].is_free = true;
        self.num_init_untyped_items += 1;
    }

    /// Allocate an empty cslot.
    pub fn alloc_cslot(&mut self) -> Result<seL4_CPtr, Error> {
        // Determine whether we have any free slots
        if (self.cslots.count - self.num_slots_used) == 0 {
            return Err(Error::ResourceExhausted);
        }

        // Pick the first one
        let result: seL4_CPtr = self.cslots.first as seL4_CPtr
            + self.num_slots_used as seL4_CPtr
            + self.root_cnode_offset;

        // Record this slot as used
        self.num_slots_used += 1;

        Ok(result)
    }

    /// Free an empty cslot.
    /// We can only free a slot if it was the last to be allocated.
    pub fn free_cslot(&mut self, slot: seL4_CPtr) {
        let next_slot: seL4_CPtr = self.cslots.first as seL4_CPtr
            + self.num_slots_used as seL4_CPtr
            + self.root_cnode_offset as seL4_CPtr;

        if next_slot == (slot + 1) {
            self.num_slots_used -= 1;
        }
    }

    /// Retype an untyped item.
    pub fn retype_untyped_memory(
        &mut self,
        untyped_item: seL4_CPtr,
        item_type: seL4_Word,
        item_size: usize,
        num_items: usize,
    ) -> Result<CapRange, Error> {
        let mut result = CapRange { first: 0, count: 0 };

        // Determine the maximum number of items we have space in our CNode for
        let max_objects = self.cslots.count - self.num_slots_used;
        if num_items > max_objects {
            result.count = 0;
            result.first = 0;
            return Err(Error::ResourceExhausted);
        }

        // Do the allocation. We expect at least one item will be created
        let err = unsafe {
            seL4_Untyped_Retype(
                untyped_item,
                item_type,
                item_size as _,
                seL4_CapInitThreadCNode,
                self.root_cnode,
                self.root_cnode_depth,
                (self.cslots.first + self.num_slots_used) as _,
                num_items as _,
            )
        };
        if err != 0 {
            return Err(Error::Other);
        }

        // Save the allocation
        result.count = num_items;
        result.first = self.cslots.first + self.num_slots_used + self.root_cnode_offset as usize;

        // Record these slots as used
        self.num_slots_used += num_items;

        Ok(result)
    }

    /// Allocate untyped item of size 'size_bits' bits.
    pub fn alloc_untyped(
        &mut self,
        size_bits: usize,
        paddr: Option<seL4_Word>,
        can_use_dev: bool,
    ) -> Result<seL4_CPtr, Error> {
        // If it is too small or too big, not much we can do
        if size_bits < MIN_UNTYPED_SIZE {
            return Err(Error::Other);
        }
        if size_bits > MAX_UNTYPED_SIZE {
            return Err(Error::Other);
        }

        let mut pool = self.untyped_items[size_bits - MIN_UNTYPED_SIZE].clone();

        // Do we have something of the correct size in one of our pools?
        if let Ok(valid_cap) = self.range_alloc(&mut pool, 1) {
            return Ok(valid_cap);
        }

        // Do we have something of the correct size in initial memory regions?
        for i in 0..self.num_init_untyped_items {
            if self.init_untyped_items[i].is_free
                && (self.init_untyped_items[i].item.size_bits >= size_bits)
            {
                let mut consume = false;

                if let Some(paddr) = paddr {
                    if self.init_untyped_items[i].item.paddr == paddr {
                        consume = true;
                    }
                } else {
                    consume = true;
                }

                if !can_use_dev {
                    if self.init_untyped_items[i].item.is_device {
                        consume = false;
                    }
                }

                if consume {
                    self.init_untyped_items[i].is_free = false;
                    return Ok(self.init_untyped_items[i].item.cap);
                }
            }
        }

        // Otherwise, try splitting something of a bigger size, recursively
        let big_untyped_item = self.alloc_untyped(size_bits + 1, paddr, can_use_dev)?;

        let range = self.retype_untyped_memory(
            big_untyped_item,
            api_object_seL4_UntypedObject,
            size_bits,
            2,
        )?;

        assert!(range.count != 0);
        pool = range;

        // Allocate and return out of our split
        self.range_alloc(&mut pool, 1)
    }

    /// Allocate 'count' items out of the given range.
    fn range_alloc(&mut self, range: &mut CapRange, count: usize) -> Result<seL4_CPtr, Error> {
        // If there are not enough items in the range, abort
        if range.count < count {
            return Err(Error::ResourceExhausted);
        }

        // Allocate from the range
        assert!(range.first != 0);
        range.count -= count;

        return Ok((range.first + range.count) as _);
    }
}
