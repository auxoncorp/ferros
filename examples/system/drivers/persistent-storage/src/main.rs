#![no_std]
#![no_main]

use selfe_runtime as _;

use crate::flash_controller::SpiNorFlashController;
use core::convert::TryInto;
use core::hash::{Hash, Hasher};
use core::str;
use debug_logger::DebugLogger;
use ferros::cap::role;
use imx6_hal::{
    gpio::GpioExt,
    pac::typenum::Unsigned,
    spi::Spi,
    spi_nor_flash::{SpiNorFlash, ERASE_SIZE_BYTES},
};
use persistent_storage::{
    ProcParams, Request, Response, StorageBufferSizeBytes, Value, MAX_VALUE_SIZE,
};
use siphasher::sip::SipHasher;
use static_assertions::const_assert_eq;
use tickv::{ErrorCode, TicKV, MAIN_KEY};

mod flash_controller;

static LOGGER: DebugLogger = DebugLogger;

const_assert_eq!(StorageBufferSizeBytes::USIZE, ERASE_SIZE_BYTES);

#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(DebugLogger::max_log_level_from_env()))
        .unwrap();

    log::debug!("[persistent-storage] Process started",);

    log::debug!(
        "[persistent-storage] storage vaddr=0x{:X} size={}",
        params.storage_buffer.vaddr(),
        params.storage_buffer.size_bytes()
    );

    log::debug!(
        "[persistent-storage] scratchpad vaddr=0x{:X} size={}",
        params.scratchpad_buffer.vaddr(),
        params.scratchpad_buffer.size_bytes()
    );

    // Local storage for a Value on the stack
    let mut value_buffer: [u8; MAX_VALUE_SIZE] = [0; MAX_VALUE_SIZE];

    // Scratchpad mem to deal with flash sub-page size writes (read-modify-write)
    let mut scratchpad_buffer = params.scratchpad_buffer;
    let scratchpad_buffer_slice = scratchpad_buffer.as_mut_slice();

    // TickV expects an array ref
    let mut storage_buffer = params.storage_buffer;
    let storage_buffer_slice = storage_buffer.as_mut_slice();
    let storage_buffer_array: &mut [u8; ERASE_SIZE_BYTES] =
        storage_buffer_slice.try_into().unwrap();

    // Configure ECSPI1 IO
    let resp = params
        .iomux_caller
        .blocking_call(&iomux::Request::ConfigureEcSpi1)
        .unwrap();
    log::debug!("[persistent-storage] Configured ECSPI1 IO resp={:?}", resp);

    let gpio = params.gpio3.split();
    let spi_nor_cs_pin = gpio.bank3.p3_19.into_push_pull_output();
    let spi = Spi::new(params.spi);

    let spi_nor_flash = SpiNorFlash::init(spi, spi_nor_cs_pin).unwrap();
    let flash = SpiNorFlashController::new(spi_nor_flash, scratchpad_buffer_slice).unwrap();

    let tickv = TicKV::<SpiNorFlashController, ERASE_SIZE_BYTES>::new(
        flash,
        storage_buffer_array,
        StorageBufferSizeBytes::USIZE,
    );

    let mut hasher = SipHasher::new();
    MAIN_KEY.hash(&mut hasher);
    tickv.initalise(hasher.finish()).unwrap();

    params
        .responder
        .reply_recv(move |req| {
            log::debug!("[persistent-storage] Processing request {}", req);
            let resp = match req {
                Request::AppendKey(key, value) => {
                    let key_hash = get_hashed_key(key.as_bytes());
                    tickv
                        .append_key(key_hash, value.as_bytes())
                        .map(Response::KeyAppended)
                }
                Request::Get(key) => {
                    let key_hash = get_hashed_key(key.as_bytes());
                    value_buffer.fill(0);
                    match tickv.get_key(key_hash, &mut value_buffer) {
                        Ok(_sc) => {
                            // Make sure it's UTF-8
                            if let Ok(s) = str::from_utf8(&value_buffer) {
                                Ok(Response::Value(Value::from(s)))
                            } else {
                                Err(ErrorCode::CorruptData)
                            }
                        }
                        Err(ec) => Err(ec),
                    }
                }
                Request::InvalidateKey(key) => {
                    let key_hash = get_hashed_key(key.as_bytes());
                    tickv.invalidate_key(key_hash).map(Response::KeyInvalidated)
                }
                Request::GarbageCollect => tickv.garbage_collect().map(Response::GarbageCollected),
            };
            if let Ok(r) = &resp {
                log::debug!("[persistent-storage] Response {}", r);
            } else {
                log::debug!("[persistent-storage] Response {:?}", resp);
            }
            resp
        })
        .expect("Could not set up a reply_recv")
        .expect("Failure on reply_recv");

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}

fn get_hashed_key(unhashed_key: &[u8]) -> u64 {
    let mut hash_function = SipHasher::new();
    unhashed_key.hash(&mut hash_function);
    hash_function.finish()
}
