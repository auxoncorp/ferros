//! OCOTP

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::Unsigned;

register! {
    Control,
    u32,
    RW,
    Fields [
        Address         WIDTH(U7) OFFSET(U0),
        Busy            WIDTH(U1) OFFSET(U8),
        Error           WIDTH(U1) OFFSET(U9),
        ReloadShadows   WIDTH(U1) OFFSET(U10),
        WriteUnlock     WIDTH(U16) OFFSET(U16),
    ]
}

register! {
    MacAddress0,
    u32,
    RW,
    Fields [
        Octet5 WIDTH(U8) OFFSET(U0),
        Octet4 WIDTH(U8) OFFSET(U8),
        Octet3 WIDTH(U8) OFFSET(U16),
        Octet2 WIDTH(U8) OFFSET(U24),
    ]
}

register! {
    MacAddress1,
    u32,
    RW,
    Fields [
        Octet1 WIDTH(U8) OFFSET(U0),
        Octet0 WIDTH(U8) OFFSET(U8),
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

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x700);

#[repr(C)]
pub struct RegisterBlock {
    pub ctrl: Control::Register,        // 0x000
    pub ctrl_set: Control::Register,    // 0x004
    pub ctrl_clr: Control::Register,    // 0x008
    pub ctrl_tog: Control::Register,    // 0x00C
    pub timing: Data::Register,         // 0x010
    __reserved_0: [u32; 3],             // 0x014
    pub data: Data::Register,           // 0x020
    __reserved_1: [u32; 3],             // 0x024
    pub read_ctrl: Data::Register,      // 0x030
    __reserved_2: [u32; 3],             // 0x034
    pub read_fuse_data: Data::Register, // 0x040
    __reserved_3: [u32; 3],             // 0x044
    pub sw_sticky: Data::Register,      // 0x050
    __reserved_4: [u32; 3],             // 0x054
    pub scs: Data::Register,            // 0x060
    pub scs_set: Data::Register,        // 0x064
    pub scs_clr: Data::Register,        // 0x068
    pub scs_tog: Data::Register,        // 0x06C
    __reserved_5: [u32; 8],             // 0x070
    pub ver: Data::Register,            // 0x090
    __reserved_6: [u32; 219],           // 0x094
    pub lock: Data::Register,           // 0x400
    __reserved_7: [u32; 3],             // 0x404
    pub cfg0: Data::Register,           // 0x410
    __reserved_8: [u32; 3],             // 0x414
    pub cfg1: Data::Register,           // 0x420
    __reserved_9: [u32; 3],             // 0x424
    pub cfg2: Data::Register,           // 0x430
    __reserved_10: [u32; 3],            // 0x434
    pub cfg3: Data::Register,           // 0x440
    __reserved_11: [u32; 3],            // 0x444
    pub cfg4: Data::Register,           // 0x450
    __reserved_12: [u32; 3],            // 0x454
    pub cfg5: Data::Register,           // 0x460
    __reserved_13: [u32; 3],            // 0x464
    pub cfg6: Data::Register,           // 0x470
    __reserved_14: [u32; 3],            // 0x474
    pub mem0: Data::Register,           // 0x480
    __reserved_15: [u32; 3],            // 0x484
    pub mem1: Data::Register,           // 0x490
    __reserved_16: [u32; 3],            // 0x494
    pub mem2: Data::Register,           // 0x4A0
    __reserved_17: [u32; 3],            // 0x4A4
    pub mem3: Data::Register,           // 0x4B0
    __reserved_18: [u32; 3],            // 0x4B4
    pub mem4: Data::Register,           // 0x4C0
    __reserved_19: [u32; 3],            // 0x4C4
    pub ana0: Data::Register,           // 0x4D0
    __reserved_20: [u32; 3],            // 0x4D4
    pub ana1: Data::Register,           // 0x4E0
    __reserved_21: [u32; 3],            // 0x4E4
    pub ana2: Data::Register,           // 0x4F0
    __reserved_22: [u32; 3],            // 0x4F4
    __reserved_23: [u32; 32],           // 0x500
    pub srk0: Data::Register,           // 0x580
    __reserved_24: [u32; 3],            // 0x584
    pub srk1: Data::Register,           // 0x590
    __reserved_25: [u32; 3],            // 0x594
    pub srk2: Data::Register,           // 0x5A0
    __reserved_26: [u32; 3],            // 0x5A4
    pub srk3: Data::Register,           // 0x5B0
    __reserved_27: [u32; 3],            // 0x5B4
    pub srk4: Data::Register,           // 0x5C0
    __reserved_28: [u32; 3],            // 0x5C4
    pub srk5: Data::Register,           // 0x5D0
    __reserved_29: [u32; 3],            // 0x5D4
    pub srk6: Data::Register,           // 0x5E0
    __reserved_30: [u32; 3],            // 0x5E4
    pub srk7: Data::Register,           // 0x5F0
    __reserved_31: [u32; 3],            // 0x5F4
    pub resp0: Data::Register,          // 0x600
    __reserved_32: [u32; 3],            // 0x604
    pub hsjc_resp1: Data::Register,     // 0x610
    __reserved_33: [u32; 3],            // 0x614
    pub mac0: MacAddress0::Register,    // 0x620
    __reserved_34: [u32; 3],            // 0x624
    pub mac1: MacAddress1::Register,    // 0x630
    __reserved_35: [u32; 3],            // 0x634
    __reserved_36: [u32; 8],            // 0x640
    pub gp0: Data::Register,            // 0x660
    __reserved_37: [u32; 3],            // 0x664
    pub gp1: Data::Register,            // 0x670
    __reserved_38: [u32; 3],            // 0x674
    __reserved_39: [u32; 20],           // 0x680
    pub misc_conf: Data::Register,      // 0x6D0
    __reserved_40: [u32; 3],            // 0x6D4
    pub field_return: Data::Register,   // 0x6E0
    __reserved_41: [u32; 3],            // 0x6E4
    pub srk_revoke: Data::Register,     // 0x6F0
    __reserved_42: [u32; 3],            // 0x6F4
}

pub struct OCOTP {
    vaddr: u32,
}

impl OCOTP {
    pub const PADDR: u32 = 0x021B_C000;
    pub const SIZE: usize = crate::PageBytes::USIZE;

    /// Key needed to unlock HW_OCOTP_DATA register.
    pub const UNLOCK_KEY: u16 = 0x3E77;

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

impl Deref for OCOTP {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for OCOTP {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
