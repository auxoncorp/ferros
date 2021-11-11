#![no_std]
#![no_main]

use selfe_runtime as _;

use console::ProcParams;
use core::fmt::{self, Write as WriteFmt};
use debug_logger::DebugLogger;
use ferros::{
    cap::role,
    userland::{Caller, Producer},
};
use imx6_hal::embedded_hal::serial::Read;
use imx6_hal::{pac::uart1::UART1, serial::Serial};
use menu::*;
use net_types::{EthernetFrameBuffer, IpcUdpTransmitBuffer};

static LOGGER: DebugLogger = DebugLogger;

#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(DebugLogger::max_log_level_from_env()))
        .unwrap();

    log::debug!("[console] Process started");

    let int_consumer = params.int_consumer;
    let serial = Serial::new(params.uart);
    let context = Context {
        serial,
        storage_caller: params.storage_caller,
        udp_producer: params.udp_producer,
    };

    let mut console_buffer_mem = params.console_buffer;
    console_buffer_mem.flush().unwrap();
    let console_buffer = console_buffer_mem.as_mut_slice();
    console_buffer.fill(0);
    let state = Runner::new(&ROOT_MENU, console_buffer, context);

    // TODO - this info is only if running on QEMU, otherwise it's the UART1 serial
    // port
    log::info!("[console] Run 'telnet 0.0.0.0 8888' to connect to the console interface (QEMU)");
    int_consumer.consume(state, move |mut state| {
        if let Ok(b) = state.context.serial.read() {
            state.input_byte(b);
        }
        state
    })
}

pub struct Context {
    serial: Serial<UART1>,
    storage_caller: Caller<
        persistent_storage::Request,
        Result<persistent_storage::Response, persistent_storage::ErrorCode>,
        role::Local,
    >,
    udp_producer: Producer<role::Local, IpcUdpTransmitBuffer>,
}

impl fmt::Write for Context {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.serial.write_str(s)
    }
}

const ROOT_MENU: Menu<Context> = Menu {
    label: "root",
    items: &[
        &Item {
            command: "storage",
            help: Some("Enter the persistent storage sub-menu."),
            item_type: ItemType::Menu(&Menu {
                label: "storage",
                items: &[
                    &Item {
                        command: "append",
                        help: Some(storage::append::HELP),
                        item_type: ItemType::Callback {
                            function: storage::append::cmd,
                            parameters: &[
                                Parameter::Mandatory {
                                    parameter_name: "key",
                                    help: Some("The entry's key string"),
                                },
                                Parameter::Mandatory {
                                    parameter_name: "value",
                                    help: Some("The entry's value string"),
                                },
                            ],
                        },
                    },
                    &Item {
                        command: "get",
                        help: Some(storage::get::HELP),
                        item_type: ItemType::Callback {
                            function: storage::get::cmd,
                            parameters: &[Parameter::Mandatory {
                                parameter_name: "key",
                                help: Some("The entry's key string"),
                            }],
                        },
                    },
                    &Item {
                        command: "invalidate",
                        help: Some(storage::invalidate::HELP),
                        item_type: ItemType::Callback {
                            function: storage::invalidate::cmd,
                            parameters: &[Parameter::Mandatory {
                                parameter_name: "key",
                                help: Some("The entry's key string"),
                            }],
                        },
                    },
                    &Item {
                        command: "gc",
                        help: Some(storage::gc::HELP),
                        item_type: ItemType::Callback {
                            function: storage::gc::cmd,
                            parameters: &[],
                        },
                    },
                ],
                entry: None,
                exit: None,
            }),
        },
        &Item {
            command: "net",
            help: Some("Enter the network sub-menu."),
            item_type: ItemType::Menu(&Menu {
                label: "net",
                items: &[&Item {
                    command: "sendto",
                    help: Some(net::sendto::HELP),
                    item_type: ItemType::Callback {
                        function: net::sendto::cmd,
                        parameters: &[
                            Parameter::Mandatory {
                                parameter_name: "addr",
                                help: Some("The remote address"),
                            },
                            Parameter::Mandatory {
                                parameter_name: "port",
                                help: Some("The remote port number"),
                            },
                            Parameter::Mandatory {
                                parameter_name: "data",
                                help: Some("The data to send"),
                            },
                        ],
                    },
                }],
                entry: None,
                exit: None,
            }),
        },
    ],
    entry: Some(enter_root_menu),
    exit: None,
};

// NOTE: you won't see this in QEMU emulation unless you remove
// the 'nowait' parameter from the QEMU invocation
// in scripts/simulate.sh
fn enter_root_menu(_menu: &Menu<Context>, context: &mut Context) {
    writeln!(context, "\n\n").unwrap();
    writeln!(context, "***************************").unwrap();
    writeln!(context, "* Welcome to the console! *").unwrap();
    writeln!(context, "***************************").unwrap();
}

mod storage {
    use super::*;
    use persistent_storage::{ErrorCode, Key, Request, Response, Value};

    fn print_resp(context: &mut Context, resp: &Result<Response, ErrorCode>) {
        if let Ok(r) = resp {
            writeln!(context.serial, "{}", r).unwrap();
        } else {
            writeln!(context.serial, "{:?}", resp).unwrap();
        }
    }

    pub mod append {
        use super::*;

        pub const HELP: &str = "Appends the key/value pair to storage.

  Example:
  append my-key my-data";

        pub fn cmd(
            _menu: &Menu<Context>,
            item: &Item<Context>,
            args: &[&str],
            context: &mut Context,
        ) {
            let key = Key::from(menu::argument_finder(item, args, "key").unwrap().unwrap());
            let value = Value::from(menu::argument_finder(item, args, "value").unwrap().unwrap());

            log::debug!(
                "[console] Append storage item key='{}' value='{}'",
                key,
                value
            );

            let resp = context
                .storage_caller
                .blocking_call(&Request::AppendKey(key, value))
                .expect("Failed to perform a blocking_call");

            print_resp(context, &resp);
        }
    }

    pub mod get {
        use super::*;

        pub const HELP: &str = "Retrieves the value for the given key from storage.

  Example:
  get my-key";

        pub fn cmd(
            _menu: &Menu<Context>,
            item: &Item<Context>,
            args: &[&str],
            context: &mut Context,
        ) {
            let key = Key::from(menu::argument_finder(item, args, "key").unwrap().unwrap());

            log::debug!("[console] Get storage value for key='{}'", key);

            let resp = context
                .storage_caller
                .blocking_call(&Request::Get(key))
                .expect("Failed to perform a blocking_call");

            print_resp(context, &resp);
        }
    }

    pub mod invalidate {
        use super::*;

        pub const HELP: &str = "Invalidates the key in storage.

  Example:
  invalidate my-key";

        pub fn cmd(
            _menu: &Menu<Context>,
            item: &Item<Context>,
            args: &[&str],
            context: &mut Context,
        ) {
            let key = Key::from(menu::argument_finder(item, args, "key").unwrap().unwrap());

            log::debug!("[console] Invalidate storage key='{}'", key);

            let resp = context
                .storage_caller
                .blocking_call(&Request::InvalidateKey(key))
                .expect("Failed to perform a blocking_call");

            print_resp(context, &resp);
        }
    }

    pub mod gc {
        use super::*;

        pub const HELP: &str = "Perform a garbage collection on storage.

  Example:
  gc";

        pub fn cmd(
            _menu: &Menu<Context>,
            _item: &Item<Context>,
            _args: &[&str],
            context: &mut Context,
        ) {
            log::debug!("[console] Garbage collect storage");

            let resp = context
                .storage_caller
                .blocking_call(&Request::GarbageCollect)
                .expect("Failed to perform a blocking_call");

            print_resp(context, &resp);
        }
    }
}

mod net {
    use super::*;

    pub mod sendto {
        use super::*;

        pub const HELP: &str = "Send a UDP message.
  
    Example:
    sendto 192.0.2.2 4567 hello";

        pub fn cmd(
            _menu: &Menu<Context>,
            item: &Item<Context>,
            args: &[&str],
            context: &mut Context,
        ) {
            let addr = menu::argument_finder(item, args, "addr").unwrap().unwrap();
            let mut addr_octets = [0_u8; 4];
            for (idx, part) in addr.split('.').into_iter().enumerate() {
                addr_octets[idx] = part.parse().unwrap();
            }

            let port = menu::argument_finder(item, args, "port").unwrap().unwrap();
            let port: u16 = port.parse().unwrap();

            let data = menu::argument_finder(item, args, "data").unwrap().unwrap();
            let data_bytes = data.as_bytes();
            let data_len = data_bytes.len();

            let mut msg = IpcUdpTransmitBuffer {
                dst_addr: addr_octets.into(),
                dst_port: port.into(),
                frame: EthernetFrameBuffer::new(),
            };
            msg.frame.truncate(data_len);
            msg.frame.as_mut_slice().copy_from_slice(data_bytes);

            log::debug!(
                "[console] Send UDP message to {}:{} data='{}'",
                addr,
                port,
                data
            );

            if context.udp_producer.send(msg).is_err() {
                log::warn!("[console] Rejected sending IpcUdpTransmitBuffer data to TCP/IP driver");
            }
        }
    }
}
