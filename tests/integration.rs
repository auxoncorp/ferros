#![feature(proc_macro_hygiene)]

use smart_alloc::smart_alloc;

struct CNodeSlots {
    capacity: usize,
}

impl CNodeSlots {
    fn alloc(self) -> (CNodeSlots, CNodeSlots) {
        (
            CNodeSlots { capacity: 1 },
            CNodeSlots {
                capacity: self.capacity - 1,
            },
        )
    }

    fn new(capacity: usize) -> Self {
        CNodeSlots { capacity }
    }
}

struct UntypedBuddy {
    capacity: usize,
}

impl UntypedBuddy {
    fn alloc(self, cslots: CNodeSlots) -> Result<(UntypedBuddy, UntypedBuddy), ()> {
        assert!(cslots.capacity > 0);
        Ok((
            UntypedBuddy { capacity: 1 },
            UntypedBuddy {
                capacity: self.capacity - 1,
            },
        ))
    }

    fn new(capacity: usize) -> Self {
        UntypedBuddy { capacity }
    }
}

struct AddressBuddy {
    capacity: usize,
}

impl AddressBuddy {
    fn alloc(
        self,
        cslots: CNodeSlots,
        untypeds: UntypedBuddy,
    ) -> Result<(AddressBuddy, AddressBuddy), ()> {
        assert!(cslots.capacity > 0);
        assert!(untypeds.capacity > 0);
        Ok((
            AddressBuddy { capacity: 1 },
            AddressBuddy {
                capacity: self.capacity - 1,
            },
        ))
    }

    fn new(capacity: usize) -> Self {
        AddressBuddy { capacity }
    }
}

#[test]
fn single_resource() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc! {|cslots | {
        cs;
        let gamma = consume_slot(cs);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(8, cslots.capacity);
    Ok(())
}

#[test]
fn single_resource_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc! {|cslots: CNodeSlots | {
        cs;
        let gamma = consume_slot(cs);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(8, cslots.capacity);
    Ok(())
}

#[test]
fn single_resource_custom_request_id() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc! {|slot_please from cslots | {
        slot_please;
        let gamma = consume_slot(slot_please);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(8, cslots.capacity);
    Ok(())
}

#[test]
fn single_resource_custom_request_id_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc! {|slot_please from cslots: CNodeSlots | {
        slot_please;
        let gamma = consume_slot(slot_please);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(8, cslots.capacity);
    Ok(())
}

#[test]
fn two_resources_custom_request_id_unkinded() -> Result<(), ()> {
    let alpha = 1;
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|s from cslots, u from untypeds | {
        let alpha_prime = alpha;
        s;
        let gamma = consume_slot(s);
        let eta = consume_untyped(u);
        let alpha = 3;
    }}

    assert_eq!(1, alpha_prime);
    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_custom_request_id_both_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|c from cslots: CNodeSlots, u from untypeds: UntypedBuddy | {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_custom_request_id_first_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|slot_please from cslots: CNodeSlots, ut_please from untypeds | {
        slot_please;
        let gamma = consume_slot(slot_please);
        let eta = consume_untyped(ut_please);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_first_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|cslots: CNodeSlots, untypeds | {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_custom_request_id_second_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|c from cslots, u from untypeds: UntypedBuddy | {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_custom_request_id_second_kinded_unordered() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|u from untypeds, c from cslots: CNodeSlots  | {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_second_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|cslots, untypeds: UntypedBuddy | {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn two_resources_second_kinded_unordered() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);

    smart_alloc! {|untypeds, cslots: CNodeSlots| {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn three_resources_unkinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc! {|cslots, untypeds, addresses | {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(0, cslots.capacity);
    assert_eq!(3, untypeds.capacity);
    assert_eq!(1, eta);
    assert_eq!(1, delta);
    assert_eq!(4, addresses.capacity);
    Ok(())
}

#[test]
fn three_resources_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc! {|cslots: CNodeSlots, untypeds: UntypedBuddy, addresses: AddressBuddy | {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(0, cslots.capacity);
    assert_eq!(3, untypeds.capacity);
    assert_eq!(1, eta);
    assert_eq!(1, delta);
    assert_eq!(4, addresses.capacity);
    Ok(())
}

#[test]
fn three_resources_kinded_unordered() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc! {|addresses: AddressBuddy, cslots: CNodeSlots, untypeds: UntypedBuddy  | {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(0, cslots.capacity);
    assert_eq!(3, untypeds.capacity);
    assert_eq!(1, eta);
    assert_eq!(1, delta);
    assert_eq!(4, addresses.capacity);
    Ok(())
}

#[test]
fn three_resources_custom_request_ids_kinded_unordered() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc! {|vaddr from addresses: AddressBuddy, slots from cslots: CNodeSlots, uuu from untypeds: UntypedBuddy  | {
        slots;
        let gamma = consume_slot(slots);
        let eta = consume_untyped(uuu);
        let alpha = 3;
        let delta = consume_address(vaddr);
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(0, cslots.capacity);
    assert_eq!(3, untypeds.capacity);
    assert_eq!(1, eta);
    assert_eq!(1, delta);
    assert_eq!(4, addresses.capacity);
    Ok(())
}

#[test]
fn three_resources_single_custom_request_id_kinded_unordered() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc! {|addresses: AddressBuddy, slots from cslots: CNodeSlots, untypeds: UntypedBuddy  | {
        slots;
        let gamma = consume_slot(slots);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    }}

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(0, cslots.capacity);
    assert_eq!(3, untypeds.capacity);
    assert_eq!(1, eta);
    assert_eq!(1, delta);
    assert_eq!(4, addresses.capacity);
    Ok(())
}

fn consume_slot(cslots: CNodeSlots) -> usize {
    cslots.capacity
}

fn consume_untyped(ut: UntypedBuddy) -> usize {
    ut.capacity
}

fn consume_address(ad: AddressBuddy) -> usize {
    ad.capacity
}
