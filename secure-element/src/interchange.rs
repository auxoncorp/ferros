/// Commands relating to data interchange,
/// as defined in 7816-4 (2013) section 11
use super::{
    CommandSerializationError, CommandSpecificationError, ExpectedResponseLength, Instruction,
    InstructionBytes, ValueOutOfRange,
};
use crate::repr::split_u16;

/// The READ BINARY command, see section 11.2.2 and 11.2.3 of 7816-4,
/// Has 'B0' and 'B1' INS options, with further... interesting... subvariants
pub enum ReadBinary {
    ReadCurrentEF(ReadCurrentEF),
    ReadShortIdentifierEF,
    ReadShortIdentifierWithDataObjectOffset,
    ReadFileIdentifierWithDataObjectOffset,
    ReadCurrentEFWithDataObjectOffset,
}

impl Instruction for ReadBinary {
    fn to_instruction_bytes(&'_ self) -> Result<InstructionBytes<'_>, CommandSerializationError> {
        match self {
            ReadBinary::ReadCurrentEF(c) => c.to_instruction_bytes(),
            ReadBinary::ReadShortIdentifierEF => unimplemented!(),
            ReadBinary::ReadShortIdentifierWithDataObjectOffset => unimplemented!(),
            ReadBinary::ReadFileIdentifierWithDataObjectOffset => unimplemented!(),
            ReadBinary::ReadCurrentEFWithDataObjectOffset => unimplemented!(),
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
                    found_value: u64::from(current_ef_offset),
                    min_value: 0,
                    max_value: u64::from(MAX_OFFSET),
                },
            ))
        } else {
            Ok(ReadCurrentEF {
                current_ef_offset,
                //buffer: [0u8; 256] ,
            })
        }
    }
}

impl Instruction for ReadCurrentEF {
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
}
