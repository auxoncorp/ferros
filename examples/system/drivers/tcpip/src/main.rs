#![no_std]
#![no_main]

use selfe_runtime as _;

use crate::ipc_phy_dev::IpcPhyDevice;
use debug_logger::DebugLogger;
use ferros::cap::role;
use imx6_hal::{
    embedded_hal::timer::CountDown,
    timer::{Event as TimerEvent, Hertz, Timer},
};
use net_types::IpcUdpTransmitBuffer;
use smoltcp::iface::{EthernetInterface, EthernetInterfaceBuilder, NeighborCache, Routes};
use smoltcp::socket::{SocketHandle, SocketSet, UdpPacketMetadata, UdpSocket, UdpSocketBuffer};
use smoltcp::time::Instant;
use smoltcp::wire::{IpCidr, IpEndpoint};
use tcpip::ProcParams;

mod ipc_phy_dev;

/// Maximum number of ARP (Neighbor) cache entries
/// available in the storage
const MAX_ARP_ENTRIES: usize = 8;

const EPHEMERAL_PORT: u16 = 49152;

const TIMER_RATE: Hertz = Hertz(100);
const TIMER_MS_PER_TICK: u32 = 1000 / TIMER_RATE.0;

static LOGGER: DebugLogger = DebugLogger;

#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(DebugLogger::max_log_level_from_env()))
        .unwrap();

    log::debug!("[tcpip-driver] Process started");

    let ipc_phy = IpcPhyDevice {
        consumer: params.frame_consumer,
        producer: params.frame_producer,
    };

    // Build the IP stack
    let ip_addr = IpCidr::new(smoltcp::wire::Ipv4Address(params.ip_addr.into()).into(), 24);
    let mut ip_addrs = [ip_addr];
    let mut neighbor_storage = [None; MAX_ARP_ENTRIES];
    let neighbor_cache = NeighborCache::new(&mut neighbor_storage[..]);

    let ethernet_addr = smoltcp::wire::EthernetAddress(params.mac_addr.into());
    let mut routes_storage = [None; 4];
    let routes = Routes::new(&mut routes_storage[..]);

    let iface = EthernetInterfaceBuilder::new(ipc_phy)
        .ethernet_addr(ethernet_addr)
        .ip_addrs(&mut ip_addrs[..])
        .neighbor_cache(neighbor_cache)
        .routes(routes)
        .finalize();

    // Only capacity for a single UDP socket
    let mut sockets_storage = [None];
    let mut sockets = SocketSet::new(&mut sockets_storage[..]);

    // Split up the memory for socket rx/tx buffers
    let socket_mem = params.socket_buffer_mem;
    socket_mem.flush().unwrap();
    let (mut rx_mem, mut tx_mem) = socket_mem.split().unwrap();

    let mut rx_meta = [UdpPacketMetadata::EMPTY];
    let mut tx_meta = [UdpPacketMetadata::EMPTY];
    let udp_socket = UdpSocket::new(
        UdpSocketBuffer::new(&mut rx_meta[..], rx_mem.as_mut_slice()),
        UdpSocketBuffer::new(&mut tx_meta[..], tx_mem.as_mut_slice()),
    );

    let udp_handle = sockets.add(udp_socket);

    // The UDP handle is used to fulfill transmits only
    // so we can bind it now to an arbitrary local port
    sockets
        .get::<UdpSocket>(udp_handle)
        .bind(EPHEMERAL_PORT)
        .unwrap();

    let mut timer = Timer::new(params.gpt);
    timer.start(TIMER_RATE);
    timer.listen(TimerEvent::TimeOut);

    log::debug!(
        "[tcpip-driver] TCP/IP stack is up IP={} MAC={}",
        params.ip_addr,
        params.mac_addr
    );

    let initial_state = Driver {
        iface,
        sockets,
        udp_handle,
        timer,
        timer_ms: 0,
    };

    params.event_consumer.consume(
        initial_state,
        |mut state| {
            // Non-queue wakeup event
            //log::trace!("[tcpip-driver IRQ wakeup");

            // Ack timer interrupt
            state.ack_timer_irq();

            // Service the IP stack,
            state.poll();

            state
        },
        |udp_transmit_buffer, mut state| {
            // UDP transmit buffer queue
            log::trace!("[tcpip-driver] Processing {}", udp_transmit_buffer);
            state.handle_udp_tx_buffer(udp_transmit_buffer);

            // Service the IP stack,
            state.poll();

            state
        },
    );
}

struct Driver<'a> {
    iface: EthernetInterface<'a, IpcPhyDevice>,
    sockets: SocketSet<'a>,
    udp_handle: SocketHandle,
    timer: Timer,
    timer_ms: i64,
}

impl<'a> Driver<'a> {
    pub fn ack_timer_irq(&mut self) {
        self.timer.wait().ok();
        self.timer_ms = self.timer_ms.wrapping_add(TIMER_MS_PER_TICK.into());
    }

    pub fn get_time(&self) -> Instant {
        Instant::from_millis(self.timer_ms)
    }

    pub fn poll(&mut self) {
        let time = self.get_time();
        if let Err(e) = self.iface.poll(&mut self.sockets, time) {
            log::trace!("[tcpip-driver] {:?}", e);
        }
    }

    pub fn handle_udp_tx_buffer(&mut self, udp_tx: IpcUdpTransmitBuffer) {
        let endpoint = IpEndpoint::new(
            smoltcp::wire::Ipv4Address(udp_tx.dst_addr.0).into(),
            udp_tx.dst_port.0,
        );

        if let Err(e) = self
            .sockets
            .get::<UdpSocket>(self.udp_handle)
            .send_slice(udp_tx.frame.as_slice(), endpoint)
        {
            log::warn!("[tcpip-driver] Failed to send UDP transmit buffer, {}", e);
        }
    }
}
