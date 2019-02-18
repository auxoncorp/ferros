use super::super::{Command, CommandSerializationError, ExpectedResponseLength, Instruction};
use crate::repr::split_u16;

/// Serialization oriented representation of the mandatory portion of a T0 command header
/// [CLA, INS, P1, P2]
pub struct CommandHeader([u8; 4]);

// TODO - expand to an enum with variants if
// it becomes apparent we need more details
/// An error occurred in byte transfer down near the physical layer.
pub struct TransmissionError;

pub enum ProtocolError {
    TransmissionError,
    CommandSerializationError(CommandSerializationError),
    /// Some intermediate interpretation of the protocol and input state was inconsistent
    InvalidInterpretation,
    // TODO - more variants
}

impl From<TransmissionError> for ProtocolError {
    fn from(_: TransmissionError) -> Self {
        ProtocolError::TransmissionError
    }
}

impl From<CommandSerializationError> for ProtocolError {
    fn from(e: CommandSerializationError) -> Self {
        ProtocolError::CommandSerializationError(e)
    }
}

// Half duplex single-byte
pub trait Connection {
    fn send(&mut self, byte: u8) -> Result<(), TransmissionError>;
    fn receive(&mut self) -> Result<u8, TransmissionError>;
}

pub struct ProtocolState<C: Connection> {
    connection: C,
}

impl<C: Connection> ProtocolState<C> {
    pub fn transmit_command<I: Instruction>(
        &mut self,
        command: &Command<I>,
    ) -> Result<I::Response, ProtocolError> {
        let class_byte = command.class.to_byte();
        self.connection.send(class_byte)?; // CLA
        let i = command.instruction.to_instruction_bytes()?;
        self.connection.send(i.instruction)?; // INS
        self.connection.send(i.parameter_1)?; // P1
        self.connection.send(i.parameter_2)?; // P2
        let (cmd_len_kind, rsp_len_kind) =
            LengthFieldKind::infer_command_response_length_pair_kinds(
                &i.command_data_field,
                &i.expected_response_length,
            );
        if APDUCase::from((cmd_len_kind, rsp_len_kind)) == APDUCase::Invalid {
            return Err(ProtocolError::InvalidInterpretation);
        }

        let expected_response_len = match (cmd_len_kind, rsp_len_kind) {
            (LengthFieldKind::None, LengthFieldKind::None) => 0, // ISO 7816-3, 12.1.2, Case 1
            (LengthFieldKind::None, LengthFieldKind::Short) => {
                // ISO 7816-3, 12.1.2, Case 2S
                send_short_expected_response_length(
                    &i.expected_response_length,
                    &mut self.connection,
                )?
            }
            (LengthFieldKind::None, LengthFieldKind::Extended) => {
                // ISO 7816-3, 12.1.2, Case 2E
                match i.expected_response_length {
                    ExpectedResponseLength::None => {
                        return Err(ProtocolError::InvalidInterpretation)
                    }
                    ExpectedResponseLength::NonZero(r_len) => {
                        // Note that the presence of a leading 0-byte for the expected response length
                        // is present in the 2E case, but *not* in the 4E case
                        self.connection.send(0)?;
                        let len_halves = split_u16(r_len);
                        self.connection.send(len_halves[0])?;
                        self.connection.send(len_halves[1])?;

                        r_len as usize
                    }
                    ExpectedResponseLength::ExtendedMaximum65536 => {
                        // Note that the presence of a leading 0-byte for the expected response length
                        // is present in the 2E case, but *not* in the 4E case
                        self.connection.send(0)?;
                        // The following two 0 bytes together mean the maximum, 65536.
                        self.connection.send(0)?;
                        self.connection.send(0)?;

                        65_536
                    }
                }
            }
            (LengthFieldKind::Short, LengthFieldKind::None) => {
                // ISO 7816-3, 12.1.2, Case 3S
                send_short_command_data_field(&i.command_data_field, &mut self.connection)?;
                0
            }
            (LengthFieldKind::Extended, LengthFieldKind::None) => {
                // ISO 7816-3, 12.1.2, Case 3E
                send_extended_command_data_field(&i.command_data_field, &mut self.connection)?;
                0
            }
            (LengthFieldKind::Short, LengthFieldKind::Short) => {
                // ISO 7816-3, 12.1.2, Case 4S
                send_short_command_data_field(&i.command_data_field, &mut self.connection)?;
                send_short_expected_response_length(
                    &i.expected_response_length,
                    &mut self.connection,
                )?
            }
            (LengthFieldKind::Extended, LengthFieldKind::Extended) => {
                // ISO 7816-3, 12.1.2, Case 4E
                send_extended_command_data_field(&i.command_data_field, &mut self.connection)?;
                match i.expected_response_length {
                    ExpectedResponseLength::None => {
                        return Err(ProtocolError::InvalidInterpretation)
                    }
                    ExpectedResponseLength::NonZero(r_len) => {
                        let len_halves = split_u16(r_len);
                        self.connection.send(len_halves[0])?;
                        self.connection.send(len_halves[1])?;

                        r_len as usize
                    }
                    ExpectedResponseLength::ExtendedMaximum65536 => {
                        // The following two 0 bytes together mean the maximum, 65536.
                        self.connection.send(0)?;
                        self.connection.send(0)?;

                        65_536
                    }
                }
            }
            (LengthFieldKind::Short, LengthFieldKind::Extended)
            | (LengthFieldKind::Extended, LengthFieldKind::Short) => {
                // Note that in case 4, when we are sending command data and expected to receive response data,
                // either both length fields are short or both are extended. They have to be kept in sync.
                return Err(ProtocolError::InvalidInterpretation);
            }
        };

        // TODO - let's listen for some response data!
        unimplemented!()
    }
}

fn send_short_command_data_field<C: Connection>(
    command_data_field: &Option<&[u8]>,
    connection: &mut C,
) -> Result<(), ProtocolError> {
    if let Some(cmd_field) = *command_data_field {
        match cmd_field.len() {
            0 => return Err(ProtocolError::InvalidInterpretation),
            len if len <= 255 => {
                connection.send(len as u8)?;
                for b in cmd_field {
                    connection.send(*b)?; // Actually send the command data field contents
                }
            }
            _ => return Err(ProtocolError::InvalidInterpretation),
        }
    } else {
        return Err(ProtocolError::InvalidInterpretation);
    }
    Ok(())
}

fn send_extended_command_data_field<C: Connection>(
    command_data_field: &Option<&[u8]>,
    connection: &mut C,
) -> Result<(), ProtocolError> {
    if let Some(cmd_field) = *command_data_field {
        match cmd_field.len() {
            0 => return Err(ProtocolError::InvalidInterpretation),
            len if len <= core::u16::MAX as usize => {
                connection.send(0)?;
                let len_halves = split_u16(len as u16);
                connection.send(len_halves[0])?;
                connection.send(len_halves[1])?;
                for b in cmd_field {
                    connection.send(*b)?; // Actually send the command data field contents
                }
            }
            _ => {
                return Err(ProtocolError::CommandSerializationError(
                    CommandSerializationError::TooManyBytesForCommandDataField,
                ))
            }
        }
    } else {
        return Err(ProtocolError::InvalidInterpretation);
    }
    Ok(())
}

fn send_short_expected_response_length<C: Connection>(
    expected_response_length: &ExpectedResponseLength,
    connection: &mut C,
) -> Result<usize, ProtocolError> {
    if let ExpectedResponseLength::NonZero(r_len) = expected_response_length {
        match *r_len {
            0 => return Err(ProtocolError::InvalidInterpretation),
            256 => {
                connection.send(0)?; // '0' means the short maximum, 256
                Ok(256)
            }
            len if len < 256 => {
                connection.send(len as u8)?;
                Ok(len as usize)
            }
            _ => return Err(ProtocolError::InvalidInterpretation),
        }
    } else {
        return Err(ProtocolError::InvalidInterpretation);
    }
}

#[derive(Copy, Clone)]
enum LengthFieldKind {
    None,
    Short,
    Extended,
}

impl LengthFieldKind {
    fn infer_command_response_length_pair_kinds(
        command_data_field: &Option<&[u8]>,
        expected_response_length: &ExpectedResponseLength,
    ) -> (LengthFieldKind, LengthFieldKind) {
        match (command_data_field, expected_response_length) {
            (None, ExpectedResponseLength::None) => (LengthFieldKind::None, LengthFieldKind::None), // Case 1
            (None, ExpectedResponseLength::ExtendedMaximum65536) => {
                (LengthFieldKind::None, LengthFieldKind::Extended)
            } // Case 2E
            (None, ExpectedResponseLength::NonZero(rsp_len)) => match *rsp_len {
                0 => (LengthFieldKind::None, LengthFieldKind::None), // Case 1
                r_len if r_len > 0 && r_len <= 256 => {
                    (LengthFieldKind::None, LengthFieldKind::Short)
                } // Case 2S
                _ => (LengthFieldKind::None, LengthFieldKind::Extended), // Case 2E
            },
            (Some(cmd_field), ExpectedResponseLength::None) => match cmd_field.len() {
                0 => (LengthFieldKind::None, LengthFieldKind::None), // Case 1
                c_len if c_len > 0 && c_len < 256 => {
                    (LengthFieldKind::Short, LengthFieldKind::None)
                } // Case 3S
                _ => (LengthFieldKind::Extended, LengthFieldKind::None), // Case 3E
            },
            (Some(cmd_field), ExpectedResponseLength::ExtendedMaximum65536) => {
                match cmd_field.len() {
                    0 => (LengthFieldKind::None, LengthFieldKind::Extended), // Case 2E
                    _ => (LengthFieldKind::Extended, LengthFieldKind::Extended), // Case 4E
                }
            }
            (Some(cmd_field), ExpectedResponseLength::NonZero(rsp_len)) => {
                match (cmd_field.len(), *rsp_len) {
                    (0, 0) => (LengthFieldKind::None, LengthFieldKind::None), // Case 1
                    (0, r_len) if r_len > 0 && r_len <= 256 => {
                        (LengthFieldKind::None, LengthFieldKind::Short)
                    } // Case 2S
                    (c_len, 0) if c_len > 0 && c_len < 256 => {
                        (LengthFieldKind::Short, LengthFieldKind::None)
                    } // Case 3S
                    (c_len, r_len) if c_len < 256 && r_len <= 256 => {
                        (LengthFieldKind::Short, LengthFieldKind::Short)
                    } // Case 4S
                    _ => (LengthFieldKind::Extended, LengthFieldKind::Extended), // Case 4E
                }
            }
        }
    }
}

#[derive(PartialEq)]
enum APDUCase {
    Invalid,
    Case1,
    Case2S,
    Case2E,
    Case3S,
    Case3E,
    Case4S,
    Case4E,
}

impl From<(LengthFieldKind, LengthFieldKind)> for APDUCase {
    fn from((lc, le): (LengthFieldKind, LengthFieldKind)) -> Self {
        match (lc, le) {
            (LengthFieldKind::None, LengthFieldKind::None) => APDUCase::Case1,
            (LengthFieldKind::None, LengthFieldKind::Short) => APDUCase::Case2S,
            (LengthFieldKind::None, LengthFieldKind::Extended) => APDUCase::Case2E,
            (LengthFieldKind::Short, LengthFieldKind::None) => APDUCase::Case3S,
            (LengthFieldKind::Extended, LengthFieldKind::None) => APDUCase::Case3E,
            (LengthFieldKind::Short, LengthFieldKind::Short) => APDUCase::Case4S,
            (LengthFieldKind::Extended, LengthFieldKind::Extended) => APDUCase::Case4E,
            _ => APDUCase::Invalid,
        }
    }
}
