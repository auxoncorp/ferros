/// Commands relating to data interchange,
/// as defined in 7816-4 (2013) section 11
use super::{
    CommandSerializationError, CommandSpecificationError, Instruction, InstructionBytes,
    ValueOutOfRange,
};
use crate::repr::split_u16;
use crate::ExpectedResponseLength;

/// The READ BINARY command, see section 11.2.2 and 11.2.3 of 7816-4,
/// Has 'B0' and 'B1' INS options, with further... interesting... subvariants
pub enum ReadBinary {
    ReadCurrentEF(ReadCurrentEF),
    ReadShortIdentifierEF(ReadShortIdentifierEF),
    ReadShortIdentifierWithDataObjectOffset(ReadShortIdentifierWithDataObjectOffset),
    ReadFileIdentifierWithDataObjectOffset(ReadFileIdentifierWithDataObjectOffset),
    ReadCurrentEFWithDataObjectOffset(ReadCurrentEFWithDataObjectOffset),
}

impl Instruction for ReadBinary {
    type Response = ();

    fn to_instruction_bytes(&'_ self) -> Result<InstructionBytes<'_>, CommandSerializationError> {
        match self {
            ReadBinary::ReadCurrentEF(c) => c.to_instruction_bytes(),
            ReadBinary::ReadShortIdentifierEF(c) => c.to_instruction_bytes(),
            ReadBinary::ReadShortIdentifierWithDataObjectOffset(c) => c.to_instruction_bytes(),
            ReadBinary::ReadFileIdentifierWithDataObjectOffset(c) => c.to_instruction_bytes(),
            ReadBinary::ReadCurrentEFWithDataObjectOffset(c) => c.to_instruction_bytes(),
        }
    }

    fn interpret_response(
        &self,
        instruction_bytes: InstructionBytes,
        response_bytes: &[u8],
    ) -> Self::Response {
        match self {
            ReadBinary::ReadCurrentEF(c) => c.interpret_response(instruction_bytes, response_bytes),
            ReadBinary::ReadShortIdentifierEF(c) => {
                c.interpret_response(instruction_bytes, response_bytes)
            }
            ReadBinary::ReadShortIdentifierWithDataObjectOffset(c) => {
                c.interpret_response(instruction_bytes, response_bytes)
            }
            ReadBinary::ReadFileIdentifierWithDataObjectOffset(c) => {
                c.interpret_response(instruction_bytes, response_bytes)
            }
            ReadBinary::ReadCurrentEFWithDataObjectOffset(c) => {
                c.interpret_response(instruction_bytes, response_bytes)
            }
        }
    }
}

/// Read Binary command variant for reading from the current EF
pub struct ReadCurrentEF {
    // Actually restricted to 15 bits
    current_ef_offset: u16,
    // TODO - Pipe expected length up to here?
}

impl ReadCurrentEF {
    pub fn new(current_ef_offset: u16) -> Result<Self, CommandSpecificationError> {
        const MAX_OFFSET: u16 = 0b0111_1111_1111_1111;
        if current_ef_offset > MAX_OFFSET {
            Err(CommandSpecificationError::ValueOutOfRange(
                ValueOutOfRange {
                    found_value: current_ef_offset as u64,
                    min_value: 0,
                    max_value: MAX_OFFSET as u64,
                },
            ))
        } else {
            Ok(ReadCurrentEF { current_ef_offset })
        }
    }

    // TODO - CLEANUP
    //pub fn current_ef_offset(&self) -> u16 {
    //    self.current_ef_offset
    //}
}

impl Instruction for ReadCurrentEF {
    type Response = ();

    fn to_instruction_bytes(&self) -> Result<InstructionBytes, CommandSerializationError> {
        // B0 and bit b8 of P1 is 0, then the rest of P1-P2 encodes a 15-bit offset in the current EF
        let offset_halves = split_u16(self.current_ef_offset);
        Ok(InstructionBytes {
            instruction: 0b1011_0000,
            parameter_1: offset_halves[0],
            parameter_2: offset_halves[1],
            command_data_field: None,
            expected_response_length: ExpectedResponseLength::ExtendedMaximum65536,
        })
    }

    fn interpret_response(
        &self,
        instruction_bytes: InstructionBytes,
        response_bytes: &[u8],
    ) -> Self::Response {
        unimplemented!()
    }
}

pub struct ReadShortIdentifierEF {}
impl Instruction for ReadShortIdentifierEF {
    type Response = ();

    fn to_instruction_bytes(&self) -> Result<InstructionBytes, CommandSerializationError> {
        unimplemented!()
    }

    fn interpret_response(
        &self,
        _instruction_bytes: InstructionBytes,
        _response_bytes: &[u8],
    ) -> Self::Response {
        unimplemented!()
    }
}

pub struct ReadShortIdentifierWithDataObjectOffset {}
impl Instruction for ReadShortIdentifierWithDataObjectOffset {
    type Response = ();

    fn to_instruction_bytes(&self) -> Result<InstructionBytes, CommandSerializationError> {
        unimplemented!()
    }

    fn interpret_response(
        &self,
        _instruction_bytes: InstructionBytes,
        _response_bytes: &[u8],
    ) -> Self::Response {
        unimplemented!()
    }
}

pub struct ReadFileIdentifierWithDataObjectOffset {}
impl Instruction for ReadFileIdentifierWithDataObjectOffset {
    type Response = ();

    fn to_instruction_bytes(&self) -> Result<InstructionBytes, CommandSerializationError> {
        unimplemented!()
    }

    fn interpret_response(
        &self,
        _instruction_bytes: InstructionBytes,
        _response_bytes: &[u8],
    ) -> Self::Response {
        unimplemented!()
    }
}

pub struct ReadCurrentEFWithDataObjectOffset {}
impl Instruction for ReadCurrentEFWithDataObjectOffset {
    type Response = ();

    fn to_instruction_bytes(&self) -> Result<InstructionBytes, CommandSerializationError> {
        unimplemented!()
    }

    fn interpret_response(
        &self,
        _instruction_bytes: InstructionBytes,
        _response_bytes: &[u8],
    ) -> Self::Response {
        unimplemented!()
    }
}
