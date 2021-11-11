//! IOMUXC
//! See [IMX6DQRM](http://cache.freescale.com/files/32bit/doc/ref_manual/IMX6DQRM.pdf) chapter 36.

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::Unsigned;

register! {
    Gpr,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    MuxControl,
    u32,
    RW,
    Fields [
        MuxMode  WIDTH(U3) OFFSET(U0) [
            ALT0 = U0,
            ALT1 = U1,
            ALT2 = U2,
            ALT3 = U3,
            ALT4 = U4,
            ALT5 = U5,
            ALT6 = U6,
            ALT7 = U7
        ]
        Sion    WIDTH(U1) OFFSET(U4)
    ]
}

register! {
    SelectInput,
    u32,
    RW,
    Fields [
        Daisy  WIDTH(U2) OFFSET(U0)
    ]
}

register! {
    PadControl,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x950);

#[repr(C)]
pub struct RegisterBlock {
    pub gpr0: Gpr::Register,                                     // 0x000
    pub gpr1: Gpr::Register,                                     // 0x004
    pub gpr2: Gpr::Register,                                     // 0x008
    pub gpr3: Gpr::Register,                                     // 0x00C
    pub gpr4: Gpr::Register,                                     // 0x010
    pub gpr5: Gpr::Register,                                     // 0x014
    pub gpr6: Gpr::Register,                                     // 0x018
    pub gpr7: Gpr::Register,                                     // 0x01C
    pub gpr8: Gpr::Register,                                     // 0x020
    pub gpr9: Gpr::Register,                                     // 0x024
    pub gpr10: Gpr::Register,                                    // 0x028
    pub gpr11: Gpr::Register,                                    // 0x02C
    pub gpr12: Gpr::Register,                                    // 0x030
    pub gpr13: Gpr::Register,                                    // 0x034
    pub reserved_0: MuxControl::Register,                        // 0x038
    pub reserved_1: MuxControl::Register,                        // 0x03C
    pub reserved_2: MuxControl::Register,                        // 0x040
    pub reserved_3: MuxControl::Register,                        // 0x044
    pub reserved_4: MuxControl::Register,                        // 0x048
    pub sw_mux_ctl_pad_sd2_data1: MuxControl::Register,          // 0x04C
    pub sw_mux_ctl_pad_sd2_data2: MuxControl::Register,          // 0x050
    pub sw_mux_ctl_pad_sd2_data0: MuxControl::Register,          // 0x054
    pub sw_mux_ctl_pad_rgmii_txc: MuxControl::Register,          // 0x058
    pub sw_mux_ctl_pad_rgmii_td0: MuxControl::Register,          // 0x05C
    pub sw_mux_ctl_pad_rgmii_td1: MuxControl::Register,          // 0x060
    pub sw_mux_ctl_pad_rgmii_td2: MuxControl::Register,          // 0x064
    pub sw_mux_ctl_pad_rgmii_td3: MuxControl::Register,          // 0x068
    pub sw_mux_ctl_pad_rgmii_rx_ctl: MuxControl::Register,       // 0x06C
    pub sw_mux_ctl_pad_rgmii_rd0: MuxControl::Register,          // 0x070
    pub sw_mux_ctl_pad_rgmii_tx_ctl: MuxControl::Register,       // 0x074
    pub sw_mux_ctl_pad_rgmii_rd1: MuxControl::Register,          // 0x078
    pub sw_mux_ctl_pad_rgmii_rd2: MuxControl::Register,          // 0x07C
    pub sw_mux_ctl_pad_rgmii_rd3: MuxControl::Register,          // 0x080
    pub sw_mux_ctl_pad_rgmii_rxc: MuxControl::Register,          // 0x084
    pub sw_mux_ctl_pad_eim_addr25: MuxControl::Register,         // 0x088
    pub sw_mux_ctl_pad_eim_eb2: MuxControl::Register,            // 0x08C
    pub sw_mux_ctl_pad_eim_data16: MuxControl::Register,         // 0x090
    pub sw_mux_ctl_pad_eim_data17: MuxControl::Register,         // 0x094
    pub sw_mux_ctl_pad_eim_data18: MuxControl::Register,         // 0x098
    pub sw_mux_ctl_pad_eim_data19: MuxControl::Register,         // 0x09C
    pub sw_mux_ctl_pad_eim_data20: MuxControl::Register,         // 0x0A0
    pub sw_mux_ctl_pad_eim_data21: MuxControl::Register,         // 0x0A4
    pub sw_mux_ctl_pad_eim_data22: MuxControl::Register,         // 0x0A8
    pub sw_mux_ctl_pad_eim_data23: MuxControl::Register,         // 0x0AC
    pub sw_mux_ctl_pad_eim_eb3: MuxControl::Register,            // 0x0B0
    pub sw_mux_ctl_pad_eim_data24: MuxControl::Register,         // 0x0B4
    pub sw_mux_ctl_pad_eim_data25: MuxControl::Register,         // 0x0B8
    pub sw_mux_ctl_pad_eim_data26: MuxControl::Register,         // 0x0BC
    pub sw_mux_ctl_pad_eim_data27: MuxControl::Register,         // 0x0C0
    pub sw_mux_ctl_pad_eim_data28: MuxControl::Register,         // 0x0C4
    pub sw_mux_ctl_pad_eim_data29: MuxControl::Register,         // 0x0C8
    pub sw_mux_ctl_pad_eim_data30: MuxControl::Register,         // 0x0CC
    pub sw_mux_ctl_pad_eim_data31: MuxControl::Register,         // 0x0D0
    pub sw_mux_ctl_pad_eim_addr24: MuxControl::Register,         // 0x0D4
    pub sw_mux_ctl_pad_eim_addr23: MuxControl::Register,         // 0x0D8
    pub sw_mux_ctl_pad_eim_addr22: MuxControl::Register,         // 0x0DC
    pub sw_mux_ctl_pad_eim_addr21: MuxControl::Register,         // 0x0E0
    pub sw_mux_ctl_pad_eim_addr20: MuxControl::Register,         // 0x0E4
    pub sw_mux_ctl_pad_eim_addr19: MuxControl::Register,         // 0x0E8
    pub sw_mux_ctl_pad_eim_addr18: MuxControl::Register,         // 0x0EC
    pub sw_mux_ctl_pad_eim_addr17: MuxControl::Register,         // 0x0F0
    pub sw_mux_ctl_pad_eim_addr16: MuxControl::Register,         // 0x0F4
    pub sw_mux_ctl_pad_eim_cs0: MuxControl::Register,            // 0x0F8
    pub sw_mux_ctl_pad_eim_cs1: MuxControl::Register,            // 0x0FC
    pub sw_mux_ctl_pad_eim_oe: MuxControl::Register,             // 0x100
    pub sw_mux_ctl_pad_eim_rw: MuxControl::Register,             // 0x104
    pub sw_mux_ctl_pad_eim_lba: MuxControl::Register,            // 0x108
    pub sw_mux_ctl_pad_eim_eb0: MuxControl::Register,            // 0x10C
    pub sw_mux_ctl_pad_eim_eb1: MuxControl::Register,            // 0x110
    pub sw_mux_ctl_pad_eim_ad00: MuxControl::Register,           // 0x114
    pub sw_mux_ctl_pad_eim_ad01: MuxControl::Register,           // 0x118
    pub sw_mux_ctl_pad_eim_ad02: MuxControl::Register,           // 0x11C
    pub sw_mux_ctl_pad_eim_ad03: MuxControl::Register,           // 0x120
    pub sw_mux_ctl_pad_eim_ad04: MuxControl::Register,           // 0x124
    pub sw_mux_ctl_pad_eim_ad05: MuxControl::Register,           // 0x128
    pub sw_mux_ctl_pad_eim_ad06: MuxControl::Register,           // 0x12C
    pub sw_mux_ctl_pad_eim_ad07: MuxControl::Register,           // 0x130
    pub sw_mux_ctl_pad_eim_ad08: MuxControl::Register,           // 0x134
    pub sw_mux_ctl_pad_eim_ad09: MuxControl::Register,           // 0x138
    pub sw_mux_ctl_pad_eim_ad10: MuxControl::Register,           // 0x13C
    pub sw_mux_ctl_pad_eim_ad11: MuxControl::Register,           // 0x140
    pub sw_mux_ctl_pad_eim_ad12: MuxControl::Register,           // 0x144
    pub sw_mux_ctl_pad_eim_ad13: MuxControl::Register,           // 0x148
    pub sw_mux_ctl_pad_eim_ad14: MuxControl::Register,           // 0x14C
    pub sw_mux_ctl_pad_eim_ad15: MuxControl::Register,           // 0x150
    pub sw_mux_ctl_pad_eim_wait: MuxControl::Register,           // 0x154
    pub sw_mux_ctl_pad_eim_bclk: MuxControl::Register,           // 0x158
    pub sw_mux_ctl_pad_di0_disp_clk: MuxControl::Register,       // 0x15C
    pub sw_mux_ctl_pad_di0_pin15: MuxControl::Register,          // 0x160
    pub sw_mux_ctl_pad_di0_pin02: MuxControl::Register,          // 0x164
    pub sw_mux_ctl_pad_di0_pin03: MuxControl::Register,          // 0x168
    pub sw_mux_ctl_pad_di0_pin04: MuxControl::Register,          // 0x16C
    pub sw_mux_ctl_pad_disp0_data00: MuxControl::Register,       // 0x170
    pub sw_mux_ctl_pad_disp0_data01: MuxControl::Register,       // 0x174
    pub sw_mux_ctl_pad_disp0_data02: MuxControl::Register,       // 0x178
    pub sw_mux_ctl_pad_disp0_data03: MuxControl::Register,       // 0x17C
    pub sw_mux_ctl_pad_disp0_data04: MuxControl::Register,       // 0x180
    pub sw_mux_ctl_pad_disp0_data05: MuxControl::Register,       // 0x184
    pub sw_mux_ctl_pad_disp0_data06: MuxControl::Register,       // 0x188
    pub sw_mux_ctl_pad_disp0_data07: MuxControl::Register,       // 0x18C
    pub sw_mux_ctl_pad_disp0_data08: MuxControl::Register,       // 0x190
    pub sw_mux_ctl_pad_disp0_data09: MuxControl::Register,       // 0x194
    pub sw_mux_ctl_pad_disp0_data10: MuxControl::Register,       // 0x198
    pub sw_mux_ctl_pad_disp0_data11: MuxControl::Register,       // 0x19C
    pub sw_mux_ctl_pad_disp0_data12: MuxControl::Register,       // 0x1A0
    pub sw_mux_ctl_pad_disp0_data13: MuxControl::Register,       // 0x1A4
    pub sw_mux_ctl_pad_disp0_data14: MuxControl::Register,       // 0x1A8
    pub sw_mux_ctl_pad_disp0_data15: MuxControl::Register,       // 0x1AC
    pub sw_mux_ctl_pad_disp0_data16: MuxControl::Register,       // 0x1B0
    pub sw_mux_ctl_pad_disp0_data17: MuxControl::Register,       // 0x1B4
    pub sw_mux_ctl_pad_disp0_data18: MuxControl::Register,       // 0x1B8
    pub sw_mux_ctl_pad_disp0_data19: MuxControl::Register,       // 0x1BC
    pub sw_mux_ctl_pad_disp0_data20: MuxControl::Register,       // 0x1C0
    pub sw_mux_ctl_pad_disp0_data21: MuxControl::Register,       // 0x1C4
    pub sw_mux_ctl_pad_disp0_data22: MuxControl::Register,       // 0x1C8
    pub sw_mux_ctl_pad_disp0_data23: MuxControl::Register,       // 0x1CC
    pub sw_mux_ctl_pad_enet_mdio: MuxControl::Register,          // 0x1D0
    pub sw_mux_ctl_pad_enet_ref_clk: MuxControl::Register,       // 0x1D4
    pub sw_mux_ctl_pad_enet_rx_er: MuxControl::Register,         // 0x1D8
    pub sw_mux_ctl_pad_enet_crs_dv: MuxControl::Register,        // 0x1DC
    pub sw_mux_ctl_pad_enet_rx_data1: MuxControl::Register,      // 0x1E0
    pub sw_mux_ctl_pad_enet_rx_data0: MuxControl::Register,      // 0x1E4
    pub sw_mux_ctl_pad_enet_tx_en: MuxControl::Register,         // 0x1E8
    pub sw_mux_ctl_pad_enet_tx_data1: MuxControl::Register,      // 0x1EC
    pub sw_mux_ctl_pad_enet_tx_data0: MuxControl::Register,      // 0x1F0
    pub sw_mux_ctl_pad_enet_mdc: MuxControl::Register,           // 0x1F4
    pub sw_mux_ctl_pad_key_col0: MuxControl::Register,           // 0x1F8
    pub sw_mux_ctl_pad_key_row0: MuxControl::Register,           // 0x1FC
    pub sw_mux_ctl_pad_key_col1: MuxControl::Register,           // 0x200
    pub sw_mux_ctl_pad_key_row1: MuxControl::Register,           // 0x204
    pub sw_mux_ctl_pad_key_col2: MuxControl::Register,           // 0x208
    pub sw_mux_ctl_pad_key_row2: MuxControl::Register,           // 0x20C
    pub sw_mux_ctl_pad_key_col3: MuxControl::Register,           // 0x210
    pub sw_mux_ctl_pad_key_row3: MuxControl::Register,           // 0x214
    pub sw_mux_ctl_pad_key_col4: MuxControl::Register,           // 0x218
    pub sw_mux_ctl_pad_key_row4: MuxControl::Register,           // 0x21C
    pub sw_mux_ctl_pad_gpio00: MuxControl::Register,             // 0x220
    pub sw_mux_ctl_pad_gpio01: MuxControl::Register,             // 0x224
    pub sw_mux_ctl_pad_gpio09: MuxControl::Register,             // 0x228
    pub sw_mux_ctl_pad_gpio03: MuxControl::Register,             // 0x22C
    pub sw_mux_ctl_pad_gpio06: MuxControl::Register,             // 0x230
    pub sw_mux_ctl_pad_gpio02: MuxControl::Register,             // 0x234
    pub sw_mux_ctl_pad_gpio04: MuxControl::Register,             // 0x238
    pub sw_mux_ctl_pad_gpio05: MuxControl::Register,             // 0x23C
    pub sw_mux_ctl_pad_gpio07: MuxControl::Register,             // 0x240
    pub sw_mux_ctl_pad_gpio08: MuxControl::Register,             // 0x244
    pub sw_mux_ctl_pad_gpio16: MuxControl::Register,             // 0x248
    pub sw_mux_ctl_pad_gpio17: MuxControl::Register,             // 0x24C
    pub sw_mux_ctl_pad_gpio18: MuxControl::Register,             // 0x250
    pub sw_mux_ctl_pad_gpio19: MuxControl::Register,             // 0x254
    pub sw_mux_ctl_pad_csi0_pixclk: MuxControl::Register,        // 0x258
    pub sw_mux_ctl_pad_csi0_hsync: MuxControl::Register,         // 0x25C
    pub sw_mux_ctl_pad_csi0_data_en: MuxControl::Register,       // 0x260
    pub sw_mux_ctl_pad_csi0_vsync: MuxControl::Register,         // 0x264
    pub sw_mux_ctl_pad_csi0_data04: MuxControl::Register,        // 0x268
    pub sw_mux_ctl_pad_csi0_data05: MuxControl::Register,        // 0x26C
    pub sw_mux_ctl_pad_csi0_data06: MuxControl::Register,        // 0x270
    pub sw_mux_ctl_pad_csi0_data07: MuxControl::Register,        // 0x274
    pub sw_mux_ctl_pad_csi0_data08: MuxControl::Register,        // 0x278
    pub sw_mux_ctl_pad_csi0_data09: MuxControl::Register,        // 0x27C
    pub sw_mux_ctl_pad_csi0_data10: MuxControl::Register,        // 0x280
    pub sw_mux_ctl_pad_csi0_data11: MuxControl::Register,        // 0x284
    pub sw_mux_ctl_pad_csi0_data12: MuxControl::Register,        // 0x288
    pub sw_mux_ctl_pad_csi0_data13: MuxControl::Register,        // 0x28C
    pub sw_mux_ctl_pad_csi0_data14: MuxControl::Register,        // 0x290
    pub sw_mux_ctl_pad_csi0_data15: MuxControl::Register,        // 0x294
    pub sw_mux_ctl_pad_csi0_data16: MuxControl::Register,        // 0x298
    pub sw_mux_ctl_pad_csi0_data17: MuxControl::Register,        // 0x29C
    pub sw_mux_ctl_pad_csi0_data18: MuxControl::Register,        // 0x2A0
    pub sw_mux_ctl_pad_csi0_data19: MuxControl::Register,        // 0x2A4
    pub sw_mux_ctl_pad_sd3_data7: MuxControl::Register,          // 0x2A8
    pub sw_mux_ctl_pad_sd3_data6: MuxControl::Register,          // 0x2AC
    pub sw_mux_ctl_pad_sd3_data5: MuxControl::Register,          // 0x2B0
    pub sw_mux_ctl_pad_sd3_data4: MuxControl::Register,          // 0x2B4
    pub sw_mux_ctl_pad_sd3_cmd: MuxControl::Register,            // 0x2B8
    pub sw_mux_ctl_pad_sd3_clk: MuxControl::Register,            // 0x2BC
    pub sw_mux_ctl_pad_sd3_data0: MuxControl::Register,          // 0x2C0
    pub sw_mux_ctl_pad_sd3_data1: MuxControl::Register,          // 0x2C4
    pub sw_mux_ctl_pad_sd3_data2: MuxControl::Register,          // 0x2C8
    pub sw_mux_ctl_pad_sd3_data3: MuxControl::Register,          // 0x2CC
    pub sw_mux_ctl_pad_sd3_reset: MuxControl::Register,          // 0x2D0
    pub sw_mux_ctl_pad_nand_cle: MuxControl::Register,           // 0x2D4
    pub sw_mux_ctl_pad_nand_ale: MuxControl::Register,           // 0x2D8
    pub sw_mux_ctl_pad_nand_wp_b: MuxControl::Register,          // 0x2DC
    pub sw_mux_ctl_pad_nand_ready: MuxControl::Register,         // 0x2E0
    pub sw_mux_ctl_pad_nand_cs0_b: MuxControl::Register,         // 0x2E4
    pub sw_mux_ctl_pad_nand_cs1_b: MuxControl::Register,         // 0x2E8
    pub sw_mux_ctl_pad_nand_cs2_b: MuxControl::Register,         // 0x2EC
    pub sw_mux_ctl_pad_nand_cs3_b: MuxControl::Register,         // 0x2F0
    pub sw_mux_ctl_pad_sd4_cmd: MuxControl::Register,            // 0x2F4
    pub sw_mux_ctl_pad_sd4_clk: MuxControl::Register,            // 0x2F8
    pub sw_mux_ctl_pad_nand_data00: MuxControl::Register,        // 0x2FC
    pub sw_mux_ctl_pad_nand_data01: MuxControl::Register,        // 0x300
    pub sw_mux_ctl_pad_nand_data02: MuxControl::Register,        // 0x304
    pub sw_mux_ctl_pad_nand_data03: MuxControl::Register,        // 0x308
    pub sw_mux_ctl_pad_nand_data04: MuxControl::Register,        // 0x30C
    pub sw_mux_ctl_pad_nand_data05: MuxControl::Register,        // 0x310
    pub sw_mux_ctl_pad_nand_data06: MuxControl::Register,        // 0x314
    pub sw_mux_ctl_pad_nand_data07: MuxControl::Register,        // 0x318
    pub sw_mux_ctl_pad_sd4_data0: MuxControl::Register,          // 0x31C
    pub sw_mux_ctl_pad_sd4_data1: MuxControl::Register,          // 0x320
    pub sw_mux_ctl_pad_sd4_data2: MuxControl::Register,          // 0x324
    pub sw_mux_ctl_pad_sd4_data3: MuxControl::Register,          // 0x328
    pub sw_mux_ctl_pad_sd4_data4: MuxControl::Register,          // 0x32C
    pub sw_mux_ctl_pad_sd4_data5: MuxControl::Register,          // 0x330
    pub sw_mux_ctl_pad_sd4_data6: MuxControl::Register,          // 0x334
    pub sw_mux_ctl_pad_sd4_data7: MuxControl::Register,          // 0x338
    pub sw_mux_ctl_pad_sd1_data1: MuxControl::Register,          // 0x33C
    pub sw_mux_ctl_pad_sd1_data0: MuxControl::Register,          // 0x340
    pub sw_mux_ctl_pad_sd1_data3: MuxControl::Register,          // 0x344
    pub sw_mux_ctl_pad_sd1_cmd: MuxControl::Register,            // 0x348
    pub sw_mux_ctl_pad_sd1_data2: MuxControl::Register,          // 0x34C
    pub sw_mux_ctl_pad_sd1_clk: MuxControl::Register,            // 0x350
    pub sw_mux_ctl_pad_sd2_clk: MuxControl::Register,            // 0x354
    pub sw_mux_ctl_pad_sd2_cmd: MuxControl::Register,            // 0x358
    pub sw_mux_ctl_pad_sd2_data3: MuxControl::Register,          // 0x35C
    pub sw_pad_ctl_pad_sd2_data1: MuxControl::Register,          // 0x360
    pub sw_pad_ctl_pad_sd2_data2: MuxControl::Register,          // 0x364
    pub sw_pad_ctl_pad_sd2_data0: MuxControl::Register,          // 0x368
    pub sw_pad_ctl_pad_rgmii_txc: MuxControl::Register,          // 0x36C
    pub sw_pad_ctl_pad_rgmii_td0: MuxControl::Register,          // 0x370
    pub sw_pad_ctl_pad_rgmii_td1: MuxControl::Register,          // 0x374
    pub sw_pad_ctl_pad_rgmii_td2: MuxControl::Register,          // 0x378
    pub sw_pad_ctl_pad_rgmii_td3: MuxControl::Register,          // 0x37C
    pub sw_pad_ctl_pad_rgmii_rx_ctl: MuxControl::Register,       // 0x380
    pub sw_pad_ctl_pad_rgmii_rd0: MuxControl::Register,          // 0x384
    pub sw_pad_ctl_pad_rgmii_tx_ctl: MuxControl::Register,       // 0x388
    pub sw_pad_ctl_pad_rgmii_rd1: MuxControl::Register,          // 0x38C
    pub sw_pad_ctl_pad_rgmii_rd2: MuxControl::Register,          // 0x390
    pub sw_pad_ctl_pad_rgmii_rd3: MuxControl::Register,          // 0x394
    pub sw_pad_ctl_pad_rgmii_rxc: MuxControl::Register,          // 0x398
    pub sw_pad_ctl_pad_eim_addr25: MuxControl::Register,         // 0x39C
    pub sw_pad_ctl_pad_eim_eb2: MuxControl::Register,            // 0x3A0
    pub sw_pad_ctl_pad_eim_data16: PadControl::Register,         // 0x3A4
    pub sw_pad_ctl_pad_eim_data17: PadControl::Register,         // 0x3A8
    pub sw_pad_ctl_pad_eim_data18: PadControl::Register,         // 0x3AC
    pub sw_pad_ctl_pad_eim_data19: PadControl::Register,         // 0x3B0
    pub sw_pad_ctl_pad_eim_data20: MuxControl::Register,         // 0x3B4
    pub sw_pad_ctl_pad_eim_data21: MuxControl::Register,         // 0x3B8
    pub sw_pad_ctl_pad_eim_data22: MuxControl::Register,         // 0x3BC
    pub sw_pad_ctl_pad_eim_data23: MuxControl::Register,         // 0x3C0
    pub sw_pad_ctl_pad_eim_eb3: MuxControl::Register,            // 0x3C4
    pub sw_pad_ctl_pad_eim_data24: MuxControl::Register,         // 0x3C8
    pub sw_pad_ctl_pad_eim_data25: MuxControl::Register,         // 0x3CC
    pub sw_pad_ctl_pad_eim_data26: MuxControl::Register,         // 0x3D0
    pub sw_pad_ctl_pad_eim_data27: MuxControl::Register,         // 0x3D4
    pub sw_pad_ctl_pad_eim_data28: MuxControl::Register,         // 0x3D8
    pub sw_pad_ctl_pad_eim_data29: MuxControl::Register,         // 0x3DC
    pub sw_pad_ctl_pad_eim_data30: MuxControl::Register,         // 0x3E0
    pub sw_pad_ctl_pad_eim_data31: MuxControl::Register,         // 0x3E4
    pub sw_pad_ctl_pad_eim_addr24: MuxControl::Register,         // 0x3E8
    pub sw_pad_ctl_pad_eim_addr23: MuxControl::Register,         // 0x3EC
    pub sw_pad_ctl_pad_eim_addr22: MuxControl::Register,         // 0x3F0
    pub sw_pad_ctl_pad_eim_addr21: MuxControl::Register,         // 0x3F4
    pub sw_pad_ctl_pad_eim_addr20: MuxControl::Register,         // 0x3F8
    pub sw_pad_ctl_pad_eim_addr19: MuxControl::Register,         // 0x3FC
    pub sw_pad_ctl_pad_eim_addr18: MuxControl::Register,         // 0x400
    pub sw_pad_ctl_pad_eim_addr17: MuxControl::Register,         // 0x404
    pub sw_pad_ctl_pad_eim_addr16: MuxControl::Register,         // 0x408
    pub sw_pad_ctl_pad_eim_cs0: MuxControl::Register,            // 0x40C
    pub sw_pad_ctl_pad_eim_cs1: MuxControl::Register,            // 0x410
    pub sw_pad_ctl_pad_eim_oe: MuxControl::Register,             // 0x414
    pub sw_pad_ctl_pad_eim_rw: MuxControl::Register,             // 0x418
    pub sw_pad_ctl_pad_eim_lba: MuxControl::Register,            // 0x41C
    pub sw_pad_ctl_pad_eim_eb0: MuxControl::Register,            // 0x420
    pub sw_pad_ctl_pad_eim_eb1: MuxControl::Register,            // 0x424
    pub sw_pad_ctl_pad_eim_ad00: MuxControl::Register,           // 0x428
    pub sw_pad_ctl_pad_eim_ad01: MuxControl::Register,           // 0x42C
    pub sw_pad_ctl_pad_eim_ad02: MuxControl::Register,           // 0x430
    pub sw_pad_ctl_pad_eim_ad03: MuxControl::Register,           // 0x434
    pub sw_pad_ctl_pad_eim_ad04: MuxControl::Register,           // 0x438
    pub sw_pad_ctl_pad_eim_ad05: MuxControl::Register,           // 0x43C
    pub sw_pad_ctl_pad_eim_ad06: MuxControl::Register,           // 0x440
    pub sw_pad_ctl_pad_eim_ad07: MuxControl::Register,           // 0x444
    pub sw_pad_ctl_pad_eim_ad08: MuxControl::Register,           // 0x448
    pub sw_pad_ctl_pad_eim_ad09: MuxControl::Register,           // 0x44C
    pub sw_pad_ctl_pad_eim_ad10: MuxControl::Register,           // 0x450
    pub sw_pad_ctl_pad_eim_ad11: MuxControl::Register,           // 0x454
    pub sw_pad_ctl_pad_eim_ad12: MuxControl::Register,           // 0x458
    pub sw_pad_ctl_pad_eim_ad13: MuxControl::Register,           // 0x45C
    pub sw_pad_ctl_pad_eim_ad14: MuxControl::Register,           // 0x460
    pub sw_pad_ctl_pad_eim_ad15: MuxControl::Register,           // 0x464
    pub sw_pad_ctl_pad_eim_wait: MuxControl::Register,           // 0x468
    pub sw_pad_ctl_pad_eim_bclk: MuxControl::Register,           // 0x46C
    pub sw_pad_ctl_pad_di0_disp_clk: MuxControl::Register,       // 0x470
    pub sw_pad_ctl_pad_di0_pin15: MuxControl::Register,          // 0x474
    pub sw_pad_ctl_pad_di0_pin02: MuxControl::Register,          // 0x478
    pub sw_pad_ctl_pad_di0_pin03: MuxControl::Register,          // 0x47C
    pub sw_pad_ctl_pad_di0_pin04: MuxControl::Register,          // 0x480
    pub sw_pad_ctl_pad_disp0_data00: MuxControl::Register,       // 0x484
    pub sw_pad_ctl_pad_disp0_data01: MuxControl::Register,       // 0x488
    pub sw_pad_ctl_pad_disp0_data02: MuxControl::Register,       // 0x48C
    pub sw_pad_ctl_pad_disp0_data03: MuxControl::Register,       // 0x490
    pub sw_pad_ctl_pad_disp0_data04: MuxControl::Register,       // 0x494
    pub sw_pad_ctl_pad_disp0_data05: MuxControl::Register,       // 0x498
    pub sw_pad_ctl_pad_disp0_data06: MuxControl::Register,       // 0x49C
    pub sw_pad_ctl_pad_disp0_data07: MuxControl::Register,       // 0x4A0
    pub sw_pad_ctl_pad_disp0_data08: MuxControl::Register,       // 0x4A4
    pub sw_pad_ctl_pad_disp0_data09: MuxControl::Register,       // 0x4A8
    pub sw_pad_ctl_pad_disp0_data10: MuxControl::Register,       // 0x4AC
    pub sw_pad_ctl_pad_disp0_data11: MuxControl::Register,       // 0x4B0
    pub sw_pad_ctl_pad_disp0_data12: MuxControl::Register,       // 0x4B4
    pub sw_pad_ctl_pad_disp0_data13: MuxControl::Register,       // 0x4B8
    pub sw_pad_ctl_pad_disp0_data14: MuxControl::Register,       // 0x4BC
    pub sw_pad_ctl_pad_disp0_data15: MuxControl::Register,       // 0x4C0
    pub sw_pad_ctl_pad_disp0_data16: MuxControl::Register,       // 0x4C4
    pub sw_pad_ctl_pad_disp0_data17: MuxControl::Register,       // 0x4C8
    pub sw_pad_ctl_pad_disp0_data18: MuxControl::Register,       // 0x4CC
    pub sw_pad_ctl_pad_disp0_data19: MuxControl::Register,       // 0x4D0
    pub sw_pad_ctl_pad_disp0_data20: MuxControl::Register,       // 0x4D4
    pub sw_pad_ctl_pad_disp0_data21: MuxControl::Register,       // 0x4D8
    pub sw_pad_ctl_pad_disp0_data22: MuxControl::Register,       // 0x4DC
    pub sw_pad_ctl_pad_disp0_data23: MuxControl::Register,       // 0x4E0
    pub sw_pad_ctl_pad_enet_mdio: MuxControl::Register,          // 0x4E4
    pub sw_pad_ctl_pad_enet_ref_clk: MuxControl::Register,       // 0x4E8
    pub sw_pad_ctl_pad_enet_rx_er: MuxControl::Register,         // 0x4EC
    pub sw_pad_ctl_pad_enet_crs_dv: MuxControl::Register,        // 0x4F0
    pub sw_pad_ctl_pad_enet_rx_data1: MuxControl::Register,      // 0x4F4
    pub sw_pad_ctl_pad_enet_rx_data0: MuxControl::Register,      // 0x4F8
    pub sw_pad_ctl_pad_enet_tx_en: MuxControl::Register,         // 0x4FC
    pub sw_pad_ctl_pad_enet_tx_data1: MuxControl::Register,      // 0x500
    pub sw_pad_ctl_pad_enet_tx_data0: MuxControl::Register,      // 0x504
    pub sw_pad_ctl_pad_enet_mdc: MuxControl::Register,           // 0x508
    pub sw_pad_ctl_pad_dram_sdqs5_p: MuxControl::Register,       // 0x50C
    pub sw_pad_ctl_pad_dram_dqm5: MuxControl::Register,          // 0x510
    pub sw_pad_ctl_pad_dram_dqm4: MuxControl::Register,          // 0x514
    pub sw_pad_ctl_pad_dram_sdqs4_p: MuxControl::Register,       // 0x518
    pub sw_pad_ctl_pad_dram_sdqs3_p: MuxControl::Register,       // 0x51C
    pub sw_pad_ctl_pad_dram_dqm3: MuxControl::Register,          // 0x520
    pub sw_pad_ctl_pad_dram_sdqs2_p: MuxControl::Register,       // 0x524
    pub sw_pad_ctl_pad_dram_dqm2: MuxControl::Register,          // 0x528
    pub sw_pad_ctl_pad_dram_addr00: MuxControl::Register,        // 0x52C
    pub sw_pad_ctl_pad_dram_addr01: MuxControl::Register,        // 0x530
    pub sw_pad_ctl_pad_dram_addr02: MuxControl::Register,        // 0x534
    pub sw_pad_ctl_pad_dram_addr03: MuxControl::Register,        // 0x538
    pub sw_pad_ctl_pad_dram_addr04: MuxControl::Register,        // 0x53C
    pub sw_pad_ctl_pad_dram_addr05: MuxControl::Register,        // 0x540
    pub sw_pad_ctl_pad_dram_addr06: MuxControl::Register,        // 0x544
    pub sw_pad_ctl_pad_dram_addr07: MuxControl::Register,        // 0x548
    pub sw_pad_ctl_pad_dram_addr08: MuxControl::Register,        // 0x54C
    pub sw_pad_ctl_pad_dram_addr09: MuxControl::Register,        // 0x550
    pub sw_pad_ctl_pad_dram_addr10: MuxControl::Register,        // 0x554
    pub sw_pad_ctl_pad_dram_addr11: MuxControl::Register,        // 0x558
    pub sw_pad_ctl_pad_dram_addr12: MuxControl::Register,        // 0x55C
    pub sw_pad_ctl_pad_dram_addr13: MuxControl::Register,        // 0x560
    pub sw_pad_ctl_pad_dram_addr14: MuxControl::Register,        // 0x564
    pub sw_pad_ctl_pad_dram_addr15: MuxControl::Register,        // 0x568
    pub sw_pad_ctl_pad_dram_cas: MuxControl::Register,           // 0x56C
    pub sw_pad_ctl_pad_dram_cs0: MuxControl::Register,           // 0x570
    pub sw_pad_ctl_pad_dram_cs1: MuxControl::Register,           // 0x574
    pub sw_pad_ctl_pad_dram_ras: MuxControl::Register,           // 0x578
    pub sw_pad_ctl_pad_dram_reset: MuxControl::Register,         // 0x57C
    pub sw_pad_ctl_pad_dram_sdba0: MuxControl::Register,         // 0x580
    pub sw_pad_ctl_pad_dram_sdba1: MuxControl::Register,         // 0x584
    pub sw_pad_ctl_pad_dram_sdclk0_p: MuxControl::Register,      // 0x588
    pub sw_pad_ctl_pad_dram_sdba2: MuxControl::Register,         // 0x58C
    pub sw_pad_ctl_pad_dram_sdcke0: MuxControl::Register,        // 0x590
    pub sw_pad_ctl_pad_dram_sdclk1_p: MuxControl::Register,      // 0x594
    pub sw_pad_ctl_pad_dram_sdcke1: MuxControl::Register,        // 0x598
    pub sw_pad_ctl_pad_dram_odt0: MuxControl::Register,          // 0x59C
    pub sw_pad_ctl_pad_dram_odt1: MuxControl::Register,          // 0x5A0
    pub sw_pad_ctl_pad_dram_sdwe: MuxControl::Register,          // 0x5A4
    pub sw_pad_ctl_pad_dram_sdqs0_p: MuxControl::Register,       // 0x5A8
    pub sw_pad_ctl_pad_dram_dqm0: MuxControl::Register,          // 0x5AC
    pub sw_pad_ctl_pad_dram_sdqs1_p: MuxControl::Register,       // 0x5B0
    pub sw_pad_ctl_pad_dram_dqm1: MuxControl::Register,          // 0x5B4
    pub sw_pad_ctl_pad_dram_sdqs6_p: MuxControl::Register,       // 0x5B8
    pub sw_pad_ctl_pad_dram_dqm6: MuxControl::Register,          // 0x5BC
    pub sw_pad_ctl_pad_dram_sdqs7_p: MuxControl::Register,       // 0x5C0
    pub sw_pad_ctl_pad_dram_dqm7: MuxControl::Register,          // 0x5C4
    pub sw_pad_ctl_pad_key_col0: MuxControl::Register,           // 0x5C8
    pub sw_pad_ctl_pad_key_row0: MuxControl::Register,           // 0x5CC
    pub sw_pad_ctl_pad_key_col1: MuxControl::Register,           // 0x5D0
    pub sw_pad_ctl_pad_key_row1: MuxControl::Register,           // 0x5D4
    pub sw_pad_ctl_pad_key_col2: MuxControl::Register,           // 0x5D8
    pub sw_pad_ctl_pad_key_row2: MuxControl::Register,           // 0x5DC
    pub sw_pad_ctl_pad_key_col3: MuxControl::Register,           // 0x5E0
    pub sw_pad_ctl_pad_key_row3: MuxControl::Register,           // 0x5E4
    pub sw_pad_ctl_pad_key_col4: MuxControl::Register,           // 0x5E8
    pub sw_pad_ctl_pad_key_row4: MuxControl::Register,           // 0x5EC
    pub sw_pad_ctl_pad_mux00: MuxControl::Register,              // 0x5F0
    pub sw_pad_ctl_pad_mux01: MuxControl::Register,              // 0x5F4
    pub sw_pad_ctl_pad_mux09: MuxControl::Register,              // 0x5F8
    pub sw_pad_ctl_pad_mux03: MuxControl::Register,              // 0x5FC
    pub sw_pad_ctl_pad_mux06: MuxControl::Register,              // 0x600
    pub sw_pad_ctl_pad_mux02: MuxControl::Register,              // 0x604
    pub sw_pad_ctl_pad_mux04: MuxControl::Register,              // 0x608
    pub sw_pad_ctl_pad_mux05: MuxControl::Register,              // 0x60C
    pub sw_pad_ctl_pad_mux07: MuxControl::Register,              // 0x610
    pub sw_pad_ctl_pad_mux08: MuxControl::Register,              // 0x614
    pub sw_pad_ctl_pad_mux16: MuxControl::Register,              // 0x618
    pub sw_pad_ctl_pad_mux17: MuxControl::Register,              // 0x61C
    pub sw_pad_ctl_pad_mux18: MuxControl::Register,              // 0x620
    pub sw_pad_ctl_pad_mux19: MuxControl::Register,              // 0x624
    pub sw_pad_ctl_pad_csi0_pixclk: MuxControl::Register,        // 0x628
    pub sw_pad_ctl_pad_csi0_hsync: MuxControl::Register,         // 0x62C
    pub sw_pad_ctl_pad_csi0_data_en: MuxControl::Register,       // 0x630
    pub sw_pad_ctl_pad_csi0_vsync: MuxControl::Register,         // 0x634
    pub sw_pad_ctl_pad_csi0_data04: MuxControl::Register,        // 0x638
    pub sw_pad_ctl_pad_csi0_data05: MuxControl::Register,        // 0x63C
    pub sw_pad_ctl_pad_csi0_data06: MuxControl::Register,        // 0x640
    pub sw_pad_ctl_pad_csi0_data07: MuxControl::Register,        // 0x644
    pub sw_pad_ctl_pad_csi0_data08: MuxControl::Register,        // 0x648
    pub sw_pad_ctl_pad_csi0_data09: MuxControl::Register,        // 0x64C
    pub sw_pad_ctl_pad_csi0_data10: MuxControl::Register,        // 0x650
    pub sw_pad_ctl_pad_csi0_data11: MuxControl::Register,        // 0x654
    pub sw_pad_ctl_pad_csi0_data12: MuxControl::Register,        // 0x658
    pub sw_pad_ctl_pad_csi0_data13: MuxControl::Register,        // 0x65C
    pub sw_pad_ctl_pad_csi0_data14: MuxControl::Register,        // 0x660
    pub sw_pad_ctl_pad_csi0_data15: MuxControl::Register,        // 0x664
    pub sw_pad_ctl_pad_csi0_data16: MuxControl::Register,        // 0x668
    pub sw_pad_ctl_pad_csi0_data17: MuxControl::Register,        // 0x66C
    pub sw_pad_ctl_pad_csi0_data18: MuxControl::Register,        // 0x670
    pub sw_pad_ctl_pad_csi0_data19: MuxControl::Register,        // 0x674
    pub sw_pad_ctl_pad_jtag_tms: MuxControl::Register,           // 0x678
    pub sw_pad_ctl_pad_jtag_mod: MuxControl::Register,           // 0x67C
    pub sw_pad_ctl_pad_jtag_trstb: MuxControl::Register,         // 0x680
    pub sw_pad_ctl_pad_jtag_tdi: MuxControl::Register,           // 0x684
    pub sw_pad_ctl_pad_jtag_tck: MuxControl::Register,           // 0x688
    pub sw_pad_ctl_pad_jtag_tdo: MuxControl::Register,           // 0x68C
    pub sw_pad_ctl_pad_sd3_data7: MuxControl::Register,          // 0x690
    pub sw_pad_ctl_pad_sd3_data6: MuxControl::Register,          // 0x694
    pub sw_pad_ctl_pad_sd3_data5: MuxControl::Register,          // 0x698
    pub sw_pad_ctl_pad_sd3_data4: MuxControl::Register,          // 0x69C
    pub sw_pad_ctl_pad_sd3_cmd: MuxControl::Register,            // 0x6A0
    pub sw_pad_ctl_pad_sd3_clk: MuxControl::Register,            // 0x6A4
    pub sw_pad_ctl_pad_sd3_data0: MuxControl::Register,          // 0x6A8
    pub sw_pad_ctl_pad_sd3_data1: MuxControl::Register,          // 0x6AC
    pub sw_pad_ctl_pad_sd3_data2: MuxControl::Register,          // 0x6B0
    pub sw_pad_ctl_pad_sd3_data3: MuxControl::Register,          // 0x6B4
    pub sw_pad_ctl_pad_sd3_reset: MuxControl::Register,          // 0x6B8
    pub sw_pad_ctl_pad_nand_cle: MuxControl::Register,           // 0x6BC
    pub sw_pad_ctl_pad_nand_ale: MuxControl::Register,           // 0x6C0
    pub sw_pad_ctl_pad_nand_wp_b: MuxControl::Register,          // 0x6C4
    pub sw_pad_ctl_pad_nand_ready: MuxControl::Register,         // 0x6C8
    pub sw_pad_ctl_pad_nand_cs0_b: MuxControl::Register,         // 0x6CC
    pub sw_pad_ctl_pad_nand_cs1_b: MuxControl::Register,         // 0x6D0
    pub sw_pad_ctl_pad_nand_cs2_b: MuxControl::Register,         // 0x6D4
    pub sw_pad_ctl_pad_nand_cs3_b: MuxControl::Register,         // 0x6D8
    pub sw_pad_ctl_pad_sd4_cmd: MuxControl::Register,            // 0x6DC
    pub sw_pad_ctl_pad_sd4_clk: MuxControl::Register,            // 0x6E0
    pub sw_pad_ctl_pad_nand_data00: MuxControl::Register,        // 0x6E4
    pub sw_pad_ctl_pad_nand_data01: MuxControl::Register,        // 0x6E8
    pub sw_pad_ctl_pad_nand_data02: MuxControl::Register,        // 0x6EC
    pub sw_pad_ctl_pad_nand_data03: MuxControl::Register,        // 0x6F0
    pub sw_pad_ctl_pad_nand_data04: MuxControl::Register,        // 0x6F4
    pub sw_pad_ctl_pad_nand_data05: MuxControl::Register,        // 0x6F8
    pub sw_pad_ctl_pad_nand_data06: MuxControl::Register,        // 0x6FC
    pub sw_pad_ctl_pad_nand_data07: MuxControl::Register,        // 0x700
    pub sw_pad_ctl_pad_sd4_data0: MuxControl::Register,          // 0x704
    pub sw_pad_ctl_pad_sd4_data1: MuxControl::Register,          // 0x708
    pub sw_pad_ctl_pad_sd4_data2: MuxControl::Register,          // 0x70C
    pub sw_pad_ctl_pad_sd4_data3: MuxControl::Register,          // 0x710
    pub sw_pad_ctl_pad_sd4_data4: MuxControl::Register,          // 0x714
    pub sw_pad_ctl_pad_sd4_data5: MuxControl::Register,          // 0x718
    pub sw_pad_ctl_pad_sd4_data6: MuxControl::Register,          // 0x71C
    pub sw_pad_ctl_pad_sd4_data7: MuxControl::Register,          // 0x720
    pub sw_pad_ctl_pad_sd1_data1: MuxControl::Register,          // 0x724
    pub sw_pad_ctl_pad_sd1_data0: MuxControl::Register,          // 0x728
    pub sw_pad_ctl_pad_sd1_data3: MuxControl::Register,          // 0x72C
    pub sw_pad_ctl_pad_sd1_cmd: MuxControl::Register,            // 0x730
    pub sw_pad_ctl_pad_sd1_data2: MuxControl::Register,          // 0x734
    pub sw_pad_ctl_pad_sd1_clk: MuxControl::Register,            // 0x738
    pub sw_pad_ctl_pad_sd2_clk: MuxControl::Register,            // 0x73C
    pub sw_pad_ctl_pad_sd2_cmd: MuxControl::Register,            // 0x740
    pub sw_pad_ctl_pad_sd2_data3: MuxControl::Register,          // 0x744
    pub sw_pad_ctl_grp_b7ds: MuxControl::Register,               // 0x748
    pub sw_pad_ctl_grp_addds: MuxControl::Register,              // 0x74C
    pub sw_pad_ctl_grp_ddrmode_ctl: MuxControl::Register,        // 0x750
    pub sw_pad_ctl_grp_term_ctl0: MuxControl::Register,          // 0x754
    pub sw_pad_ctl_grp_ddrpke: MuxControl::Register,             // 0x758
    pub sw_pad_ctl_grp_term_ctl1: MuxControl::Register,          // 0x75C
    pub sw_pad_ctl_grp_term_ctl2: MuxControl::Register,          // 0x760
    pub sw_pad_ctl_grp_term_ctl3: MuxControl::Register,          // 0x764
    pub sw_pad_ctl_grp_ddrpk: MuxControl::Register,              // 0x768
    pub sw_pad_ctl_grp_term_ctl4: MuxControl::Register,          // 0x76C
    pub sw_pad_ctl_grp_ddrhys: MuxControl::Register,             // 0x770
    pub sw_pad_ctl_grp_ddrmode: MuxControl::Register,            // 0x774
    pub sw_pad_ctl_grp_term_ctl5: MuxControl::Register,          // 0x778
    pub sw_pad_ctl_grp_term_ctl6: MuxControl::Register,          // 0x77C
    pub sw_pad_ctl_grp_term_ctl7: MuxControl::Register,          // 0x780
    pub sw_pad_ctl_grp_b0ds: MuxControl::Register,               // 0x784
    pub sw_pad_ctl_grp_b1ds: MuxControl::Register,               // 0x788
    pub sw_pad_ctl_grp_ctlds: MuxControl::Register,              // 0x78C
    pub sw_pad_ctl_grp_ddr_type_rgmii: MuxControl::Register,     // 0x790
    pub sw_pad_ctl_grp_b2ds: MuxControl::Register,               // 0x794
    pub sw_pad_ctl_grp_ddr_type: MuxControl::Register,           // 0x798
    pub sw_pad_ctl_grp_b3ds: MuxControl::Register,               // 0x79C
    pub sw_pad_ctl_grp_b4ds: MuxControl::Register,               // 0x7A0
    pub sw_pad_ctl_grp_b5ds: MuxControl::Register,               // 0x7A4
    pub sw_pad_ctl_grp_b6ds: MuxControl::Register,               // 0x7A8
    pub sw_pad_ctl_grp_rgmii_term: MuxControl::Register,         // 0x7AC
    pub asrc_asrck_clock_6_select_input: MuxControl::Register,   // 0x7B0
    pub aud4_input_da_amx_select_input: MuxControl::Register,    // 0x7B4
    pub aud4_input_db_amx_select_input: MuxControl::Register,    // 0x7B8
    pub aud4_input_rxclk_amx_select_input: MuxControl::Register, // 0x7BC
    pub aud4_input_rxfs_amx_select_input: MuxControl::Register,  // 0x7C0
    pub aud4_input_txclk_amx_select_input: MuxControl::Register, // 0x7C4
    pub aud4_input_txfs_amx_select_input: MuxControl::Register,  // 0x7C8
    pub aud5_input_da_amx_select_input: MuxControl::Register,    // 0x7CC
    pub aud5_input_db_amx_select_input: MuxControl::Register,    // 0x7D0
    pub aud5_input_rxclk_amx_select_input: MuxControl::Register, // 0x7D4
    pub aud5_input_rxfs_amx_select_input: MuxControl::Register,  // 0x7D8
    pub aud5_input_txclk_amx_select_input: MuxControl::Register, // 0x7DC
    pub aud5_input_txfs_amx_select_input: MuxControl::Register,  // 0x7E0
    pub flexcan1_rx_select_input: MuxControl::Register,          // 0x7E4
    pub flexcan2_rx_select_input: MuxControl::Register,          // 0x7E8
    pub res2: MuxControl::Register,                              // 0x7EC
    pub ccm_pmic_ready_select_input: MuxControl::Register,       // 0x7F0
    pub ecspi1_cspi_clk_in_select_input: SelectInput::Register,  // 0x7F4
    pub ecspi1_miso_select_input: SelectInput::Register,         // 0x7F8
    pub ecspi1_mosi_select_input: SelectInput::Register,         // 0x7FC
    pub ecspi1_ss0_select_input: SelectInput::Register,          // 0x800
    pub ecspi1_ss1_select_input: SelectInput::Register,          // 0x804
    pub ecspi1_ss2_select_input: MuxControl::Register,           // 0x808
    pub ecspi1_ss3_select_input: MuxControl::Register,           // 0x80C
    pub ecspi2_cspi_clk_in_select_input: MuxControl::Register,   // 0x810
    pub ecspi2_miso_select_input: MuxControl::Register,          // 0x814
    pub ecspi2_mosi_select_input: MuxControl::Register,          // 0x818
    pub ecspi2_ss0_select_input: MuxControl::Register,           // 0x81C
    pub ecspi2_ss1_select_input: MuxControl::Register,           // 0x820
    pub ecspi4_ss0_select_input: MuxControl::Register,           // 0x824
    pub ecspi5_cspi_clk_in_select_input: MuxControl::Register,   // 0x828
    pub ecspi5_miso_select_input: MuxControl::Register,          // 0x82C
    pub ecspi5_mosi_select_input: MuxControl::Register,          // 0x830
    pub ecspi5_ss0_select_input: MuxControl::Register,           // 0x834
    pub ecspi5_ss1_select_input: MuxControl::Register,           // 0x838
    pub enet_ref_clk_select_input: MuxControl::Register,         // 0x83C
    pub enet_mac0_mdio_select_input: MuxControl::Register,       // 0x840
    pub enet_mac0_rx_clk_select_input: MuxControl::Register,     // 0x844
    pub enet_mac0_rx_data0_select_input: MuxControl::Register,   // 0x848
    pub enet_mac0_rx_data1_select_input: MuxControl::Register,   // 0x84C
    pub enet_mac0_rx_data2_select_input: MuxControl::Register,   // 0x850
    pub enet_mac0_rx_data3_select_input: MuxControl::Register,   // 0x854
    pub enet_mac0_rx_en_select_input: MuxControl::Register,      // 0x858
    pub esai_rx_fs_select_input: MuxControl::Register,           // 0x85C
    pub esai_tx_fs_select_input: MuxControl::Register,           // 0x860
    pub esai_rx_hf_clk_select_input: MuxControl::Register,       // 0x864
    pub esai_tx_hf_clk_select_input: MuxControl::Register,       // 0x868
    pub esai_rx_clk_select_input: MuxControl::Register,          // 0x86C
    pub esai_tx_clk_select_input: MuxControl::Register,          // 0x870
    pub esai_sdo0_select_input: MuxControl::Register,            // 0x874
    pub esai_sdo1_select_input: MuxControl::Register,            // 0x878
    pub esai_sdo2_sdi3_select_input: MuxControl::Register,       // 0x87C
    pub esai_sdo3_sdi2_select_input: MuxControl::Register,       // 0x880
    pub esai_sdo4_sdi1_select_input: MuxControl::Register,       // 0x884
    pub esai_sdo5_sdi0_select_input: MuxControl::Register,       // 0x888
    pub hdmi_icecin_select_input: MuxControl::Register,          // 0x88C
    pub hdmi_ii2c_clkin_select_input: MuxControl::Register,      // 0x890
    pub hdmi_ii2c_datain_select_input: MuxControl::Register,     // 0x894
    pub i2c1_scl_in_select_input: MuxControl::Register,          // 0x898
    pub i2c1_sda_in_select_input: MuxControl::Register,          // 0x89C
    pub i2c2_scl_in_select_input: MuxControl::Register,          // 0x8A0
    pub i2c2_sda_in_select_input: MuxControl::Register,          // 0x8A4
    pub i2c3_scl_in_select_input: MuxControl::Register,          // 0x8A8
    pub i2c3_sda_in_select_input: MuxControl::Register,          // 0x8AC
    pub ipu2_sens1_data10_select_input: MuxControl::Register,    // 0x8B0
    pub ipu2_sens1_data11_select_input: MuxControl::Register,    // 0x8B4
    pub ipu2_sens1_data12_select_input: MuxControl::Register,    // 0x8B8
    pub ipu2_sens1_data13_select_input: MuxControl::Register,    // 0x8BC
    pub ipu2_sens1_data14_select_input: MuxControl::Register,    // 0x8C0
    pub ipu2_sens1_data15_select_input: MuxControl::Register,    // 0x8C4
    pub ipu2_sens1_data16_select_input: MuxControl::Register,    // 0x8C8
    pub ipu2_sens1_data17_select_input: MuxControl::Register,    // 0x8CC
    pub ipu2_sens1_data18_select_input: MuxControl::Register,    // 0x8D0
    pub ipu2_sens1_data19_select_input: MuxControl::Register,    // 0x8D4
    pub ipu2_sens1_data_en_select_input: MuxControl::Register,   // 0x8D8
    pub ipu2_sens1_hsync_select_input: MuxControl::Register,     // 0x8DC
    pub ipu2_sens1_pix_clk_select_input: MuxControl::Register,   // 0x8E0
    pub ipu2_sens1_vsync_select_input: MuxControl::Register,     // 0x8E4
    pub key_col5_select_input: MuxControl::Register,             // 0x8E8
    pub key_col6_select_input: MuxControl::Register,             // 0x8EC
    pub key_col7_select_input: MuxControl::Register,             // 0x8F0
    pub key_row5_select_input: MuxControl::Register,             // 0x8F4
    pub key_row6_select_input: MuxControl::Register,             // 0x8F8
    pub key_row7_select_input: MuxControl::Register,             // 0x8FC
    pub mlb_mlb_clk_in_select_input: MuxControl::Register,       // 0x900
    pub mlb_mlb_data_in_select_input: MuxControl::Register,      // 0x904
    pub mlb_mlb_sig_in_select_input: MuxControl::Register,       // 0x908
    pub sdma_events14_select_input: MuxControl::Register,        // 0x90C
    pub sdma_events15_select_input: MuxControl::Register,        // 0x910
    pub spdif_spdif_in1_select_input: MuxControl::Register,      // 0x914
    pub spdif_tx_clk2_select_input: MuxControl::Register,        // 0x918
    pub uart1_uart_rts_b_select_input: MuxControl::Register,     // 0x91C
    pub uart1_uart_rx_data_select_input: MuxControl::Register,   // 0x920
    pub uart2_uart_rts_b_select_input: MuxControl::Register,     // 0x924
    pub uart2_uart_rx_data_select_input: MuxControl::Register,   // 0x928
    pub uart3_uart_rts_b_select_input: MuxControl::Register,     // 0x92C
    pub uart3_uart_rx_data_select_input: MuxControl::Register,   // 0x930
    pub uart4_uart_rts_b_select_input: MuxControl::Register,     // 0x934
    pub uart4_uart_rx_data_select_input: MuxControl::Register,   // 0x938
    pub uart5_uart_rts_b_select_input: MuxControl::Register,     // 0x93C
    pub uart5_uart_rx_data_select_input: MuxControl::Register,   // 0x940
    pub usb_otg_oc_select_input: MuxControl::Register,           // 0x944
    pub usb_h1_oc_select_input: MuxControl::Register,            // 0x948
    pub usdhc1_wp_on_select_input: MuxControl::Register,         // 0x94C
}

pub struct IOMUXC {
    vaddr: u32,
}

impl IOMUXC {
    pub const PADDR: u32 = 0x020E_0000;
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

impl Deref for IOMUXC {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for IOMUXC {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
