use selfe_sys::*;

use ferros::drivers::uart::UartParams;
use ferros::alloc::{self, smart_alloc, micro_alloc};
use ferros::userland::{role, root_cnode, BootInfo, InterruptConsumer, VSpace, retype, retype_cnode};
use typenum::*;

use super::TopLevelError;

const UART1_PADDR: usize = 0x02020000;
type Uart1IrqLine = U58;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    let ut27 = allocator
        .get_untyped::<U27>()
        .expect("initial alloc failure");
    let uts = alloc::ut_buddy(ut27);

    // The UART1 region is 4 pages i.e. 14 bits.
    // C.f. i.MX 6ULL Reference Manual Table 2.2.
    let uart1_base_untyped = allocator
        .get_device_untyped::<U14>(UART1_PADDR)
        .expect("find uart1 device memory");

    smart_alloc!(|slots from local_slots, ut from uts| {
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        let unmapped_scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, boot_info) =
            boot_info.map_page_table(unmapped_scratch_page_table)?;

        let (uart1_cnode, uart1_slots) = retype_cnode::<U12>(ut, slots)?;
        let (uart1_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;

        let (slots_u, uart1_slots) = uart1_slots.alloc();
        let (interrupt_consumer, _) = InterruptConsumer::new(
            ut,
            &mut boot_info.irq_control,
            &root_cnode,
            slots,
            slots_u
        )?;

        let (uart1_page_1_untyped, _, _, _) = uart1_base_untyped.quarter(slots)?;
        let uart1_page_1 = uart1_page_1_untyped.retype_device_page(slots)?;
        let (uart1_page_1, uart1_vspace) = uart1_vspace.map_page(uart1_page_1)?;

        let uart1_params = UartParams::<Uart1IrqLine, role::Child> {
            base_ptr: uart1_page_1.vaddr(),
            consumer: interrupt_consumer,
        };

        let (uart1_thread, _) = uart1_vspace.prepare_thread(
            uart::run,
            uart1_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        uart1_thread.start(uart1_cnode, None, &boot_info.tcb, 255)?;
    });

    Ok(())
}

pub mod uart {

    use core::mem;

    use selfe_sys::*;

    use ferros::drivers::uart::UartParams;
    use ferros::userland::role;

    use typenum::consts::{True, U1, U256};
    use typenum::{IsLess, Unsigned};

    use registers::{ReadOnlyRegister, ReadWriteRegister, WriteOnlyRegister};

    register! {
        UartRX,
        RO,
        Fields [
            Data        WIDTH(U8) OFFSET(U0),
            ParityError WIDTH(U1) OFFSET(U10),
            Brk         WIDTH(U1) OFFSET(U11),
            FrameError  WIDTH(U1) OFFSET(U12),
            Overrrun    WIDTH(U1) OFFSET(U13),
            Error       WIDTH(U1) OFFSET(U14),
            ChrRdy      WIDTH(U1) OFFSET(U15)
        ]
    }

    const TX_OFFSET: usize = 1 << 6;

    register! {
        UartTX,
        WO,
        Fields [
            Data WIDTH(U8) OFFSET(U0)
        ]
    }

    const CTL1_OFFSET: usize = 1 << 7;

    register! {
        UartControl1,
        RW,
        Fields [
            Enable              WIDTH(U1) OFFSET(U0),
            Doze                WIDTH(U1) OFFSET(U1),
            AgingDMATimerEnable WIDTH(U1) OFFSET(U2),
            TxRdyDMAENable      WIDTH(U1) OFFSET(U3),
            SendBreak           WIDTH(U1) OFFSET(U4),
            RTSDeltaInterrupt   WIDTH(U1) OFFSET(U5),
            TxEmptyInterrupt    WIDTH(U1) OFFSET(U6),
            Infrared            WIDTH(U1) OFFSET(U7),
            RecvReadyDMA        WIDTH(U1) OFFSET(U8),
            RecvReadyInterrupt  WIDTH(U1) OFFSET(U9),
            IdleCondition       WIDTH(U2) OFFSET(U10),
            IdleInterrupt       WIDTH(U1) OFFSET(U12),
            TxReadyInterrupt    WIDTH(U1) OFFSET(U13),
            AutoBaud            WIDTH(U1) OFFSET(U14),
            AutoBaudInterrupt   WIDTH(U1) OFFSET(U15)
        ]
    }

    register! {
        UartControl2,
        RW,
        Fields [
            SoftwareReset      WIDTH(U1) OFFSET(U0),
            RxEnable           WIDTH(U1) OFFSET(U1),
            TxEnable           WIDTH(U1) OFFSET(U2),
            AgingTimer         WIDTH(U1) OFFSET(U3),
            ReqSendInterrupt   WIDTH(U1) OFFSET(U4),
            WordSize           WIDTH(U1) OFFSET(U5),
            TwoStopBits        WIDTH(U1) OFFSET(U6),
            ParityOddEven      WIDTH(U1) OFFSET(U7),
            ParityEnable       WIDTH(U1) OFFSET(U8),
            RequestToSendEdge  WIDTH(U2) OFFSET(U9),
            Escape             WIDTH(U1) OFFSET(U11),
            ClearToSend        WIDTH(U1) OFFSET(U12),
            ClearToSendControl WIDTH(U1) OFFSET(U13),
            IgnoreRTS          WIDTH(U1) OFFSET(U14),
            EscapeInterrupt    WIDTH(U1) OFFSET(U15)
        ]
    }

    #[repr(C)]
    struct UartBlock {
        rx: UartRX::Register,
        _padding1: [u32; 15],
        tx: UartTX::Register,
        _padding2: [u32; 15],
        control1: UartControl1::Register,
        control2: UartControl2::Register,
    }

    struct Uart {
        addr: u32,
    }

    impl Uart {
        fn get(&self) -> Option<u8> {
            let ub = self.addr as *mut UartBlock;
            unsafe {
                (*ub)
                    .rx
                    .get_field(UartRX::Data::Read)
                    .map(|field| field.val() as u8)
            }
        }

        fn put(&mut self, data: u8) {
            let checked = UartTX::Data::Field::new(data as u32).expect("uart data out of bounds");
            let ub = self.addr as *mut UartBlock;
            unsafe { (*ub).tx.modify(checked) };
        }
    }

    use core::ptr;

    pub extern "C" fn run<IRQ: Unsigned + Sync + Send>(params: UartParams<IRQ, role::Local>)
    where
        IRQ: IsLess<U256, Output = True>,
    {
        let mut uart = Uart {
            addr: params.base_ptr as u32,
        };

        let ub = uart.addr as *mut UartBlock;

        unsafe {
            // Writing a 0 to the SoftwareReset field causes the
            // hardware to reset. In order for us to not reset the
            // UART with any write to control register 2, we need to
            // set this field to 1. After that, any modify call will
            // put a 1 back in this field, avoiding the triggering
            // of a reset.
            (*ub)
                .control2
                .modify(UartControl2::SoftwareReset::Field::checked::<U1>());
            (*ub)
                .control1
                .modify(UartControl1::RecvReadyInterrupt::Field::checked::<U1>());
        }

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