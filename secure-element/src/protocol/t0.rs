use super::super::{
    BufferSource, BufferUnavailableError, Command, CommandDeserializationError,
    CommandSerializationError, ExpectedResponseLength, Instruction,
};
use core::fmt::Debug;
use crate::repr::split_u16;
use crate::InstructionBytes;

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
    /// An error was indicated by the status bytes
    StatusError(StatusError),
    ///
    InvalidProcedureByte(u8),
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

impl From<StatusError> for ProtocolError {
    fn from(se: StatusError) -> Self {
        ProtocolError::StatusError(se)
    }
}

/// Half duplex single-byte connection
pub trait Connection {
    fn send(&mut self, byte: u8) -> Result<(), TransmissionError>;
    fn receive(&mut self) -> Result<u8, TransmissionError>;
}

pub struct ProtocolState<C: Connection> {
    connection: C,
}

impl<C: Connection> Debug for ProtocolState<C>
where
    C: Debug,
{
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> Result<(), ::core::fmt::Error> {
        self.connection.fmt(f)
    }
}

impl<C: Connection> ProtocolState<C> {
    pub fn transmit_command<B: BufferSource, I: Instruction>(
        &mut self,
        command: &Command<I>,
        buffer_source: &mut B,
    ) -> Result<I::Response, ProtocolError> {
        // Send 5 bytes for the command header
        // at this top level, then hand off reference slices to the rest of the command data
        // to a procedure-byte-driven loop.
        let class_byte = command.class.to_byte();
        self.connection.send(class_byte)?; // CLA
        let i = command.instruction.to_instruction_bytes()?;
        self.connection.send(i.instruction)?; // INS
        self.connection.send(i.parameter_1)?; // P1
        self.connection.send(i.parameter_2)?; // P2

        // P3's contents are highly case-specific
        let (cmd_len_kind, rsp_len_kind) =
            LengthFieldKind::infer_command_response_length_pair_kinds(
                &i.command_data_field,
                &i.expected_response_length,
            );
        if APDUCase::from((cmd_len_kind, rsp_len_kind)) == APDUCase::Invalid {
            return Err(ProtocolError::InvalidInterpretation);
        }

        let mut leftover_lc_buffer: [u8; 2] = [0, 0];
        let mut leftover_le_buffer: [u8; 2] = [0, 0];

        let case_agnostic: Agnostic = compute_case_agnostic_transfer_plan(
            &i,
            cmd_len_kind,
            rsp_len_kind,
            &mut leftover_lc_buffer,
            &mut leftover_le_buffer,
        )?;
        self.connection.send(case_agnostic.p3)?; // Final byte in the 5-byte command header

        let response_body = buffer_source.request_buffer(case_agnostic.expected_response_len)?;
        let _status = run_procedure_byte_loop(
            &mut self.connection,
            command.class.to_byte(),
            &i,
            &case_agnostic,
            response_body,
        )?;
        // TODO - consider whether to pass StatusCompleted content (since it may contain warning data)
        // to the response-interpretation layer
        Ok(command.instruction.interpret_response(i, response_body)?)
    }
}

/// Track data that is expected to be transferred for a command,
/// represented in a way that is case-agnostic.
struct Agnostic<'a> {
    /// Unlike the other fields, this one does not play into the procedure byte loop,
    /// as it represents the fifth byte of the command header and is sent before the procedure loop.
    p3: u8,
    expected_response_len: usize,
    leftover_lc_len: &'a [u8],
    data_field: &'a [u8],
    leftover_le_len: &'a [u8],
}

fn compute_case_agnostic_transfer_plan<'a>(
    i: &'a InstructionBytes,
    cmd_len_kind: LengthFieldKind,
    rsp_len_kind: LengthFieldKind,
    leftover_lc_buffer: &'a mut [u8; 2],
    leftover_le_buffer: &'a mut [u8; 2],
) -> Result<Agnostic<'a>, ProtocolError> {
    let agnostic = match (cmd_len_kind, rsp_len_kind) {
        (LengthFieldKind::None, LengthFieldKind::None) => {
            // ISO 7816-3, 12.1.2, Case 1
            // Per 12.2.2 , P3 is encoded as '00'
            Agnostic {
                p3: 0,
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
                        Agnostic {
                            p3: 0, // '0' means the short maximum, 256
                            expected_response_len: 256,
                            leftover_lc_len: &[],
                            data_field: &[],
                            leftover_le_len: &[],
                        }
                    }
                    len if len < 256 => Agnostic {
                        p3: len as u8,
                        expected_response_len: len as usize,
                        leftover_lc_len: &[],
                        data_field: &[],
                        leftover_le_len: &[],
                    },
                    _ => return Err(ProtocolError::InvalidInterpretation),
                }
            } else {
                return Err(ProtocolError::InvalidInterpretation);
            }
        }
        (LengthFieldKind::None, LengthFieldKind::Extended) => {
            // ISO 7816-3, 12.1.2, Case 2E
            match i.expected_response_length {
                ExpectedResponseLength::None => return Err(ProtocolError::InvalidInterpretation),
                ExpectedResponseLength::NonZero(r_len) => {
                    // Note that the presence of a leading 0-byte for the expected response length
                    // is present in the 2E case, but *not* in the 4E case
                    let p3 = 0;
                    let rsp_len_halves = split_u16(r_len);
                    leftover_le_buffer[0] = rsp_len_halves[0];
                    leftover_le_buffer[1] = rsp_len_halves[1];
                    Agnostic {
                        p3,
                        expected_response_len: r_len as usize,
                        leftover_lc_len: &[],
                        data_field: &[],
                        leftover_le_len: &leftover_le_buffer[..],
                    }
                }
                ExpectedResponseLength::ExtendedMaximum65536 => {
                    // Note that the presence of a leading 0-byte for the expected response length
                    // is present in the 2E case, but *not* in the 4E case
                    Agnostic {
                        p3: 0,
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
                    len if len <= 255 => Agnostic {
                        p3: len as u8,
                        expected_response_len: 0,
                        leftover_lc_len: &[],
                        data_field: &cmd_field,
                        leftover_le_len: &[],
                    },
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
                        let p3 = 0;
                        let cmd_len_halves = split_u16(len as u16);
                        leftover_lc_buffer[0] = cmd_len_halves[0];
                        leftover_lc_buffer[1] = cmd_len_halves[1];
                        Agnostic {
                            p3,
                            expected_response_len: 0,
                            leftover_lc_len: &leftover_lc_buffer[..],
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
                        let p3 = len as u8;
                        if let ExpectedResponseLength::NonZero(r_len) = i.expected_response_length {
                            match r_len {
                                0 => return Err(ProtocolError::InvalidInterpretation),
                                256 => {
                                    Agnostic {
                                        p3,
                                        expected_response_len: 256,
                                        leftover_lc_len: &[],
                                        data_field: cmd_field,
                                        // '0' means the short maximum, 256
                                        leftover_le_len: &[0],
                                    }
                                }
                                rsp_len if rsp_len < 256 => {
                                    leftover_le_buffer[0] = rsp_len as u8;
                                    Agnostic {
                                        p3,
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
                        let p3 = 0;
                        let cmd_len_halves = split_u16(len as u16);
                        leftover_lc_buffer[0] = cmd_len_halves[0];
                        leftover_lc_buffer[1] = cmd_len_halves[1];
                        match i.expected_response_length {
                            ExpectedResponseLength::None => {
                                return Err(ProtocolError::InvalidInterpretation)
                            }
                            ExpectedResponseLength::NonZero(r_len) => {
                                let rsp_len_halves = split_u16(r_len);
                                leftover_le_buffer[0] = rsp_len_halves[0];
                                leftover_le_buffer[1] = rsp_len_halves[1];
                                Agnostic {
                                    p3,
                                    expected_response_len: r_len as usize,
                                    leftover_lc_len: &leftover_lc_buffer[..],
                                    data_field: cmd_field,
                                    leftover_le_len: &leftover_le_buffer[..],
                                }
                            }
                            ExpectedResponseLength::ExtendedMaximum65536 => {
                                Agnostic {
                                    p3,
                                    expected_response_len: 65_536,
                                    leftover_lc_len: &leftover_lc_buffer[..],
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
    Ok(agnostic)
}

fn run_procedure_byte_loop<C: Connection>(
    connection: &mut C,
    class_byte: u8,
    instruction_bytes: &InstructionBytes<'_>,
    agnostic: &Agnostic,
    response_body_buffer: &mut [u8],
) -> Result<StatusCompleted, ProtocolError> {
    const HEXTY: u8 = 0b0110_0000; // Hex '60'
    let is_not_hexty_and_leading_half_byte_is_six_or_nine =
        |val: u8| ((val >> 4) == 6u8 && (val << 4) != 0u8) || ((val >> 4) == 9u8);

    #[derive(Debug, PartialEq)]
    enum Cursor {
        OnLCBytes(usize),
        OnCmdDataFieldBytes(usize),
        OnLEBytes(usize),
        // Current response-bytes index, is_response_chaining
        OnResponseBytes(usize, bool),
        // Current response-bytes index (for piping through when going from trailer to chaining), is_response_chaining
        OnTrailerBytes(usize, bool),
        Done,
    }
    impl Cursor {
        fn is_chaining(&self) -> bool {
            match self {
                Cursor::OnLCBytes(_) => false,
                Cursor::OnCmdDataFieldBytes(_) => false,
                Cursor::OnLEBytes(_) => false,
                Cursor::OnResponseBytes(_, is_chaining) => *is_chaining,
                Cursor::OnTrailerBytes(_, is_chaining) => *is_chaining,
                Cursor::Done => false,
            }
        }
    }

    let mut cursor = match (
        agnostic.leftover_lc_len.len(),
        agnostic.data_field.len(),
        agnostic.leftover_le_len.len(),
    ) {
        (0, 0, 0) => {
            if agnostic.expected_response_len == 0 {
                Cursor::OnTrailerBytes(0, false)
            } else {
                Cursor::OnResponseBytes(0, false)
            }
        }
        (0, 0, _le) => Cursor::OnLEBytes(0),
        (0, _cdf, _) => Cursor::OnCmdDataFieldBytes(0),
        (_lc, _, _) => Cursor::OnLCBytes(0),
    };

    if cursor == Cursor::Done {
        return Err(ProtocolError::InvalidInterpretation);
    }
    let ack_all_byte = instruction_bytes.instruction;
    let ack_single_byte = instruction_bytes.instruction ^ 0b1111_1111;
    let ack_all_byte_chaining = 0xC0;
    let ack_single_byte_chaining = 0xC0 ^ 0b1111_1111;

    println!("Initial Cursor: {:?}", cursor); // TODO - DEBUG - DELETE
    println!("Ack All Byte: {:?}", ack_all_byte); // TODO - DEBUG - DELETE

    // TODO - we may need to be more lax when there are fewer response bytes supplied than the
    // maximum possible. Alternately, we need to pipe down a parameter that lets us know when
    // we are dealing with an *absolute expected response length* and a *maximum possible with less allowed response length*

    for _ in 0..core::usize::MAX {
        let procedure_byte = connection.receive()?;
        println!(
            "Current Procedure Byte: {} aka {:X} with cursor state: {:?}",
            procedure_byte, procedure_byte, cursor
        ); // TODO - DEBUG - DELETE
        match procedure_byte {
            HEXTY => continue,
            c if is_not_hexty_and_leading_half_byte_is_six_or_nine(c) => {
                if let Cursor::OnTrailerBytes(response_body_bytes_received, _is_chaining) = cursor {
                    let sw1 = c;
                    let sw2 = connection.receive()?;
                    match interpret_sws(sw1, sw2)? {
                        StatusCompleted::NormallyWithBytesRemaining(bytes_remaining) => {
                            // Response chaining, per 7816-4 5.3.4. Send GET RESPONSE command
                            // TODO - If bytes_remaining < 255, should we make a copy of the class and toggle the `is_last` flag? before sending?
                            connection.send(class_byte)?; // CLA
                            connection.send(0xC0)?; // INS
                            connection.send(0x0)?; // P1
                            connection.send(0x0)?; // P2
                            connection.send(bytes_remaining)?; // P3
                            cursor = Cursor::OnResponseBytes(response_body_bytes_received, true)
                        }
                        s => return Ok(s),
                    }
                } else {
                    // We don't think we belong here, e.g., because we still think there are more response bytes to come
                    return Err(ProtocolError::PrematureEndStatusByte(c));
                }
            }
            b if b == ack_all_byte && !cursor.is_chaining()
                || b == ack_all_byte_chaining && cursor.is_chaining() =>
            {
                match cursor {
                    Cursor::OnLCBytes(i) => {
                        send_all(connection, &agnostic.leftover_lc_len[i..])?;
                        send_all(connection, agnostic.data_field)?;
                        send_all(connection, agnostic.leftover_le_len)?;
                        for slot in response_body_buffer
                            .iter_mut()
                            .take(agnostic.expected_response_len)
                        {
                            *slot = connection.receive()?;
                        }
                        // We know we can't be response-chaining because response chaining starts
                        // at the OnResponseBytes state, and OnLCBytes is before that state / inaccessible from that state
                        cursor = Cursor::OnTrailerBytes(agnostic.expected_response_len, false);
                    }
                    Cursor::OnCmdDataFieldBytes(i) => {
                        send_all(connection, &agnostic.data_field[i..])?;
                        send_all(connection, agnostic.leftover_le_len)?;
                        for slot in response_body_buffer
                            .iter_mut()
                            .take(agnostic.expected_response_len)
                        {
                            *slot = connection.receive()?;
                        }
                        // We know we can't be response-chaining because response chaining starts
                        // at the OnResponseBytes state, and OnCmdDataFieldBytes is before that state / inaccessible from that state
                        cursor = Cursor::OnTrailerBytes(agnostic.expected_response_len, false);
                    }
                    Cursor::OnLEBytes(i) => {
                        send_all(connection, &agnostic.leftover_le_len[i..])?;
                        for slot in response_body_buffer
                            .iter_mut()
                            .take(agnostic.expected_response_len)
                        {
                            *slot = connection.receive()?;
                        }
                        // We know we can't be response-chaining because response chaining starts
                        // at the OnResponseBytes state, and OnLEBytes is before that state / inaccessible from that state
                        cursor = Cursor::OnTrailerBytes(agnostic.expected_response_len, false);
                    }
                    Cursor::OnResponseBytes(initial_index, is_chaining) => {
                        for slot in response_body_buffer
                            .iter_mut()
                            .skip(initial_index)
                            .take(agnostic.expected_response_len)
                        {
                            *slot = connection.receive()?;
                        }
                        cursor =
                            Cursor::OnTrailerBytes(agnostic.expected_response_len, is_chaining);
                    }
                    Cursor::OnTrailerBytes(_, _) => continue,
                    Cursor::Done => {
                        // We don't expect to be getting an ack response after thinking we're done
                        return Err(ProtocolError::InvalidInterpretation);
                    }
                }
            }
            b if b == ack_single_byte && !cursor.is_chaining()
                || b == ack_single_byte_chaining && cursor.is_chaining() =>
            {
                match cursor {
                    Cursor::OnLCBytes(i) => {
                        connection.send(agnostic.leftover_lc_len[i])?;
                        cursor = if i + 1 > agnostic.leftover_lc_len.len() {
                            match (agnostic.data_field.len(), agnostic.leftover_le_len.len()) {
                                (0, 0) => {
                                    if agnostic.expected_response_len == 0 {
                                        Cursor::OnTrailerBytes(0, false)
                                    } else {
                                        Cursor::OnResponseBytes(0, false)
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
                        connection.send(agnostic.data_field[i])?;
                        cursor = if i + 1 > agnostic.data_field.len() {
                            if !agnostic.leftover_le_len.is_empty() {
                                Cursor::OnLEBytes(0)
                            } else if agnostic.expected_response_len == 0 {
                                Cursor::OnTrailerBytes(0, false)
                            } else {
                                Cursor::OnResponseBytes(0, false)
                            }
                        } else {
                            Cursor::OnCmdDataFieldBytes(i + 1)
                        };
                    }
                    Cursor::OnLEBytes(i) => {
                        connection.send(agnostic.leftover_le_len[i])?;
                        cursor = if i + 1 > agnostic.leftover_le_len.len() {
                            if agnostic.expected_response_len == 0 {
                                Cursor::OnTrailerBytes(0, false)
                            } else {
                                Cursor::OnResponseBytes(0, false)
                            }
                        } else {
                            Cursor::OnLEBytes(i + 1)
                        };
                    }
                    Cursor::OnResponseBytes(i, is_chaining) => {
                        if i > response_body_buffer.len() {
                            return Err(ProtocolError::InsufficientResponseBuffer);
                        }
                        response_body_buffer[i] = connection.receive()?;
                        cursor = if i + 1 >= agnostic.expected_response_len {
                            Cursor::OnTrailerBytes(i + 1, is_chaining)
                        } else {
                            Cursor::OnResponseBytes(i + 1, is_chaining)
                        }
                    }
                    Cursor::OnTrailerBytes(_, _) => continue,
                    Cursor::Done => {
                        // We don't expect to be getting an ack response after thinking we're done
                        return Err(ProtocolError::InvalidInterpretation);
                    }
                }
            }
            b => return Err(ProtocolError::InvalidProcedureByte(b)),
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

/// Happy path outcome from interpreting the status bytes.
///
/// This comes from -3 12.2.1 Table 14. It uses the status bytes SW1 &
/// SW2 to ascribe a status to a command/response exchange.
#[derive(Debug, PartialEq)]
enum StatusCompleted {
    /// A fixed value of 0x9000.
    Normally,
    /// 0x61XY: The process completed successfully, but the card has
    /// bytes remaining. In cases 1 & 3, the card should not use this
    /// value.
    NormallyWithBytesRemaining(u8),
    /// 0x62XY: The process completed with warning.
    ///
    /// ## Note
    /// The specific meaning of the second byte can found in -4:
    ///
    /// > e.g., '6202' to '6280', GET DATA command for transferring a
    /// > card-originated byte string, see ISO/IEC 7816-4.
    WithWarningA(u8),
    /// 0x63XY: TODO(pittma): No clear meaning as to what this one
    /// means. Maybe in -4?
    WithWarningB(u8),
}

/// Error-indicating outcome from interpreting the status bytes.
///
/// This comes from -3 12.2.1 Table 14. It uses the status bytes SW1 &
/// SW2 to ascribe a status to a command/response exchange.
#[derive(Debug, PartialEq)]
pub enum StatusError {
    /// A fixed value of 0x6700.
    AbortedWithWrongLen,
    /// 0x6CXY: There was an $L\_e$ mismatch. Upon receipt, the
    /// expected response shall contain a P3 equal to the second byte.
    AbortedWithWrongExpectedLen(u8),
    /// The given instruction was invalid, or the card does not
    /// implement it.
    BadOrUnimplementedInstruction,
    /// Invalid Status Bytes - the status bytes SW1 and SW2 did not match an allowed pattern
    InvalidStatusBytes(u16),
}

fn interpret_sws(sw1: u8, sw2: u8) -> Result<StatusCompleted, StatusError> {
    let joined = (u16::from(sw1) << 8) | u16::from(sw2);
    match joined {
        0x9000 => Ok(StatusCompleted::Normally),
        0x6700 => Err(StatusError::AbortedWithWrongLen),
        0x6D00 => Err(StatusError::BadOrUnimplementedInstruction),

        // These cases are intrepreted through masking the first byte:
        //   1. The match is determined by and-ing with the case's
        //      mask. If the given value falls within the range, anding
        //      it with the mask shall produce the mask.
        //   2. By anding the value with the inverse of the mask, we cancel out
        //      the first byte, but the on bits in the second byte are carried
        //      through, leaving a u16 with the value of only the second byte.
        j if joined & 0x6100 == 0x6100 => Ok(StatusCompleted::NormallyWithBytesRemaining(
            (!0x6100 & j) as u8,
        )),
        j if joined & 0x6200 == 0x6200 => Ok(StatusCompleted::WithWarningA((!0x6200 & j) as u8)),
        j if joined & 0x6300 == 0x6300 => Ok(StatusCompleted::WithWarningB((!0x6300 & j) as u8)),
        j if joined & 0x6C00 == 0x6C00 => Err(StatusError::AbortedWithWrongExpectedLen(
            (!0x6C00 & j) as u8,
        )),
        _ => Err(StatusError::InvalidStatusBytes(joined)),
    }
}

#[cfg(test)]
mod test_sws {

    use super::{interpret_sws, StatusCompleted, StatusError};

    #[test]
    fn test_norm() {
        assert_eq!(interpret_sws(0x90, 0), Ok(StatusCompleted::Normally));
    }

    #[test]
    fn test_wrong_len() {
        assert_eq!(
            interpret_sws(0x67, 0),
            Err(StatusError::AbortedWithWrongLen)
        );
    }

    #[test]
    fn test_bad_inst() {
        assert_eq!(
            interpret_sws(0x6D, 0),
            Err(StatusError::BadOrUnimplementedInstruction)
        );
    }

    #[test]
    fn test_bytes_remain() {
        assert_eq!(
            interpret_sws(0x61, 47),
            Ok(StatusCompleted::NormallyWithBytesRemaining(47))
        );
    }

    #[test]
    fn test_wrong_elen() {
        assert_eq!(
            interpret_sws(0x6C, 47),
            Err(StatusError::AbortedWithWrongExpectedLen(47))
        );
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

#[cfg(test)]
mod test_protocol {
    use super::*;
    use crate::{Class, Interindustry, InterindustrySecureMessaging};
    use std::collections::VecDeque;
    use std::vec::Vec;

    struct TinyBufferSource {
        buffer: [u8; 256],
    }
    impl TinyBufferSource {
        fn new() -> Self {
            TinyBufferSource { buffer: [0; 256] }
        }
    }

    impl BufferSource for TinyBufferSource {
        fn request_buffer(&mut self, len: usize) -> Result<&mut [u8], BufferUnavailableError> {
            if len <= 256 {
                Ok(&mut self.buffer[..len])
            } else {
                Err(BufferUnavailableError)
            }
        }
    }

    #[derive(Debug)]
    struct DynamicInstruction<'a> {
        ins: u8,
        p1: u8,
        p2: u8,
        command_data_field: Option<&'a [u8]>,
        expected_response_length: ExpectedResponseLength,
    }

    impl<'a> Instruction for DynamicInstruction<'a> {
        type Response = usize;

        fn to_instruction_bytes(
            &'_ self,
        ) -> Result<InstructionBytes<'_>, CommandSerializationError> {
            Ok(InstructionBytes::<'_> {
                instruction: self.ins,
                parameter_1: self.p1,
                parameter_2: self.p2,
                command_data_field: self.command_data_field,
                expected_response_length: self.expected_response_length,
            })
        }

        fn interpret_response(
            &self,
            instruction_bytes: InstructionBytes,
            response_bytes: &mut [u8],
        ) -> Result<Self::Response, CommandDeserializationError> {
            let mut sum: usize = 0;
            for b in response_bytes {
                sum = sum.saturating_add(usize::from(*b))
            }
            Ok(sum)
        }
    }

    #[derive(Debug)]
    struct PreProgrammedConnection {
        pending_send: VecDeque<u8>,
        received: Vec<u8>,
    }
    impl Connection for PreProgrammedConnection {
        fn send(&mut self, byte: u8) -> Result<(), TransmissionError> {
            self.received.push(byte);
            Ok(())
        }

        fn receive(&mut self) -> Result<u8, TransmissionError> {
            match self.pending_send.pop_front() {
                Some(b) => Ok(b),
                None => Err(TransmissionError),
            }
        }
    }

    #[test]
    pub fn happy_path_no_response_transmit() {
        let c = PreProgrammedConnection {
            pending_send: VecDeque::from(vec![
                // Completed normally SW1 and SW1
                0x90, 0x00,
            ]),
            received: vec![],
        };
        let mut ps = ProtocolState { connection: c };
        let instruction = DynamicInstruction {
            ins: 0xAB,
            p1: 0,
            p2: 0,
            command_data_field: None,
            expected_response_length: ExpectedResponseLength::None,
        };
        let class = Class::Interindustry(
            Interindustry::new(true, InterindustrySecureMessaging::None, 2)
                .expect("Invalid class channel"),
        );
        let command = Command::<DynamicInstruction> { class, instruction };
        let mut buffer_source = TinyBufferSource::new();
        let response = ps
            .transmit_command(&command, &mut buffer_source)
            .expect("We're trying to stay on the happpy path here");
        assert_eq!(0, response);
    }

    #[test]
    pub fn happy_path_short_response_ack_all_transmit() {
        let ins: u8 = 0xAB;
        let c = PreProgrammedConnection {
            pending_send: VecDeque::from(vec![
                // Send INS byte as ACK meaning to send all remaning data
                ins, 1, 2, 3, // Completed normally SW1 and SW1
                0x90, 0x00,
            ]),
            received: vec![],
        };
        let mut ps = ProtocolState { connection: c };
        let instruction = DynamicInstruction {
            ins,
            p1: 0,
            p2: 0,
            command_data_field: None,
            expected_response_length: ExpectedResponseLength::NonZero(3),
        };
        let class = Class::Interindustry(
            Interindustry::new(true, InterindustrySecureMessaging::None, 2)
                .expect("Invalid class channel"),
        );
        let command = Command::<DynamicInstruction> { class, instruction };
        let mut buffer_source = TinyBufferSource::new();
        let result = ps.transmit_command(&command, &mut buffer_source);
        println!("PS: {:?}", ps);
        let response = result.expect("Stay on the sunny side");
        assert_eq!(6, response);
    }

}
