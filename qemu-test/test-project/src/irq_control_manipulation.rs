use super::TopLevelError;
use ferros::alloc::smart_alloc;
use ferros::cap::{IRQControl, LocalCNode, LocalCNodeSlots, LocalCap, MaxIRQCount};
use ferros::userland::WIRQHandlerCollection;
use typenum::*;

#[ferros_test::ferros_test]
pub fn irq_control_manipulation(
    src_cnode: &LocalCap<LocalCNode>,
    cnode_slots: LocalCNodeSlots<U24>,
    weak_slots: LocalCNodeSlots<U24>,
    mut irq_control: LocalCap<IRQControl>,
) -> Result<(), super::TopLevelError> {
    const TOP_LEVEL_CLAIM: u16 = 20;
    const COLLECTION_CLAIM_A: u16 = 30;
    const COLLECTION_CLAIM_B: u16 = 50;
    const NEVER_HANDLED: u16 = 70;
    let mut weak_slots = weak_slots.weaken();
    smart_alloc! { |slots: cnode_slots<CNodeSlots>| {

        if let Ok(_) = irq_control.create_weak_handler(slots, MaxIRQCount::U16)  {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to make an IRQ handler >= the declared max count"));
        }
        if let Err(_) = irq_control.create_weak_handler(slots, TOP_LEVEL_CLAIM) {
            return Err(TopLevelError::TestAssertionFailure("Should be able to make a handler for an unclaimed IRQ"));
        }
        if let Ok(_) = irq_control.create_weak_handler(slots, TOP_LEVEL_CLAIM) {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to make a handler for a previous claimed IRQ"));
        }

        let mut previously_handled_irq_request = [false; MaxIRQCount::USIZE];
        previously_handled_irq_request[usize::from(TOP_LEVEL_CLAIM)] = true;
        if let Ok(_) = WIRQHandlerCollection::new(&mut irq_control, src_cnode, &mut weak_slots, previously_handled_irq_request) {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to split off an WIRQHandlerCollection while requesting a previously claimed/handled IRQ"));
        }

        let mut successful_request = [false; MaxIRQCount::USIZE];
        successful_request[usize::from(COLLECTION_CLAIM_A)] = true;
        successful_request[usize::from(COLLECTION_CLAIM_B)] = true;
        let successful_request_dupe = successful_request.clone();
        let mut split_collection = WIRQHandlerCollection::new(&mut irq_control,src_cnode, &mut weak_slots, successful_request)
            .map_err(|e| {
            debug_println!("{:?}", e);
            TopLevelError::TestAssertionFailure("Should be able to split off unclaimed irqs")})?;
        if let Ok(_) = WIRQHandlerCollection::new(&mut irq_control,src_cnode, &mut weak_slots, successful_request_dupe) {
            return Err(TopLevelError::TestAssertionFailure("The original source IRQControl should not be able split off multiple IRQControls with overlapping requested IRQs"));
        }

        let split_a_handler = split_collection.get_weak_handler(COLLECTION_CLAIM_A)
            .ok_or_else(|| TopLevelError::TestAssertionFailure("Should be able to make a handler from a split-off IRQControl"))?;

        if let Some(_) = split_collection.get_weak_handler(COLLECTION_CLAIM_A) {
            return Err(TopLevelError::TestAssertionFailure("WIRQHandleCollection should not return a handler for a given IRQ more than once"));
        }
        if let Some(_) = split_collection.get_weak_handler(NEVER_HANDLED) {
            return Err(TopLevelError::TestAssertionFailure("WIRQHandleCollection should not return a handler that was never supplied to it"));
        }

        if let Ok(_) = irq_control.create_weak_handler(slots, COLLECTION_CLAIM_A) {
            return Err(TopLevelError::TestAssertionFailure("The original source IRQControl should not be able to make a handler for a split-off IRQ"));
        }
    }}
    Ok(())
}
