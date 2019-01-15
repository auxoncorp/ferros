use super::{
    Allocator, CapRange, Error, InitUntypedItem, UntypedItem, MAX_INIT_UNTYPED_ITEMS,
    MAX_UNTYPED_SIZE, MIN_UNTYPED_SIZE,
};
use sel4_sys::{
    api_object_seL4_UntypedObject, seL4_BootInfo, seL4_CPtr, seL4_CapInitThreadCNode,
    seL4_Untyped_Retype, seL4_Word, seL4_WordBits,
};

use arrayvec::ArrayVec;
use core::iter::FromIterator;

impl Allocator {
    /// Initialise an allocator object.
    ///
    /// The struct 'Allocator' is memory where we should construct the
    /// allocator. All state will be kept in this struct, allowing multiple
    /// independent allocators to co-exist.
    /// 'root_cnode', 'root_cnode_depth', 'first_slot' and 'num_slots' specify
    /// a CNode containing a contiguous range of free cap slots that we will
    /// use for our allocations.
    ///
    /// 'items' specifies untyped memory items that we will
    /// allocate from.
    pub fn new<I>(
        root_cnode: seL4_CPtr,
        root_cnode_depth: usize,
        root_cnode_offset: usize,
        first_slot: usize,
        num_slots: usize,
        untyped_items_iter: I,
    ) -> Result<Allocator, Error>
    where
        I: Iterator<Item = UntypedItem>,
    {
        let mut init_items_iter = untyped_items_iter.map(|i| InitUntypedItem {
            item: i.clone(),
            is_free: true,
        });

        // Setup CNode information
        let a = Allocator {
            page_directory: 0,
            page_table: 0,
            last_allocated: 0,

            root_cnode: root_cnode,
            root_cnode_depth: root_cnode_depth as _,
            root_cnode_offset: root_cnode_offset as _,

            cslots: CapRange {
                first: first_slot,
                count: num_slots,
            },

            num_slots_used: 0,
            init_untyped_items: ArrayVec::from_iter(&mut init_items_iter),

            untyped_items: [CapRange { first: 0, count: 0 };
                (MAX_UNTYPED_SIZE - MIN_UNTYPED_SIZE) + 1],
        };

        // Check that all of the untyped items were consumed. If they were not,
        // then more than MAX_INIT_UNTYPED_ITEMS were supplied.
        match init_items_iter.next() {
            None => Ok(a),
            Some(_) => Err(Error::ResourceExhausted),
        }
    }

    /// Create an object allocator managing the root CNode's free slots.
    pub fn bootstrap(bootinfo: &'static seL4_BootInfo) -> Result<Allocator, Error> {
        let untyped_item_iter = (0..(bootinfo.untyped.end - bootinfo.untyped.start)).map(|i| {
            UntypedItem::new(
                bootinfo.untyped.start + i,                     // cap
                bootinfo.untypedList[i as usize].sizeBits as _, // size_bits
                bootinfo.untypedList[i as usize].paddr,         // paddr
                bootinfo.untypedList[i as usize].isDevice != 0, // is_device
            )
        });

        if let Some(Some(e)) = untyped_item_iter
            .clone()
            .map(|i| match i {
                Ok(_) => None,
                Err(e) => Some(e),
            })
            .find(|o| o.is_some())
        {
            return Err(e);
        }

        Self::new(
            seL4_CapInitThreadCNode,
            seL4_WordBits as _,
            0,
            bootinfo.empty.start as _,
            (bootinfo.empty.end - bootinfo.empty.start) as _,
            untyped_item_iter.map(|i| i.unwrap()),
        )
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
        // Determine the maximum number of items we have space in our CNode for
        let max_objects = self.cslots.count - self.num_slots_used;
        if num_items > max_objects {
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

        let result = CapRange {
            first: self.cslots.first + self.num_slots_used + self.root_cnode_offset as usize,
            count: num_items,
        };

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
        for init_item in &mut self.init_untyped_items {
            if init_item.is_free && (init_item.item.size_bits >= size_bits) {
                let mut consume = false;

                if let Some(paddr) = paddr {
                    if init_item.item.paddr == paddr {
                        consume = true;
                    }
                } else {
                    consume = true;
                }

                if !can_use_dev {
                    if init_item.item.is_device {
                        consume = false;
                    }
                }

                if consume {
                    init_item.is_free = false;
                    return Ok(init_item.item.cap);
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
