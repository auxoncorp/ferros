//! ENET
//! See [IMX6DQRM](http://cache.freescale.com/files/32bit/doc/ref_manual/IMX6DQRM.pdf) chapter 23.

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::{Unsigned, U150};

pub type Irq = U150;

register! {
    InterruptEvent,
    u32,
    RW,
    Fields [
        TsTimer             WIDTH(U1) OFFSET(U15),
        TsAvail             WIDTH(U1) OFFSET(U16),
        Wakeup              WIDTH(U1) OFFSET(U17),
        PayloadRxErr        WIDTH(U1) OFFSET(U18),
        TxFifoUnderrun      WIDTH(U1) OFFSET(U19),
        CollisionRetryLimit WIDTH(U1) OFFSET(U20),
        LateCollision       WIDTH(U1) OFFSET(U21),
        BusErr              WIDTH(U1) OFFSET(U22),
        Mii                 WIDTH(U1) OFFSET(U23),
        RxBuffer            WIDTH(U1) OFFSET(U24),
        RxFrame             WIDTH(U1) OFFSET(U25),
        TxBuffer            WIDTH(U1) OFFSET(U26),
        TxFrame             WIDTH(U1) OFFSET(U27),
        GStopComple         WIDTH(U1) OFFSET(U28),
        BTxErr              WIDTH(U1) OFFSET(U29),
        BRxErr              WIDTH(U1) OFFSET(U30),
    ]
}

register! {
    InterruptMask,
    u32,
    RW,
    Fields [
        TsTimer             WIDTH(U1) OFFSET(U15),
        TsAvail             WIDTH(U1) OFFSET(U16),
        Wakeup              WIDTH(U1) OFFSET(U17),
        PayloadRxErr        WIDTH(U1) OFFSET(U18),
        TxFifoUnderrun      WIDTH(U1) OFFSET(U19),
        CollisionRetryLimit WIDTH(U1) OFFSET(U20),
        LateCollision       WIDTH(U1) OFFSET(U21),
        BusErr              WIDTH(U1) OFFSET(U22),
        Mii                 WIDTH(U1) OFFSET(U23),
        RxBuffer            WIDTH(U1) OFFSET(U24),
        RxFrame             WIDTH(U1) OFFSET(U25),
        TxBuffer            WIDTH(U1) OFFSET(U26),
        TxFrame             WIDTH(U1) OFFSET(U27),
        GStopComple         WIDTH(U1) OFFSET(U28),
        BTxErr              WIDTH(U1) OFFSET(U29),
        BRxErr              WIDTH(U1) OFFSET(U30),
    ]
}

register! {
    RxDescActive,
    u32,
    RW,
    Fields [
        RxDescActive  WIDTH(U1) OFFSET(U24),
    ]
}

register! {
    TxDescActive,
    u32,
    RW,
    Fields [
        TxDescActive  WIDTH(U1) OFFSET(U24),
    ]
}

register! {
    Control,
    u32,
    RW,
    Fields [
        Reset           WIDTH(U1) OFFSET(U0),
        Enable          WIDTH(U1) OFFSET(U1),
        Enable1588      WIDTH(U1) OFFSET(U4),
        Speed           WIDTH(U1) OFFSET(U5),
        DescByteSwap    WIDTH(U1) OFFSET(U8),
    ]
}

register! {
    MiiMf,
    u32,
    RW,
    Fields [
        Data            WIDTH(U16) OFFSET(U0),
        TurnAround      WIDTH(U2) OFFSET(U16) [
            Valid = U2
        ]
        RegisterAddress WIDTH(U5) OFFSET(U18),
        PhyAddress      WIDTH(U5) OFFSET(U23),
        OpCode          WIDTH(U2) OFFSET(U28) [
            WriteOp = U1,
            ReadOp  = U2
        ]
        StartOfFrame    WIDTH(U2) OFFSET(U30) [
            Standard = U1
        ]
    ]
}

register! {
    MibControl,
    u32,
    RW,
    Fields [
        Clear           WIDTH(U1) OFFSET(U29),
        Idle            WIDTH(U1) OFFSET(U30),
        Disable         WIDTH(U1) OFFSET(U31),
    ]
}

register! {
    PhysicalAddressLower,
    u32,
    RW,
    Fields [
        Octet3 WIDTH(U8) OFFSET(U0),
        Octet2 WIDTH(U8) OFFSET(U8),
        Octet1 WIDTH(U8) OFFSET(U16),
        Octet0 WIDTH(U8) OFFSET(U24),
    ]
}

register! {
    PhysicalAddressUpper,
    u32,
    RW,
    Fields [
        Type        WIDTH(U16) OFFSET(U0),
        Octet5      WIDTH(U8) OFFSET(U16),
        Octet4      WIDTH(U8) OFFSET(U24),
    ]
}

register! {
    OpcodePauseDuration,
    u32,
    RW,
    Fields [
        Duration    WIDTH(U16) OFFSET(U0),
        Opcode      WIDTH(U16) OFFSET(U16),
    ]
}

register! {
    TxIpg,
    u32,
    RW,
    Fields [
        Ipg WIDTH(U5) OFFSET(U0),
    ]
}

register! {
    TxFifoWatermark,
    u32,
    RW,
    Fields [
        FifoWrite            WIDTH(U6) OFFSET(U0),
        StoreAndFowardEnable WIDTH(U1) OFFSET(U8),
    ]
}

register! {
    RxAccelFnConfig,
    u32,
    RW,
    Fields [
        PadRem          WIDTH(U1) OFFSET(U0),
        IpDiscard       WIDTH(U1) OFFSET(U1),
        ProtoDiscard    WIDTH(U1) OFFSET(U2),
        LineDiscard     WIDTH(U1) OFFSET(U6),
        Shift16         WIDTH(U1) OFFSET(U7),
    ]
}

register! {
    RxControl,
    u32,
    RW,
    Fields [
        Loop                WIDTH(U1) OFFSET(U0),
        Drt                 WIDTH(U1) OFFSET(U1),
        MiiMode             WIDTH(U1) OFFSET(U2),
        Prom                WIDTH(U1) OFFSET(U3),
        BcastReject         WIDTH(U1) OFFSET(U4),
        FlowControlEnable   WIDTH(U1) OFFSET(U5),
        RgmiiEnable         WIDTH(U1) OFFSET(U6),
        RmiiMode            WIDTH(U1) OFFSET(U8),
        Rmii10t             WIDTH(U1) OFFSET(U9),
        PadEnable           WIDTH(U1) OFFSET(U12),
        PauseForward        WIDTH(U1) OFFSET(U13),
        CrcForward          WIDTH(U1) OFFSET(U14),
        CfEnable            WIDTH(U1) OFFSET(U15),
        MaxFrameLength      WIDTH(U14) OFFSET(U16),
        Nlc                 WIDTH(U1) OFFSET(U30),
        Grs                 WIDTH(U1) OFFSET(U31),
    ]
}

register! {
    TxControl,
    u32,
    RW,
    Fields [
        Gts                 WIDTH(U1) OFFSET(U0),
        FdEnable            WIDTH(U1) OFFSET(U2),
        TfcPause            WIDTH(U1) OFFSET(U3),
        RfcPause            WIDTH(U1) OFFSET(U4),
        AddrSelect          WIDTH(U3) OFFSET(U5),
        AddrIns             WIDTH(U1) OFFSET(U8),
        CrcForward          WIDTH(U1) OFFSET(U9),
    ]
}

register! {
    MaxRxBufferSize,
    u32,
    RW,
    Fields [
        BufSize  WIDTH(U11) OFFSET(U0),
    ]
}

register! {
    Data,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x628);

#[repr(C)]
pub struct RegisterBlock {
    __reserved_0: [u32; 1],                   // 0x000
    pub eir: InterruptEvent::Register,        // 0x004
    pub eimr: InterruptMask::Register,        // 0x008
    __reserved_1: [u32; 1],                   // 0x00C
    pub rdar: RxDescActive::Register,         // 0x010
    pub tdar: TxDescActive::Register,         // 0x014
    __reserved_2: [u32; 3],                   // 0x018
    pub ecr: Control::Register,               // 0x024
    __reserved_3: [u32; 6],                   // 0x028
    pub mmfr: MiiMf::Register,                // 0x040
    pub mscr: Data::Register,                 // 0x044
    __reserved_4: [u32; 7],                   // 0x048
    pub mibc: MibControl::Register,           // 0x064
    __reserved_5: [u32; 7],                   // 0x068
    pub rcr: RxControl::Register,             // 0x084
    __reserved_6: [u32; 15],                  // 0x088
    pub tcr: TxControl::Register,             // 0x0C4
    __reserved_7: [u32; 7],                   // 0x0C8
    pub palr: PhysicalAddressLower::Register, // 0x0E4
    pub paur: PhysicalAddressUpper::Register, // 0x0E8
    pub opd: OpcodePauseDuration::Register,   // 0x0EC
    __reserved_8: [u32; 10],                  // 0x0F0
    pub iaur: Data::Register,                 // 0x118
    pub ialr: Data::Register,                 // 0x11C
    pub gaur: Data::Register,                 // 0x120
    pub galr: Data::Register,                 // 0x124
    __reserved_9: [u32; 7],                   // 0x128
    pub tfwr: TxFifoWatermark::Register,      // 0x144
    __reserved_10: [u32; 14],                 // 0x148
    pub rdsr: Data::Register,                 // 0x180
    pub tdsr: Data::Register,                 // 0x184
    pub mrbr: MaxRxBufferSize::Register,      // 0x188
    __reserved_11: [u32; 1],                  // 0x18C
    pub rsfl: Data::Register,                 // 0x190
    pub rsem: Data::Register,                 // 0x194
    pub raem: Data::Register,                 // 0x198
    pub rafl: Data::Register,                 // 0x19C
    pub tsem: Data::Register,                 // 0x1A0
    pub taem: Data::Register,                 // 0x1A4
    pub tafl: Data::Register,                 // 0x1A8
    pub tipg: TxIpg::Register,                // 0x1AC
    pub ftrl: Data::Register,                 // 0x1B0
    __reserved_12: [u32; 3],                  // 0x1B4
    pub tacc: Data::Register,                 // 0x1C0
    pub racc: RxAccelFnConfig::Register,      // 0x1C4
    __reserved_13: [u32; 14],                 // 0x1C8
    pub rmon_t_drop: Data::Register,          // 0x200
    pub rmon_t_packets: Data::Register,       // 0x204
    pub rmon_t_bc_pkt: Data::Register,        // 0x208
    pub rmon_t_mc_pkt: Data::Register,        // 0x20C
    pub rmon_t_crc_align: Data::Register,     // 0x210
    pub rmon_t_undersize: Data::Register,     // 0x214
    pub rmon_t_oversize: Data::Register,      // 0x218
    pub rmon_t_frag: Data::Register,          // 0x21C
    pub rmon_t_jab: Data::Register,           // 0x220
    pub rmon_t_col: Data::Register,           // 0x224
    pub rmon_t_p64: Data::Register,           // 0x228
    pub rmon_t_p65to127n: Data::Register,     // 0x22C
    pub rmon_t_p128to255n: Data::Register,    // 0x230
    pub rmon_t_p256to511: Data::Register,     // 0x234
    pub rmon_t_p512to1023: Data::Register,    // 0x238
    pub rmon_t_p1024to2047: Data::Register,   // 0x23C
    pub rmon_t_p_gte2048: Data::Register,     // 0x240
    pub rmon_t_octets: Data::Register,        // 0x244
    pub ieee_t_drop: Data::Register,          // 0x248
    pub ieee_t_frame_ok: Data::Register,      // 0x24C
    pub ieee_t_1col: Data::Register,          // 0x250
    pub ieee_t_mcol: Data::Register,          // 0x254
    pub ieee_t_def: Data::Register,           // 0x258
    pub ieee_t_lcol: Data::Register,          // 0x25C
    pub ieee_t_excol: Data::Register,         // 0x260
    pub ieee_t_macerr: Data::Register,        // 0x264
    pub ieee_t_cserr: Data::Register,         // 0x268
    pub ieee_t_sqe: Data::Register,           // 0x26C
    pub ieee_t_fdxfc: Data::Register,         // 0x270
    pub ieee_t_octets_ok: Data::Register,     // 0x274
    __reserved_14: [u32; 3],                  // 0x278
    pub rmon_r_packets: Data::Register,       // 0x284
    pub rmon_r_bc_pkt: Data::Register,        // 0x288
    pub rmon_r_mc_pkt: Data::Register,        // 0x28C
    pub rmon_r_crc_align: Data::Register,     // 0x290
    pub rmon_r_undersize: Data::Register,     // 0x294
    pub rmon_r_oversize: Data::Register,      // 0x298
    pub rmon_r_frag: Data::Register,          // 0x29C
    pub rmon_r_jab: Data::Register,           // 0x2A0
    pub rmon_r_resvd_0: Data::Register,       // 0x2A4
    pub rmon_r_p64: Data::Register,           // 0x2A8
    pub rmon_r_p65to127: Data::Register,      // 0x2AC
    pub rmon_r_p128to255: Data::Register,     // 0x2B0
    pub rmon_r_p256to511: Data::Register,     // 0x2B4
    pub rmon_r_p512to1023: Data::Register,    // 0x2B8
    pub rmon_r_p1024to2047: Data::Register,   // 0x2BC
    pub rmon_r_p_gte2048: Data::Register,     // 0x2C0
    pub rmon_r_octets: Data::Register,        // 0x2C4
    pub ieee_r_drop: Data::Register,          // 0x2C8
    pub ieee_r_frame_ok: Data::Register,      // 0x2CC
    pub ieee_r_crc: Data::Register,           // 0x2D0
    pub ieee_r_align: Data::Register,         // 0x2D4
    pub ieee_r_macerr: Data::Register,        // 0x2D8
    pub ieee_r_fdxfc: Data::Register,         // 0x2DC
    pub ieee_r_octets_ok: Data::Register,     // 0x2E0
    __reserved_15: [u32; 7],                  // 0x2E4
    __reserved_16: [u32; 64],                 // 0x300
    pub atcr: Data::Register,                 // 0x400
    pub atvr: Data::Register,                 // 0x404
    pub atoff: Data::Register,                // 0x408
    pub atper: Data::Register,                // 0x40C
    pub atcor: Data::Register,                // 0x410
    pub atinc: Data::Register,                // 0x414
    pub atstmp: Data::Register,               // 0x418
    __reserved_17: [u32; 121],                // 0x41C
    __reserved_18: [u32; 1],                  // 0x600
    pub tgsr: Data::Register,                 // 0x604
    pub tcsr0: Data::Register,                // 0x608
    pub tccr0: Data::Register,                // 0x60C
    pub tcsr1: Data::Register,                // 0x610
    pub tccr1: Data::Register,                // 0x614
    pub tcsr2: Data::Register,                // 0x618
    pub tccr2: Data::Register,                // 0x61C
    pub tcsr3: Data::Register,                // 0x620
    pub tccr3: Data::Register,                // 0x624
}

pub struct ENET {
    vaddr: u32,
}

impl ENET {
    pub const PADDR: u32 = 0x0218_8000;
    pub const SIZE: usize = crate::PageBytes::USIZE;

    /// # Safety
    /// out of thin air
    pub unsafe fn from_vaddr(vaddr: u32) -> Self {
        Self { vaddr }
    }

    fn as_ptr(&self) -> *const RegisterBlock {
        self.vaddr as *const _
    }

    fn as_mut_ptr(&mut self) -> *mut RegisterBlock {
        self.vaddr as *mut _
    }
}

impl Deref for ENET {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for ENET {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
