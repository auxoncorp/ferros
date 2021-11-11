#![no_std]
#![no_main]

use selfe_runtime as _;

use debug_logger::DebugLogger;
use enet::ProcParams;
use ferros::cap::role;
use ferros::userland::Producer;
use imx6_hal::enet::{uncached_memory_region::UncachedMemoryRegion, Enet};
use imx6_hal::pac::typenum::Unsigned;
use net_types::IpcEthernetFrame;

static LOGGER: DebugLogger = DebugLogger;

#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(DebugLogger::max_log_level_from_env()))
        .unwrap();

    log::debug!("[enet-driver] Process started");

    let dma_mem = params.dma_mem;
    dma_mem.flush().unwrap();

    // Downgrade to something more easily managed by the HAL
    let mut dma_mem = unsafe {
        UncachedMemoryRegion::new(
            dma_mem.vaddr(),
            dma_mem.paddr().unwrap(),
            dma_mem.size_bytes(),
        )
    };
    log::trace!("[enet-driver] DMA memory {}", dma_mem);

    let pkt_mem = dma_mem.split_off(ferros::arch::PageBytes::USIZE).unwrap();
    let desc_mem = dma_mem;

    log::trace!("[enet-driver] Descriptor pool {}", desc_mem);
    log::trace!("[enet-driver] Packet pool {}", pkt_mem);

    let mut enet = Enet::new(params.enet, params.mac_addr, desc_mem, pkt_mem).unwrap();

    enet.reset();

    // TODO - ipc to do the clock configs and IOMUX'ing

    enet.init();

    struct State {
        enet: Enet,
        producer: Producer<role::Local, IpcEthernetFrame>,
    }

    let producer_qlen = params.producer.capacity();
    let initial_state = State {
        enet,
        producer: params.producer,
    };

    params.consumer.consume(
        initial_state,
        |mut state| {
            // Non-queue IRQ wakeup event
            log::trace!("[enet-driver] IRQ wakeup");

            let rx_ready = state.enet.ack_irqs();

            // Attempt to drain up to qlen worth of packets from the rx ring
            if rx_ready {
                for _ in 0..producer_qlen {
                    let mut rx_frame = IpcEthernetFrame::new();
                    let bytes_recvd = state.enet.receive(|pkt| {
                        log::trace!("[enet-driver] Dequeue rx packet {} bytes", pkt.len());
                        rx_frame.truncate(pkt.len());
                        rx_frame.as_mut_slice().copy_from_slice(pkt);
                    });

                    if bytes_recvd != 0 {
                        if state.producer.send(rx_frame).is_err() {
                            log::warn!("[enet-driver] Rejected sending IpcEthernetFrame");
                        }
                    } else {
                        // Break out early if the rx ring is empty
                        break;
                    }
                }
            }

            state
        },
        |tx_frame, mut state| {
            // Transmit request queue

            log::trace!("[enet-driver] Enqueue {}", tx_frame);

            if let Err(e) = state.enet.transmit(tx_frame.as_slice()) {
                log::warn!("[enet-driver] Failed to transmit IpcEthernetFrame {:?}", e);
            }

            state
        },
    );
}
