//! Legacy FEC DMA buffer descriptor
//!
//! See [IMX6DQRM](http://cache.freescale.com/files/32bit/doc/ref_manual/IMX6DQRM.pdf)
//! chapter 23.6.13.
//!
//! NOTE:
//! * To enable legacy support, write 0 to ENETn_ECR[1588EN].
//! * DBSWP must be set to 1 after reset to enable little-endian mode.

use bitflags::bitflags;
use core::{fmt, ptr};
use imx6_devices::typenum::U8;

/// Size of the DMA descriptor, in bytes
pub type DescriptorSize = U8;

pub mod rx {
    use super::*;
    use imx6_devices::typenum::Unsigned;
    use static_assertions::{assert_eq_align, assert_eq_size, const_assert_eq};

    assert_eq_size!(Descriptor, [u32; 2]);
    assert_eq_align!(Descriptor, u32);
    const_assert_eq!(DescriptorSize::USIZE, core::mem::size_of::<Descriptor>());

    bitflags! {
        /// Legacy receive buffer descriptor status
        #[repr(transparent)]
        pub struct Status: u16 {
            /// Set if the receive frame is truncated (frame length >TRUNC_FL).
            /// If the TR field is set, the frame must be discarded and the other
            /// error fields must be ignored because they may be TR incorrect.
            const TR = 1 << 0;
            /// Overrun.
            /// Written by the MAC.
            /// A receive FIFO overrun occurred during frame reception.
            /// If this field is set, the other status
            /// fields, M, LG, NO, and CR, lose their normal meaning and are OV zero.
            /// This field is valid only if the L field is set.
            const OV = 1 << 1;
            /// Receive CRC or frame error.
            /// Written by the MAC.
            /// This frame contains a PHY or CRC error and is an
            /// integral number of octets in length. This field is valid only if the L field is set.
            const CR = 1 << 2;
            /// Receive non-octet aligned frame.
            /// Written by the MAC. A frame that contained a number of
            /// bits not divisible by 8 was received, and the CRC check
            /// that occurred at the preceding byte NO boundary generated
            /// an error or a PHY error occurred.
            /// This field is valid only if the L field is set.
            /// If this field is set, the CR field is not set.
            const NO = 1 << 4;
            /// Receive frame length violation.
            /// Written by the MAC.
            /// A frame length greater than RCR[MAX_FL] was recognized.
            /// This field is valid only if the L field is set.
            /// The receive data is LG not altered in any way unless the length exceeds TRUNC_FL bytes.
            const LG = 1 << 5;
            /// Set if the DA is multicast and not BC.
            const MC = 1 << 6;
            /// Set if the DA is broadcast (FFFF_FFFF_FFFF).
            const BC = 1 << 7;
            /// Miss.
            /// Written by the MAC.
            /// This field is set by the MAC for frames accepted in promiscuous
            /// mode, but flagged as a miss by the internal address recognition. Therefore, while in
            /// promiscuous mode, you can use the this field to quickly determine whether the frame was
            /// destined to this station. This field is valid only if the L and PROM bits are set.
            /// * 0 The frame was received because of an address recognition hit.
            /// * 1 The frame was received because of promiscuous mode.
            ///
            /// The information needed for this field comes from
            /// the promiscuous_miss(ff_rx_err_stat[26]) sideband signal.
            const M = 1 << 8;
            /// Last in frame. Written by the uDMA.
            /// * 0 The buffer is not the last in a frame.
            /// * 1 The buffer is the last in a frame.
            const L = 1 << 11;
            /// Receive software ownership.
            /// This field is reserved for use by software.
            /// This read/write field is not modified by hardware, nor does its value affect hardware.
            const RO2 = 1 << 12;
            /// Wrap. Written by user.
            /// * 0 The next buffer descriptor is found in the consecutive location.
            /// * 1 The next buffer descriptor is found at the location defined in ENETn_RDSR.
            const W = 1 << 13;
            /// Receive software ownership.
            /// This field is reserved for use by software.
            /// This read/write field is not modified by hardware, nor does its value affect hardware.
            const RO1 = 1 << 14;
            /// Empty. Written by the MAC (= 0) and user (= 1).
            /// * 0 The data buffer associated with this BD is filled with received data, or data reception has
            ///     aborted due to an error condition. The status and length fields have been updated as
            /// required.
            /// * 1 The data buffer associated with this BD is empty, or reception is currently in progress.
            const E = 1 << 15;
        }
    }

    /// Legacy receive buffer descriptor
    #[derive(Debug)]
    #[repr(C, align(4))]
    pub struct Descriptor {
        /// Data length.
        /// Written by the MAC. Data length is the number of octets
        /// written by the MAC into this BD's data buffer if L is
        /// cleared (the value is equal to EMRBR), or the length of the frame
        /// including CRC if L is set. It is written by the MAC once as the BD
        /// is closed.
        length: u16,

        /// Status flags.
        status: Status,

        /// Receive data buffer pointer (physical address).
        /// The receive buffer pointer, containing the address of the associated
        /// data buffer, must always pointer be evenly divisible by 16.
        /// The buffer must reside in memory external to the MAC.
        /// The high Ethernet controller never modifies this value.
        address: u32,
    }

    impl Descriptor {
        pub fn zero(&mut self) {
            self.set_length(0);
            self.set_status(Status::empty());
            self.set_address(0);
        }

        pub fn length(&self) -> u16 {
            unsafe { ptr::read_volatile(&self.length) }
        }

        pub fn set_length(&mut self, length: u16) {
            unsafe { ptr::write_volatile(&mut self.length, length) }
        }

        pub fn status(&self) -> Status {
            unsafe { ptr::read_volatile(&self.status) }
        }

        pub fn set_status(&mut self, status: Status) {
            unsafe { ptr::write_volatile(&mut self.status, status) }
        }

        pub fn address(&self) -> u32 {
            unsafe { ptr::read_volatile(&self.address) }
        }

        pub fn set_address(&mut self, address: u32) {
            unsafe { ptr::write_volatile(&mut self.address, address) }
        }
    }

    impl fmt::Display for Descriptor {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                "RxDescriptor {{ length={}, address=0x{:X}, status={:?}",
                self.length(),
                self.address(),
                self.status()
            )
        }
    }
}

pub mod tx {
    use super::*;
    use imx6_devices::typenum::Unsigned;
    use static_assertions::{assert_eq_align, assert_eq_size, const_assert_eq};

    assert_eq_size!(Descriptor, [u32; 2]);
    assert_eq_align!(Descriptor, u32);
    const_assert_eq!(DescriptorSize::USIZE, core::mem::size_of::<Descriptor>());

    bitflags! {
        /// Legacy transmit buffer descriptor status
        #[repr(transparent)]
        pub struct Status: u16 {
            /// Append bad CRC.
            /// Note: This field is not supported by the uDMA and is ignored.
            const ABC = 1 << 9;
            /// Transmit CRC.
            /// Written by user, and valid only when L is set.
            /// * 0 End transmission immediately after the last data byte
            /// * 1 Transmit the CRC sequence after the last data byte
            /// This field is valid only when the L field is set.
            const TC = 1 << 10;
            /// Last in frame.
            /// Written by user.
            /// * 0 The buffer is not the last in the transmit frame
            /// * 1 The buffer is the last in the transmit frame
            const L = 1 << 11;
            /// Transmit software ownership.
            /// This field is reserved for use by software.
            /// This read/write field is not modified by hardware and its
            /// value does not affect hardware.
            const TO2 = 1 << 12;
            /// Wrap.
            /// Written by user.
            /// * 0 The next buffer descriptor is found in the consecutive location
            /// * 1 The next buffer descriptor is found at the location defined in ETDSR.
            const W = 1 << 13;
            /// Transmit software ownership.
            /// This field is reserved for software use.
            /// This read/write field is not modified by hardware and its value does not affect hardware.
            const TO1 = 1 << 14;
            /// Ready.
            /// Written by the MAC and you.
            /// * 0 The data buffer associated with this BD is not ready for transmission.
            ///     You are free to manipulate this BD or its associated data buffer.
            ///     The MAC clears this field after the buffer has been transmitted or after an error
            ///     condition is encountered.
            /// * 1 The data buffer, prepared for transmission by you, has not been
            ///     transmitted or currently transmits.
            ///     You may write no fields of this BD after this field is set.
            const R = 1 << 15;
        }
    }

    /// Legacy transmit buffer descriptor
    #[derive(Debug)]
    #[repr(C, align(4))]
    pub struct Descriptor {
        /// Data length, written by user.
        /// Data length is the number of octets the MAC should transmit from
        /// this BD's data buffer.
        /// It is never modified by the MAC.
        length: u16,

        /// Status flags.  
        status: Status,

        /// Transmit data buffer pointer (physical address).
        /// The buffer must reside in memory external to the MAC.
        /// This value is never modified by the Ethernet controller.
        address: u32,
    }

    impl Descriptor {
        pub fn zero(&mut self) {
            self.set_length(0);
            self.set_status(Status::empty());
            self.set_address(0);
        }

        pub fn length(&self) -> u16 {
            unsafe { ptr::read_volatile(&self.length) }
        }

        pub fn set_length(&mut self, length: u16) {
            unsafe { ptr::write_volatile(&mut self.length, length) }
        }

        pub fn status(&self) -> Status {
            unsafe { ptr::read_volatile(&self.status) }
        }

        pub fn set_status(&mut self, status: Status) {
            unsafe { ptr::write_volatile(&mut self.status, status) }
        }

        pub fn address(&self) -> u32 {
            unsafe { ptr::read_volatile(&self.address) }
        }

        pub fn set_address(&mut self, address: u32) {
            unsafe { ptr::write_volatile(&mut self.address, address) }
        }
    }

    impl fmt::Display for Descriptor {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                "TxDescriptor {{ length={}, address=0x{:X}, status={:?}",
                self.length(),
                self.address(),
                self.status()
            )
        }
    }
}
