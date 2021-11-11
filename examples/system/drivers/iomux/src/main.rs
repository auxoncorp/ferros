#![no_std]
#![no_main]

use selfe_runtime as _;

use debug_logger::DebugLogger;
use ferros::cap::role;
use imx6_hal::pac::{iomuxc::*, typenum};
use iomux::{ProcParams, Request, Response};

static LOGGER: DebugLogger = DebugLogger;

#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(DebugLogger::max_log_level_from_env()))
        .unwrap();

    log::debug!("[iomux] Process started");

    let mut iomuxc = params.iomuxc;

    params
        .responder
        .reply_recv(move |req| {
            log::debug!("[iomux] Processing request {:?}", req);
            match req {
                Request::ConfigureEcSpi1 => {
                    log::trace!("[iomux] PAD_EIM_D17__ECSPI1_MISO");
                    iomuxc
                        .sw_mux_ctl_pad_eim_data17
                        .modify(MuxControl::MuxMode::ALT1);
                    iomuxc
                        .ecspi1_miso_select_input
                        .modify(SelectInput::Daisy::Field::checked::<typenum::U0>());
                    iomuxc
                        .sw_pad_ctl_pad_eim_data17
                        .modify(PadControl::Bits::Field::new(0x100B1).unwrap());

                    log::trace!("[iomux] PAD_EIM_D18__ECSPI1_MOSI");
                    iomuxc
                        .sw_mux_ctl_pad_eim_data18
                        .modify(MuxControl::MuxMode::ALT1);
                    iomuxc
                        .ecspi1_mosi_select_input
                        .modify(SelectInput::Daisy::Field::checked::<typenum::U0>());
                    iomuxc
                        .sw_pad_ctl_pad_eim_data18
                        .modify(PadControl::Bits::Field::new(0x100B1).unwrap());

                    log::trace!("[iomux] PAD_EIM_D16__ECSPI1_SCLK");
                    iomuxc
                        .sw_mux_ctl_pad_eim_data16
                        .modify(MuxControl::MuxMode::ALT1);
                    iomuxc
                        .ecspi1_cspi_clk_in_select_input
                        .modify(SelectInput::Daisy::Field::checked::<typenum::U0>());
                    iomuxc
                        .sw_pad_ctl_pad_eim_data16
                        .modify(PadControl::Bits::Field::new(0xB1).unwrap());

                    log::trace!("[iomux] PAD_EIM_D19__GPIO3_IO19");
                    iomuxc
                        .sw_mux_ctl_pad_eim_data19
                        .modify(MuxControl::MuxMode::ALT5);
                    iomuxc
                        .sw_pad_ctl_pad_eim_data19
                        .modify(PadControl::Bits::Field::new(0xB0B1).unwrap());

                    Response::EcSpi1Configured
                }
            }
        })
        .expect("Could not set up a reply_recv");

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}
