#![feature(proc_macro_hygiene)]

use smart_alloc::smart_alloc;

//#[test]
//fn scope_management() {
//    let a = 1;
//
//    smart_alloc! {
//        let a = 3;
//        let b = q;
//    }
//
//    assert_eq!(a, 3);
//    //assert_eq!(b, 314159);
//}

struct CSlots {
    capacity: usize,
}

impl CSlots {
    fn alloc(self) -> (CSlots, CSlots) {
        (
            CSlots { capacity: 1 },
            CSlots {
                capacity: self.capacity - 1,
            },
        )
    }

    fn new(capacity: usize) -> Self {
        CSlots { capacity }
    }
}

struct Untypeds {
    capacity: usize,
}

impl Untypeds {
    fn alloc(self, cslots: CSlots) -> (Untypeds, Untypeds) {
        assert!(cslots.capacity > 0);
        (
            Untypeds { capacity: 1 },
            Untypeds {
                capacity: self.capacity - 1,
            },
        )
    }

    fn new(capacity: usize) -> Self {
        Untypeds { capacity }
    }
}

#[test]
fn foo() {
    let alpha = 1;
    let cslots = CSlots::new(5);
    let untypeds = Untypeds::new(5);

    smart_alloc! {|cslots as cs, untypeds as ut| {
        cs;
        let gamma = consume_a_slot(cs);
        let eta = consume_an_untyped(ut);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
}

fn consume_a_slot(cslots: CSlots) -> usize {
    cslots.capacity
}

fn consume_an_untyped(ut: Untypeds) -> usize {
    ut.capacity
}
