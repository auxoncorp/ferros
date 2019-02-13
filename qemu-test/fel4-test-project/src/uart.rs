use sel4_sys::*;

use typenum::consts::{U12, U14, U18, U58};

use ferros::drivers::uart::UartParams;
use ferros::micro_alloc;
use ferros::userland::{role, root_cnode, BootInfo, InterruptConsumer, VSpace};

use super::TopLevelError;

const UART1_PADDR: usize = 0x02020000;
type Uart1IrqLine = U58;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let root_cnode = root_cnode(&raw_boot_info);

    // Untypeds needed (in bits):
    //   1. Address Space Identifier Pool (ASID Pool):  12
    //   2. Scratch page table:                         10
    //   3. For any processes:
    //     a. Root Cnode (CSpace) (U12 + U4 for Radix): 16
    //     b. VSpace:                                   16
    //     c. Thread (Stack, fault ep, &c.)             14
    //   4. A notification for the interrupt             4
    // ---------------------------------------------------
    // Start with:                                      18

    // Find an untyped of size 18 bits (256k / 0.25m).
    let untyped_18 = allocator
        .get_untyped::<U18>()
        .expect("initial alloc failure");

    // The UART1 region is 4 pages i.e. 14 bits.
    // C.f. i.MX 6ULL Reference Manual Table 2.2.
    let uart1_base_untyped = allocator
        .get_device_untyped::<U14>(UART1_PADDR)
        .expect("find uart1 device memory");

    let (uart1_cspace_untyped, uart1_vspace_untyped, untyped_16, _, root_cnode) =
        untyped_18.quarter(root_cnode)?;
    let (uart1_thread_untyped, untyped_14, _, _, root_cnode) = untyped_16.quarter(root_cnode)?;
    let (asid_pool_untyped, untyped_12, _, _, root_cnode) = untyped_14.quarter(root_cnode)?;
    let (scratch_page_table_untyped, untyped_10, _, _, root_cnode) =
        untyped_12.quarter(root_cnode)?;
    let (untyped_8, _, _, _, root_cnode) = untyped_10.quarter(root_cnode)?;
    let (untyped_6, _, _, _, root_cnode) = untyped_8.quarter(root_cnode)?;
    let (notification_untyped, _, _, _, root_cnode) = untyped_6.quarter(root_cnode)?;

    let (boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_untyped, root_cnode);

    let (unmapped_scratch_page_table, root_cnode) =
        scratch_page_table_untyped.retype_local(root_cnode)?;
    let (mut scratch_page_table, boot_info) =
        boot_info.map_page_table(unmapped_scratch_page_table)?;

    let (uart1_cnode, root_cnode) = uart1_cspace_untyped.retype_cnode::<_, U12>(root_cnode)?;

    let (uart1_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, uart1_vspace_untyped, root_cnode)?;

    let (interrupt_consumer, _, uart1_cnode, root_cnode) = InterruptConsumer::new(
        notification_untyped,
        uart1_cnode,
        &mut boot_info.irq_control,
        root_cnode,
    )?;

    let (uart1_page_1_untyped, _, _, _, root_cnode) = uart1_base_untyped.quarter(root_cnode)?;
    let (unmapped_uart1_page_1, root_cnode) =
        uart1_page_1_untyped.retype_device_page(root_cnode)?;
    let (uart1_page_1, uart1_vspace) = uart1_vspace.map_page(unmapped_uart1_page_1)?;

    let uart1_params = UartParams::<Uart1IrqLine, role::Child> {
        base_ptr: uart1_page_1.vaddr(),
        consumer: interrupt_consumer,
    };

    let (uart1_thread, _, _) = uart1_vspace.prepare_thread(
        uart::run,
        uart1_params,
        uart1_thread_untyped,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;

    uart1_thread.start(uart1_cnode, None, &boot_info.tcb, 255)?;

    Ok(())
}

pub mod uart {

    use core::mem;

    use sel4_sys::*;

    use ferros::drivers::uart::*;
    use ferros::userland::role;

    use typenum::consts::{True, U1, U256};
    use typenum::{IsLess, Unsigned};

    use registers::ReadWriteRegister;

    pub extern "C" fn run<IRQ: Unsigned + Sync + Send>(params: UartParams<IRQ, role::Local>)
    where
        IRQ: IsLess<U256, Output = True>,
    {
        let mut uart = Uart {
            control1: UartControl1::Register::new(unsafe {
                mem::transmute(params.base_ptr + CTL1_OFFSET)
            }),
            tx: UartTX::Register::new(unsafe { mem::transmute(params.base_ptr + TX_OFFSET) }),
            rx: UartRX::Register::new(unsafe { mem::transmute(params.base_ptr) }),
        };

        uart.control1
            .modify(UartControl1::RecvReadyInterrupt::Field::checked::<U1>());

        debug_println!("thou art ready");

        params.consumer.consume((), move |state| {
            let data = uart.get();
            if let Some(d) = data {
                debug_println!("got byte: {:?}", d);
            }
            state
        })
    }
}
