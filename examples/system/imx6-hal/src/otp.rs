//! One time programmable memory controler

use imx6_devices::ocotp::*;
use net_types::EthernetAddress;

pub struct Otp {
    ocotp: OCOTP,
}

impl Otp {
    pub fn new(ocotp: OCOTP) -> Self {
        Otp { ocotp }
    }

    pub fn read_mac_address(&self) -> EthernetAddress {
        let b0 = self
            .ocotp
            .mac1
            .get_field(MacAddress1::Octet0::Read)
            .unwrap()
            .val() as u8;
        let b1 = self
            .ocotp
            .mac1
            .get_field(MacAddress1::Octet1::Read)
            .unwrap()
            .val() as u8;
        let b2 = self
            .ocotp
            .mac0
            .get_field(MacAddress0::Octet2::Read)
            .unwrap()
            .val() as u8;
        let b3 = self
            .ocotp
            .mac0
            .get_field(MacAddress0::Octet3::Read)
            .unwrap()
            .val() as u8;
        let b4 = self
            .ocotp
            .mac0
            .get_field(MacAddress0::Octet4::Read)
            .unwrap()
            .val() as u8;
        let b5 = self
            .ocotp
            .mac0
            .get_field(MacAddress0::Octet5::Read)
            .unwrap()
            .val() as u8;
        EthernetAddress([b0, b1, b2, b3, b4, b5])
    }
}
