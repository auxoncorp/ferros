use super::TopLevelError;
use ferros::alloc::smart_alloc;
use ferros::cap::{IRQControl, LocalCNode, LocalCNodeSlots, LocalCap, MaxIRQCount};
use typenum::*;

#[ferros_test::ferros_test]
pub fn irq_control_manipulation(
    src_cnode: &LocalCap<LocalCNode>,
    cnode_slots: LocalCNodeSlots<U24>,
    mut irq_control: LocalCap<IRQControl>,
) -> Result<(), super::TopLevelError> {
    smart_alloc! { |slots: cnode_slots<CNodeSlots>| {

        if let Ok(_) = irq_control.create_weak_handler(slots, MaxIRQCount::U16)  {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to make an IRQ handler >= the declared max count"));
        }
        if let Err(_) = irq_control.create_weak_handler(slots, 20) {
            return Err(TopLevelError::TestAssertionFailure("Should be able to make a handler for an unclaimed IRQ"));
        }
        if let Ok(_) = irq_control.create_weak_handler(slots, 20) {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to make a handler for a previous claimed IRQ"));
        }

        let mut previously_handled_irq_request = [false; MaxIRQCount::USIZE];
        previously_handled_irq_request[20] = true;
        if let Ok(_) = irq_control.request_split(src_cnode, slots, previously_handled_irq_request) {
            return Err(TopLevelError::TestAssertionFailure("Should not be able to split off an IRQControl while requesting a previously claimed/handled IRQ"));
        }

        let mut successful_request = [false; MaxIRQCount::USIZE];
        successful_request[30] = true;
        successful_request[50] = true;
        let successful_request_dupe = successful_request.clone();
        let mut split_side_control = irq_control.request_split(src_cnode, slots, successful_request)
            .map_err(|e| {
            debug_println!("{:?}", e);
            TopLevelError::TestAssertionFailure("Should be able to split off unclaimed irqs")})?;
        if let Ok(_) = irq_control.request_split(src_cnode, slots, successful_request_dupe) {
            return Err(TopLevelError::TestAssertionFailure("The original source IRQControl should not be able split off multiple IRQControls with overlapping requested IRQs"));
        }

        let split_a_handler = split_side_control.create_weak_handler(slots, 50)
            .map_err(|_| TopLevelError::TestAssertionFailure("Should be able to make a handler from a split-off IRQControl"))?;

        if let Ok(_) = irq_control.create_weak_handler(slots, 50) {
            return Err(TopLevelError::TestAssertionFailure("The original source IRQControl should not be able to make a handler for a split-off IRQ"));
        }
    }}
    Ok(())
}
