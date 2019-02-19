use super::super::{
    BufferSource, BufferUnavailableError, Command, CommandDeserializationError,
    CommandSerializationError, ExpectedResponseLength, Instruction,
};
use crate::repr::split_u16;
use crate::InstructionBytes;

/// Serialization oriented representation of the mandatory portion of a T0 command header
/// [CLA, INS, P1, P2]
pub struct CommandHeader([u8; 4]);

// TODO - expand to an enum with variants if
// it becomes apparent we need more details
/// An error occurred in byte transfer down near the physical layer.
pub struct TransmissionError;

#[derive(Debug, PartialEq)]
pub enum ProtocolError {
    TransmissionError,
    CommandSerializationError(CommandSerializationError),
    CommandDeserializationError(CommandDeserializationError),
    /// Some intermediate interpretation of the protocol and input state was inconsistent
    InvalidInterpretation,
    InsufficientResponseBuffer,
    PrematureEndStatusByte(u8),
    /// We set a fixed maximum on the number of individual procedure byte cycles to run
    ExceededMaxProcedureByteCycles,
    InvalidStatusBytes(u16),
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

impl From<CommandDeserializationError> for ProtocolError {
    fn from(e: CommandDeserializationError) -> Self {
        ProtocolError::CommandDeserializationError(e)
    }
}

impl From<BufferUnavailableError> for ProtocolError {
    fn from(_: BufferUnavailableError) -> Self {
        ProtocolError::InsufficientResponseBuffer
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
    pub fn transmit_command<B: BufferSource, I: Instruction>(
        &mut self,
        command: &Command<I>,
        buffer_source: &mut B,
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

        let leftover_lc_buffer: [u8; 2];
        let mut leftover_le_buffer: [u8; 2] = [0, 0];

        // Current plan is to send 5 bytes for the command header,
        // then hand off reference slices to the rest of the command data
        // to a procedure-byte-driven loop.  The three slices we need are:
        // Pre-Data Field, Data Field, Post Data Field, really Lc, DF, Le
        let case_bytes = match (cmd_len_kind, rsp_len_kind) {
            (LengthFieldKind::None, LengthFieldKind::None) => {
                // ISO 7816-3, 12.1.2, Case 1
                // Per 12.2.2 , P3 is encoded as '00'
                self.connection.send(0)?;
                CaseAgnosticBytes {
                    expected_response_len: 0,
                    leftover_lc_len: &[],
                    data_field: &[],
                    leftover_le_len: &[],
                }
            }
            (LengthFieldKind::None, LengthFieldKind::Short) => {
                // ISO 7816-3, 12.1.2, Case 2S
                if let ExpectedResponseLength::NonZero(r_len) = &i.expected_response_length {
                    match *r_len {
                        0 => return Err(ProtocolError::InvalidInterpretation),
                        256 => {
                            self.connection.send(0)?; // '0' means the short maximum, 256
                            CaseAgnosticBytes {
                                expected_response_len: 256,
                                leftover_lc_len: &[],
                                data_field: &[],
                                leftover_le_len: &[],
                            }
                        }
                        len if len < 256 => {
                            self.connection.send(len as u8)?;
                            CaseAgnosticBytes {
                                expected_response_len: len as usize,
                                leftover_lc_len: &[],
                                data_field: &[],
                                leftover_le_len: &[],
                            }
                        }
                        _ => return Err(ProtocolError::InvalidInterpretation),
                    }
                } else {
                    return Err(ProtocolError::InvalidInterpretation);
                }
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
                        leftover_le_buffer = split_u16(r_len);
                        CaseAgnosticBytes {
                            expected_response_len: r_len as usize,
                            leftover_lc_len: &[],
                            data_field: &[],
                            leftover_le_len: &leftover_le_buffer,
                        }
                    }
                    ExpectedResponseLength::ExtendedMaximum65536 => {
                        // Note that the presence of a leading 0-byte for the expected response length
                        // is present in the 2E case, but *not* in the 4E case
                        self.connection.send(0)?;
                        CaseAgnosticBytes {
                            expected_response_len: 65_536,
                            leftover_lc_len: &[],
                            data_field: &[],
                            // The following two 0 bytes together mean the maximum, 65536.
                            leftover_le_len: &[0, 0],
                        }
                    }
                }
            }
            (LengthFieldKind::Short, LengthFieldKind::None) => {
                // ISO 7816-3, 12.1.2, Case 3S
                if let Some(cmd_field) = i.command_data_field {
                    match cmd_field.len() {
                        0 => return Err(ProtocolError::InvalidInterpretation),
                        len if len <= 255 => {
                            self.connection.send(len as u8)?;
                            CaseAgnosticBytes {
                                expected_response_len: 0,
                                leftover_lc_len: &[],
                                data_field: &cmd_field,
                                leftover_le_len: &[],
                            }
                        }
                        _ => return Err(ProtocolError::InvalidInterpretation),
                    }
                } else {
                    return Err(ProtocolError::InvalidInterpretation);
                }
            }
            (LengthFieldKind::Extended, LengthFieldKind::None) => {
                // ISO 7816-3, 12.1.2, Case 3E
                if let Some(cmd_field) = i.command_data_field {
                    match cmd_field.len() {
                        0 => return Err(ProtocolError::InvalidInterpretation),
                        len if len <= core::u16::MAX as usize => {
                            self.connection.send(0)?;
                            leftover_lc_buffer = split_u16(len as u16);
                            CaseAgnosticBytes {
                                expected_response_len: 0,
                                leftover_lc_len: &leftover_lc_buffer,
                                data_field: cmd_field,
                                leftover_le_len: &[],
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
            }
            (LengthFieldKind::Short, LengthFieldKind::Short) => {
                // ISO 7816-3, 12.1.2, Case 4S
                if let Some(cmd_field) = i.command_data_field {
                    match cmd_field.len() {
                        0 => return Err(ProtocolError::InvalidInterpretation),
                        len if len <= 255 => {
                            self.connection.send(len as u8)?;
                            if let ExpectedResponseLength::NonZero(r_len) =
                                i.expected_response_length
                            {
                                match r_len {
                                    0 => return Err(ProtocolError::InvalidInterpretation),
                                    256 => {
                                        CaseAgnosticBytes {
                                            expected_response_len: 256,
                                            leftover_lc_len: &[],
                                            data_field: cmd_field,
                                            // '0' means the short maximum, 256
                                            leftover_le_len: &[0],
                                        }
                                    }
                                    rsp_len if rsp_len < 256 => {
                                        // '0' means the short maximum, 256
                                        leftover_le_buffer[0] = rsp_len as u8;
                                        CaseAgnosticBytes {
                                            expected_response_len: rsp_len as usize,
                                            leftover_lc_len: &[],
                                            data_field: cmd_field,
                                            leftover_le_len: &leftover_le_buffer[..1],
                                        }
                                    }
                                    _ => return Err(ProtocolError::InvalidInterpretation),
                                }
                            } else {
                                return Err(ProtocolError::InvalidInterpretation);
                            }
                        }
                        _ => return Err(ProtocolError::InvalidInterpretation),
                    }
                } else {
                    return Err(ProtocolError::InvalidInterpretation);
                }
            }
            (LengthFieldKind::Extended, LengthFieldKind::Extended) => {
                // ISO 7816-3, 12.1.2, Case 4E
                if let Some(cmd_field) = i.command_data_field {
                    match cmd_field.len() {
                        0 => return Err(ProtocolError::InvalidInterpretation),
                        len if len <= core::u16::MAX as usize => {
                            self.connection.send(0)?;
                            leftover_lc_buffer = split_u16(len as u16);
                            match i.expected_response_length {
                                ExpectedResponseLength::None => {
                                    return Err(ProtocolError::InvalidInterpretation)
                                }
                                ExpectedResponseLength::NonZero(r_len) => {
                                    leftover_le_buffer = split_u16(r_len);
                                    CaseAgnosticBytes {
                                        expected_response_len: r_len as usize,
                                        leftover_lc_len: &leftover_lc_buffer,
                                        data_field: cmd_field,
                                        leftover_le_len: &leftover_le_buffer,
                                    }
                                }
                                ExpectedResponseLength::ExtendedMaximum65536 => {
                                    CaseAgnosticBytes {
                                        expected_response_len: 65_536,
                                        leftover_lc_len: &leftover_lc_buffer,
                                        data_field: cmd_field,
                                        // The following two 0 bytes together mean the maximum, 65536.
                                        leftover_le_len: &[0, 0],
                                    }
                                }
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
            }
            (LengthFieldKind::Short, LengthFieldKind::Extended)
            | (LengthFieldKind::Extended, LengthFieldKind::Short) => {
                // Note that in case 4, when we are sending command data and expected to receive response data,
                // either both length fields are short or both are extended. They have to be kept in sync.
                return Err(ProtocolError::InvalidInterpretation);
            }
        };

        let response_body = buffer_source.request_buffer(case_bytes.expected_response_len)?;
        let status_bytes =
            run_procedure_byte_loop(&mut self.connection, &i, case_bytes, response_body)?;
        Ok(command.instruction.interpret_response(i, response_body)?)
    }
}

struct CaseAgnosticBytes<'a> {
    /// Unlike the other fields, this one does not play into the procedure byte loop... probably
    expected_response_len: usize,
    leftover_lc_len: &'a [u8],
    data_field: &'a [u8],
    leftover_le_len: &'a [u8],
}

struct StatusBytes {
    sw1: u8,
    sw2: u8,
}

fn run_procedure_byte_loop<C: Connection>(
    connection: &mut C,
    instruction_bytes: &InstructionBytes<'_>,
    case_bytes: CaseAgnosticBytes,
    response_body_buffer: &mut [u8],
) -> Result<Status, ProtocolError> {
    const SIXTY: u8 = 0b0110_0000; // Hex '60'
    let is_in_sixties_or_nineties_but_not_sixty =
        |val: u8| ((val >> 4) == 6u8 && (val << 4) != 0u8) || ((val >> 4) == 9u8);

    #[derive(Debug, PartialEq)]
    enum Cursor {
        OnLCBytes(usize),
        OnCmdDataFieldBytes(usize),
        OnLEBytes(usize),
        OnResponseBytes(usize), // ???
        OnTrailerBytes,
        Done,
    }
    let mut cursor = match (
        case_bytes.leftover_lc_len.len(),
        case_bytes.data_field.len(),
        case_bytes.leftover_le_len.len(),
    ) {
        (0, 0, 0) => {
            if case_bytes.expected_response_len == 0 {
                Cursor::OnTrailerBytes
            } else {
                Cursor::OnResponseBytes(0)
            }
        }
        (0, 0, le) => Cursor::OnLEBytes(0),
        (0, cdf, _) => Cursor::OnCmdDataFieldBytes(0),
        (lc, _, _) => Cursor::OnLCBytes(0),
    };

    if cursor == Cursor::Done {
        return Err(ProtocolError::InvalidInterpretation);
    }
    let ack_all_byte = instruction_bytes.instruction;
    let ack_single_byte = instruction_bytes.instruction ^ 0b1111_1111;

    // TODO - we may need to be more lax when there are fewer response bytes supplied than the
    // maximum possible. Alternately, we need to pipe down a parameter that lets us know when
    // we are dealing with an *absolute expected response length* and a *maximum possible with less allowed response length*

    for i in 0..core::usize::MAX {
        let current = connection.receive()?;
        match current {
            SIXTY => continue,
            c if is_in_sixties_or_nineties_but_not_sixty(c) => {
                if cursor == Cursor::OnTrailerBytes {
                    let sw1 = c;
                    let sw2 = connection.receive()?;
                    return interpret_sws(sw1, sw2);
                } else {
                    // We don't think we belong here, e.g., because we still think there are more response bytes to come
                    return Err(ProtocolError::PrematureEndStatusByte(c));
                }
            }
            ack_all_byte => {
                match cursor {
                    Cursor::OnLCBytes(i) => {
                        send_all(connection, &case_bytes.leftover_lc_len[i..])?;
                        send_all(connection, case_bytes.data_field)?;
                        send_all(connection, case_bytes.leftover_le_len)?;
                        for i in 0..case_bytes.expected_response_len {
                            response_body_buffer[i] = connection.receive()?;
                        }
                        cursor = Cursor::OnTrailerBytes;
                    }
                    Cursor::OnCmdDataFieldBytes(i) => {
                        send_all(connection, &case_bytes.data_field[i..])?;
                        send_all(connection, case_bytes.leftover_le_len)?;
                        for i in 0..case_bytes.expected_response_len {
                            response_body_buffer[i] = connection.receive()?;
                        }
                        cursor = Cursor::OnTrailerBytes;
                    }
                    Cursor::OnLEBytes(i) => {
                        send_all(connection, &case_bytes.leftover_le_len[i..])?;
                        for i in 0..case_bytes.expected_response_len {
                            response_body_buffer[i] = connection.receive()?;
                        }
                        cursor = Cursor::OnTrailerBytes;
                    }
                    Cursor::OnResponseBytes(initial_index) => {
                        for i in initial_index..case_bytes.expected_response_len {
                            response_body_buffer[i] = connection.receive()?;
                        }
                        cursor = Cursor::OnTrailerBytes;
                    }
                    Cursor::OnTrailerBytes => continue,
                    Cursor::Done => {
                        // We don't expect to be getting an ack response after thinking we're done
                        return Err(ProtocolError::InvalidInterpretation);
                    }
                }
            }
            ack_single_byte => {
                match cursor {
                    Cursor::OnLCBytes(i) => {
                        connection.send(case_bytes.leftover_lc_len[i])?;
                        cursor = if i + 1 > case_bytes.leftover_lc_len.len() {
                            match (
                                case_bytes.data_field.len(),
                                case_bytes.leftover_le_len.len(),
                            ) {
                                (0, 0) => {
                                    if case_bytes.expected_response_len == 0 {
                                        Cursor::OnTrailerBytes
                                    } else {
                                        Cursor::OnResponseBytes(0)
                                    }
                                }
                                (0, _le) => Cursor::OnLEBytes(0),
                                (_cdf, _) => Cursor::OnCmdDataFieldBytes(0),
                            }
                        } else {
                            Cursor::OnLCBytes(i + 1)
                        }
                    }
                    Cursor::OnCmdDataFieldBytes(i) => {
                        connection.send(case_bytes.data_field[i])?;
                        cursor = if i + 1 > case_bytes.data_field.len() {
                            if case_bytes.leftover_le_len.len() > 0 {
                                Cursor::OnLEBytes(0)
                            } else if case_bytes.expected_response_len == 0 {
                                Cursor::OnTrailerBytes
                            } else {
                                Cursor::OnResponseBytes(0)
                            }
                        } else {
                            Cursor::OnCmdDataFieldBytes(i + 1)
                        };
                    }
                    Cursor::OnLEBytes(i) => {
                        connection.send(case_bytes.leftover_le_len[i])?;
                        cursor = if i + 1 > case_bytes.leftover_le_len.len() {
                            if case_bytes.expected_response_len == 0 {
                                Cursor::OnTrailerBytes
                            } else {
                                Cursor::OnResponseBytes(0)
                            }
                        } else {
                            Cursor::OnLEBytes(i + 1)
                        };
                    }
                    Cursor::OnResponseBytes(i) => {
                        if i > response_body_buffer.len() {
                            return Err(ProtocolError::InsufficientResponseBuffer);
                        }
                        response_body_buffer[i] = connection.receive()?;
                        cursor = if i + 1 >= case_bytes.expected_response_len {
                            Cursor::OnTrailerBytes
                        } else {
                            Cursor::OnResponseBytes(i + 1)
                        }
                    }
                    Cursor::OnTrailerBytes => continue,
                    Cursor::Done => {
                        // We don't expect to be getting an ack response after thinking we're done
                        return Err(ProtocolError::InvalidInterpretation);
                    }
                }
            }
        }
    }
    Err(ProtocolError::ExceededMaxProcedureByteCycles)
}

fn send_all<C: Connection>(connection: &mut C, bytes: &[u8]) -> Result<usize, TransmissionError> {
    for b in bytes {
        connection.send(*b)?;
    }
    Ok(bytes.len())
}

/// This comes from -3 12.2.1 Table 14. It uses the status bytes SW1 &
/// SW2 to ascribe a status to a command/response exchange.
#[derive(Debug, PartialEq)]
enum Status {
    /// A fixed value of 0x9000.
    CompletedNormally,
    /// 0x61XY: The process completed successfully, but the card has
    /// bytes remaining. In cases 1 & 3, the card should not use this
    /// value.
    CompletedNormallyWithBytesRemaining(u8),
    /// 0x62XY: The process completed with warning.
    ///
    /// ## Note
    /// The specific meaning of the second byte can found in -4:
    ///
    /// > e.g., '6202' to '6280', GET DATA command for transferring a
    /// > card-originated byte string, see ISO/IEC 7816-4.
    CompletedWithWarningA(u8),
    /// 0x63XY: TODO(pittma): No clear meaning as to what this one
    /// means. Maybe in -4?
    CompletedWithWarningB(u8),
    /// A fixed value of 0x6700.
    AbortedWithWrongLen,
    /// 0x6CXY: There was an $L\_e$ mismatch. Upon receipt, the
    /// expected response shall contain a P3 equal to the second byte.
    AbortedWithWrongExpectedLen(u8),
    /// The given instruction was invalid, or the card does not
    /// implement it.
    BadOrUnimplementedInstruction,
}

fn interpret_sws(sw1: u8, sw2: u8) -> Result<Status, ProtocolError> {
    let joined = ((sw1 as u16) << 8) | (sw2 as u16);
    match joined {
        0x9000 => Ok(Status::CompletedNormally),
        0x6700 => Ok(Status::AbortedWithWrongLen),
        0x6D00 => Ok(Status::BadOrUnimplementedInstruction),

        // These cases are intrepreted through masking the first byte:
        //   1. The match is determined by and-ing with the case's
        //      mask. If the given value falls within the range, anding
        //      it with the mask shall produce the mask.
        //   2. By anding the value with the inverse of the mask, we cancel out
        //      the first byte, but the on bits in the second byte are carried
        //      through, leaving a u16 with the value of only the second byte.
        j if joined & 0x6100 == 0x6100 => Ok(Status::CompletedNormallyWithBytesRemaining(
            (!0x6100 & j) as u8,
        )),
        j if joined & 0x6200 == 0x6200 => Ok(Status::CompletedWithWarningA((!0x6200 & j) as u8)),
        j if joined & 0x6300 == 0x6300 => Ok(Status::CompletedWithWarningB((!0x6300 & j) as u8)),
        j if joined & 0x6C00 == 0x6C00 => {
            Ok(Status::AbortedWithWrongExpectedLen((!0x6C00 & j) as u8))
        }
        _ => Err(ProtocolError::InvalidStatusBytes(joined)),
    }
}

#[cfg(test)]
mod test_sws {

    use super::{interpret_sws, Status};

    #[test]
    fn test_norm() {
        assert_eq!(interpret_sws(0x90, 0), Ok(Status::CompletedNormally));
    }

    #[test]
    fn test_wrong_len() {
        assert_eq!(interpret_sws(0x67, 0), Ok(Status::AbortedWithWrongLen));
    }

    #[test]
    fn test_bad_inst() {
        assert_eq!(
            interpret_sws(0x6D, 0),
            Ok(Status::BadOrUnimplementedInstruction)
        );
    }

    #[test]
    fn test_bytes_remain() {
        assert_eq!(
            interpret_sws(0x61, 47),
            Ok(Status::CompletedNormallyWithBytesRemaining(47))
        );
    }

    #[test]
    fn test_wrong_elen() {
        assert_eq!(
            interpret_sws(0x6C, 47),
            Ok(Status::AbortedWithWrongExpectedLen(47))
        );
    }
}

//fn send_short_command_data_field<C: Connection>(
//    command_data_field: &Option<&[u8]>,
//    connection: &mut C,
//) -> Result<(), ProtocolError> {
//    if let Some(cmd_field) = *command_data_field {
//        match cmd_field.len() {
//            0 => return Err(ProtocolError::InvalidInterpretation),
//            len if len <= 255 => {
//                connection.send(len as u8)?;
//                for b in cmd_field {
//                    connection.send(*b)?; // Actually send the command data field contents
//                }
//            }
//            _ => return Err(ProtocolError::InvalidInterpretation),
//        }
//    } else {
//        return Err(ProtocolError::InvalidInterpretation);
//    }
//    Ok(())
//}

//fn send_extended_command_data_field<C: Connection>(
//    command_data_field: &Option<&[u8]>,
//    connection: &mut C,
//) -> Result<(), ProtocolError> {
//    if let Some(cmd_field) = *command_data_field {
//        match cmd_field.len() {
//            0 => return Err(ProtocolError::InvalidInterpretation),
//            len if len <= core::u16::MAX as usize => {
//                connection.send(0)?;
//                let len_halves = split_u16(len as u16);
//                connection.send(len_halves[0])?;
//                connection.send(len_halves[1])?;
//                for b in cmd_field {
//                    connection.send(*b)?; // Actually send the command data field contents
//                }
//            }
//            _ => {
//                return Err(ProtocolError::CommandSerializationError(
//                    CommandSerializationError::TooManyBytesForCommandDataField,
//                ))
//            }
//        }
//    } else {
//        return Err(ProtocolError::InvalidInterpretation);
//    }
//    Ok(())
//}
//
//fn send_short_expected_response_length<C: Connection>(
//    expected_response_length: &ExpectedResponseLength,
//    connection: &mut C,
//) -> Result<usize, ProtocolError> {
//    if let ExpectedResponseLength::NonZero(r_len) = expected_response_length {
//        match *r_len {
//            0 => return Err(ProtocolError::InvalidInterpretation),
//            256 => {
//                connection.send(0)?; // '0' means the short maximum, 256
//                Ok(256)
//            }
//            len if len < 256 => {
//                connection.send(len as u8)?;
//                Ok(len as usize)
//            }
//            _ => return Err(ProtocolError::InvalidInterpretation),
//        }
//    } else {
//        return Err(ProtocolError::InvalidInterpretation);
//    }
//}

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
