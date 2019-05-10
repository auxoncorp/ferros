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

    smart_alloc!(|slot_please: cslots| {
        slot_please;
        let gamma = consume_slot(slot_please);
        let alpha = 3;
    });

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(8, cslots.capacity);
    Ok(())
}

#[test]
fn single_resource_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc!(|slot_please: cslots<CNodeSlots>| {
        slot_please;
        let gamma = consume_slot(slot_please);
        let alpha = 3;
    });

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

    smart_alloc!(|s: cslots, u: untypeds| {
        let alpha_prime = alpha;
        s;
        let gamma = consume_slot(s);
        let eta = consume_untyped(u);
        let alpha = 3;
    });

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

    smart_alloc!(|c: cslots<CNodeSlots>, u: untypeds<UntypedBuddy>| {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    });

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

    smart_alloc!(|slot_please: cslots<CNodeSlots>, ut_please: untypeds| {
        slot_please;
        let gamma = consume_slot(slot_please);
        let eta = consume_untyped(ut_please);
        let alpha = 3;
    });

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

    smart_alloc!(|c: cslots, u: untypeds<UntypedBuddy>| {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    });

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

    smart_alloc!(|u: untypeds, c: cslots<CNodeSlots>| {
        c;
        let gamma = consume_slot(c);
        let eta = consume_untyped(u);
        let alpha = 3;
    });

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(2, cslots.capacity);
    assert_eq!(4, untypeds.capacity);
    assert_eq!(1, eta);
    Ok(())
}

#[test]
fn three_resources_kinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc!(|cs: cslots<CNodeSlots>,
                  ut: untypeds<UntypedBuddy>,
                  ad: addresses<AddressBuddy>| {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    });

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

    smart_alloc!(|ad: addresses<AddressBuddy>,
                  cs: cslots<CNodeSlots>,
                  ut: untypeds<UntypedBuddy>| {
        cs;
        let gamma = consume_slot(cs);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    });

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

    smart_alloc!(|vaddr: addresses<AddressBuddy>,
                  slots: cslots<CNodeSlots>,
                  uuu: untypeds<UntypedBuddy>| {
        slots;
        let gamma = consume_slot(slots);
        let eta = consume_untyped(uuu);
        let alpha = 3;
        let delta = consume_address(vaddr);
    });

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
fn three_resources_unkinded() -> Result<(), ()> {
    let cslots = CNodeSlots::new(5);
    let untypeds = UntypedBuddy::new(5);
    let addresses = AddressBuddy::new(5);

    smart_alloc!(|slots: cslots, ut: untypeds, ad: addresses| {
        slots;
        let gamma = consume_slot(slots);
        let eta = consume_untyped(ut);
        let alpha = 3;
        let delta = consume_address(ad);
    });

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
fn single_resources_nested() -> Result<(), ()> {
    let cslots = CNodeSlots::new(10);

    smart_alloc!(|outer_slot_please: cslots| {
        outer_slot_please;
        let gamma = consume_slot(outer_slot_please);
        let cslots_inner = CNodeSlots::new(5);
        smart_alloc! {|inner_slot_please: cslots_inner| {
            let delta = inner_slot_please;
            let epsilon = consume_slot(inner_slot_please);
            let alpha = 3;
            let psi = consume_slot(outer_slot_please);
        }};
    });

    assert_eq!(3, alpha);
    assert_eq!(1, gamma);
    assert_eq!(7, cslots.capacity);
    assert_eq!(1, delta.capacity);
    assert_eq!(1, epsilon);
    assert_eq!(1, psi);
    assert_eq!(3, cslots_inner.capacity);
    Ok(())
}

#[test]
fn single_resources_deeply_nested() -> Result<(), ()> {
    let cslots_crust = CNodeSlots::new(5);
    let cslots_mantle = CNodeSlots::new(5);
    let cslots_core = CNodeSlots::new(5);

    smart_alloc!(|crust: cslots_crust| {
        let beta = crust;
        // Note that by some quirk, curly brackets
        // are necessary for nested invocations to be
        // able to access resources declared in higher scopes.
        //
        // By some other quirk, rustfmt doesn't work with
        // curly bracket based macro invocation, so weigh
        // your choice wisely.
        smart_alloc! {|mantle: cslots_mantle| {
            let gamma = crust;
            let delta = mantle;
            smart_alloc!{|core: cslots_core| {
                let epsilon = crust;
                let zeta = mantle;
                let eta = core;

                let alpha = 3;
            }};
        }};
    });

    assert_eq!(1, beta.capacity);
    assert_eq!(1, gamma.capacity);
    assert_eq!(1, delta.capacity);
    assert_eq!(1, epsilon.capacity);
    assert_eq!(1, zeta.capacity);
    assert_eq!(1, eta.capacity);

    assert_eq!(2, cslots_crust.capacity);
    assert_eq!(3, cslots_mantle.capacity);
    assert_eq!(4, cslots_core.capacity);

    assert_eq!(3, alpha);
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
