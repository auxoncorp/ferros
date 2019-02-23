trait AsByte {
    fn as_byte(&self) -> u8;
}

enum ProtocolError {
    Broken,
}

struct CommandHeader<C: AsByte, I: AsByte> {
    class: C,
    instruction: I,
    p1: u8,
    p2: u8,
    p3: Option<u8>,
    extended_lc: Option<u16>,
}

impl<C: AsByte, I: AsByte> CommandHeader<C, I> {
    fn data_len(&self) -> Option<u16> {
        if self.p3.is_some() {
            return self.p3 as u16;
        }
        if self.extended_lc.is_some() {
            return self.extended_lc;
        }
        None
    }
}

struct Command<'data, C: AsByte, I: AsByte> {
    header: CommandHeader<C, I>,
    expected_len: Option<u16>,
    data: Option<&'data [u8]>,
}

trait Connection {
    fn send(&self, byte: u8) -> Result<(), ProtocolError>;
    fn receive(&self) -> Result<u8, ProtocolError>;

    fn send_command_header<C: AsByte, I: AsByte>(
        &self,
        header: CommandHeader<C, I>,
    ) -> Result<(), ProtocolError> {
        self.send(header.class.as_bytes())?;
        self.send(header.instruction.as_bytes())?;
        self.send(header.p1)?;
        self.send(header.p2)?;
        self.send(header.p3)?;
    }
}

/// TransmitCursor is a session-type shaped progression which a
/// command transmission must abide.
enum TransmitCursor {
    Header,
    CommandLen(u16),
    Data((u16, u16)),
    ExpectedLen(u16),
    ResponseData((u16, u16)),
    Done,
}

impl TransmitCursor {
    fn step<C: AsByte, I: AsByte>(&self, cmd: &Command<C, I>) -> Result<Self, ProtocolError> {
        match *self {
            TransmitCursor::Header => {
                if cmd.header.p3.is_some() {
                    return Ok(TransmitCursor::Data(cmd.data.map(|d| (0, d.len() as u16))));
                } else {
                    if let Some(ext) = cmd.header.extended_lc {
                        return Ok(TransmitCursor::CommandLen(ext));
                    }
                    return Err(ProtocolError::Broken);
                }
            }
            TransmitCursor::CommandLen(_) => {
                Ok(TransmitCursor::Data(cmd.data.map(|d| (0, d.len() as u16))))
            }
            TransmitCursor::Data(Some((sent, total))) => {
                if sent == total {
                    if let Some(el) = cmd.expected_len {
                        return Ok(TransmitCursor::ExpectedLen(el));
                    }
                    return Ok(TransmitCursor::Done);
                }
                return Ok(TransmitCursor::Data(Some((sent + 1, total))));
            }
            TransmitCursor::Done => unreachable!(),
        }
    }
}

// TODO: Accumulate response bytes according to $L\_e$.
fn transmit_command<'cmd_data, 'resp_data, Conn: Connection, C: AsByte, I: AsByte>(
    conn: Conn,
    cmd: Command<'cmd_data, C, I>,
) -> Result<Status, ProtocolError> {
    // -3, 10.3.3, Table 11: '6X' (/= '60'), '9X
    //
    // NB: `0x9F` is the inverse of the 6X mask. The 9 cancels out
    // the higher 4 bits while the F carries forward any "on" bits in
    // the lower half. If we get 0, then none of the lower bits have
    // been set, indicating this is not an SW1 byte.
    let is_sw1 = |byte| byte == 0x90 || (byte & 0x9F != 0);

    let mut cursor = TransmitCursor::Header;

    conn.send_header(cmd.header)?;

    cursor = cursor.step(&cmd);

    loop {
        let mut procedure = conn.receive()?;
        match procedure {
            // The NULL case. Await the next byte.
            0x60 => continue,

            // The first half of a status has arrived. Wait for the
            // second half, then break.
            b if is_sw1(b) => {
                let second_sw = conn.receive()?;
                let status = interpret_sws(b, second_sw)?;
                return Ok(status);
            }

            // We've received the instruction byte back from the card
            // indicating we ought to send the remainder of the
            // command data.
            b if b == cmd.header.instruction.as_byte() => match cursor {
                TransmitCursor::CommandLen(val) => {
                    let (higher, lower) = split_bytes(val);
                    conn.send(higher)?;
                    conn.send(lower)?;
                    cursor.step(&cmd)?;
                }
                TransmitCursor::Data((sent, total)) => {
                    conn.send(cmd.data[sent]);
                    cursor = cursor.step(&cmd)?;
                    loop {
                        match cursor {
                            TransmitCursor::Data((sent, total)) => {
                                conn.send(cmd.data[sent]);
                                cursor = cursor.step(&cmd)?;
                            }
                            TransmitCursor::ExpectedLen(el) => {
                                let (higher, lower) = split_bytes(el);
                                conn.send(higher)?;
                                conn.send(lower)?;
                                cursor.step(&cmd)?;
                            }
                            TransmitCursor::Done => break,
                        }
                    }
                }
            },

            // We've received the instruction byte XORed with `0xFF`
            // indicating we ought to send the next data byte, then
            // await the next procedure byte.
            b if b == (cmd.header.instruction.as_byte() ^ 0xFF) => match cursor {
                TransmitCursor::CommandLen(val) => {
                    let (higher, lower) = split_bytes(val);
                    conn.send(higher)?;
                    // TODO: this isn't right only send one, mark it
                    conn.send(lower)?;
                    cursor.step(&cmd)?;
                }
                TransmitCursor::Data((sent, total)) => {
                    conn.send(cmd.data[sent]);
                    cursor = cursor.step(&cmd)?;
                }
                TransmitCursor::Done => continue,
            },
            _ => return ProtocolError::Broken,
        }
    }
}

// Happy path outcome from interpreting the status bytes.
///
/// This comes from -3 12.2.1 Table 14. It uses the status bytes SW1 &
/// SW2 to ascribe a status to a command/response exchange.
#[derive(Debug, PartialEq)]
pub enum Status {
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

fn interpret_sws(sw1: u8, sw2: u8) -> Result<Status, Status> {
    let joined = (u16::from(sw1) << 8) | u16::from(sw2);
    match joined {
        0x9000 => Ok(Status::Normally),
        0x6700 => Err(Status::AbortedWithWrongLen),
        0x6D00 => Err(Status::BadOrUnimplementedInstruction),

        // These cases are intrepreted through masking the first byte:
        //   1. The match is determined by and-ing with the case's
        //      mask. If the given value falls within the range, anding
        //      it with the mask shall produce the mask.
        //   2. By anding the value with the inverse of the mask, we cancel out
        //      the first byte, but the on bits in the second byte are carried
        //      through, leaving a u16 with the value of only the second byte.
        j if joined & 0x6100 == 0x6100 => {
            Ok(Status::NormallyWithBytesRemaining((!0x6100 & j) as u8))
        }
        j if joined & 0x6200 == 0x6200 => Ok(Status::WithWarningA((!0x6200 & j) as u8)),
        j if joined & 0x6300 == 0x6300 => Ok(Status::WithWarningB((!0x6300 & j) as u8)),
        j if joined & 0x6C00 == 0x6C00 => {
            Err(Status::AbortedWithWrongExpectedLen((!0x6C00 & j) as u8))
        }
        _ => Err(Status::InvalidStatusBytes(joined)),
    }
}

#[cfg(test)]
mod test_protocol {
    use super::*;
    use crate::{BufferSource, Class, Interindustry, InterindustrySecureMessaging};
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
        let serialized = serialize(&command).expect("Serialization troubles");
        let mut buffer_source = TinyBufferSource::new();
        let resp_size = serialized.expected_response_len();
        let response_buffer = buffer_source
            .request_buffer(resp_size)
            .expect("Unable to get a response buffer");
        let (output_buffer, status) = ps
            .transmit_command(&command, response_buffer)
            .expect("We're trying to stay on the happy path here");
        assert!(output_buffer.is_empty());
        assert_eq!(StatusCompleted::Normally, status);
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
        let serialized = serialize(&command).expect("Serialization troubles");
        let mut buffer_source = TinyBufferSource::new();
        // TODO - get real response size from instruction
        let resp_size = serialized.expected_response_len();
        let response_buffer = buffer_source
            .request_buffer(resp_size)
            .expect("Unable to get a response buffer");
        let result = ps.transmit_serialized_command(&serialized, response_buffer);
        println!("PS: {:?}", ps);
        let (output_buffer, status) = result.expect("Stay on the sunny side");
        assert_eq!(StatusCompleted::Normally, status);
        assert_eq!(6u8, output_buffer.into_iter().sum());
    }

}
