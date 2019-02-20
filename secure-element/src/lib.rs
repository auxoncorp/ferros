//#![no_std] // TODO - RESTORE
// Necessary to use core helpers for splitting unsigned integers into parts
#![feature(int_to_from_bytes)]
#![feature(extern_crate_item_prelude)]

#[cfg(test)]
#[macro_use]
extern crate std;

pub mod interchange;
pub mod protocol;
pub(crate) mod repr;

// TODO - expect to be switching to using a trait that also involves
// dragging meaning out of the response as well
pub struct Command<I: Instruction> {
    class: Class,
    instruction: I,
}

#[derive(Debug, PartialEq)]
pub enum CommandSerializationError {
    /// The command data field content was longer than the maximum supported amount
    TooManyBytesForCommandDataField,
    /// The expected length of the response body was larger than the
    /// maximum size available for a data buffer.
    TooManyBytesRequestedForResponseBody,
}

#[derive(Debug, PartialEq)]
pub enum CommandDeserializationError {
    // TODO - proper naming
    MysteriousDeserializationFailure(&'static str),
}

pub enum CommandSpecificationError {
    ValueOutOfRange(ValueOutOfRange),
}

pub struct ValueOutOfRange {
    pub found_value: u64,
    pub min_value: u64,
    pub max_value: u64,
}

// TODO - Add implementations from the command set in 7816-4 Table 4.1
// as broader support for 7816-4 comes into scope.
pub trait Instruction {
    type Response: Sized;

    fn to_instruction_bytes(&'_ self) -> Result<InstructionBytes<'_>, CommandSerializationError>;

    fn interpret_response(
        &'_ self,
        instruction_bytes: InstructionBytes<'_>,
        // Might also need SW1/SW2
        response_bytes: &mut [u8],
    ) -> Result<Self::Response, CommandDeserializationError>;
}

pub struct BufferUnavailableError;

pub trait BufferSource {
    fn request_buffer(&mut self, len: usize) -> Result<&mut [u8], BufferUnavailableError>;
}

/// Serialization-oriented representation of the instruction-specific portions of a Command Header [INS, P1, P2, and maybe P3],
/// as well as the associated command data bytes
pub struct InstructionBytes<'a> {
    /// INS
    instruction: u8,
    /// P1: Has meaning only in context of INS
    parameter_1: u8,
    /// P2: Has meaning only in context of INS
    parameter_2: u8,
    /// P3: Encodes the number of data bytes to be transferred during the command
    /// TODO - Confirm -3 vs -4 differences in interpreting  P3/Lc fields
    /// For example, -4 treats this field as optional and uses absence to encode 0 data length,
    /// while -3 for T=0 makes no mention of an absence based option.
    /// In T=0, in an outgoing transfer command, P3='00' introduces a 256-byte transfer *from* the card
    /// In T=0, in an incoming transfer command, P3='00' introduces no data transfer
    /// Supports short-length data transfers only
    command_data_field: Option<&'a [u8]>,

    /// Le. Length expected for the response data, specified separate from the response data field slice length
    /// because... probably... sometimes... they diverge from direct interpretation. E.G. 0 has specialized meaning..
    expected_response_length: ExpectedResponseLength,
    // TODO - resolve
    //response_data_field: Option<&'a mut [u8]>
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExpectedResponseLength {
    None,
    // TODO - enforce non-zero contents
    NonZero(u16),
    ExtendedMaximum65536,
}

/// Secure messaging indication for command classes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterindustrySecureMessaging {
    /// No secure messaging or no indication
    None,
    /// Proprietary secure messaging format
    Proprietary,
    /// Secure messaging according to clause 10,
    /// but command header *not* processed according to 10.2.3.1
    NotAuthenticated,
    /// Secure messaging according to clause 10,
    /// *and* command header processed according to 10.2.3.1
    Authenticated,
}

/// Secure messaging indication for command classes
/// when a limited amount of options are expressable
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterindustryExtendedSecureMessaging {
    /// No secure messaging or no indication
    None,
    /// Secure messaging according to clause 10,
    /// but command header *not* processed according to 10.2.3.1
    NotAuthenticated,
}

/// Application-level representation of the data encoded in
/// the Command Header's CLA byte
#[derive(Debug, PartialEq)]
pub enum Class {
    Proprietary,
    Interindustry(Interindustry),
    InterindustryExtended(InterindustryExtended),
}

#[derive(Debug, PartialEq)]
pub struct Interindustry {
    // The command is the last or only command of a chain
    is_last: bool,
    secure_messaging: InterindustrySecureMessaging,
    // Logical channel number from zero to three
    channel: u8,
}

/// The channel selected was out of bounds
/// For Interindustry classes, 0 <= channel <= 3
/// For Interindustry extended classes, 4 <= channel <= 19
#[derive(Debug, PartialEq)]
pub struct InvalidChannelError;

impl Interindustry {
    /// channel must be between 0 and 3, inclusive.
    pub fn new(
        is_last: bool,
        secure_messaging: InterindustrySecureMessaging,
        channel: u8,
    ) -> Result<Self, InvalidChannelError> {
        if channel > 3 {
            return Err(InvalidChannelError);
        }
        Ok(Interindustry {
            is_last,
            secure_messaging,
            channel,
        })
    }

    /// Convert to single-byte CLA representation according to ISO 7816-3 and 7816-4
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0b0000_0000;
        if self.is_last {
            byte |= 0b0001_0000;
        }
        match self.secure_messaging {
            InterindustrySecureMessaging::None => {}
            InterindustrySecureMessaging::Proprietary => {
                byte |= 0b0000_0100;
            }
            InterindustrySecureMessaging::NotAuthenticated => {
                byte |= 0b0000_1000;
            }
            InterindustrySecureMessaging::Authenticated => {
                byte |= 0b0000_1100;
            }
        }
        // Because channel is 0..=3 as maintained by the constructor,
        // we think it fits in the last two bits and does not
        // interfere with any other bits. Clear out other bits just to be safe.
        let channel = self.channel & 0b0000_0011;
        byte |= channel;
        byte
    }
}

#[derive(Debug, PartialEq)]
pub struct InterindustryExtended {
    // The command is the last or only command of a chain
    is_last: bool,
    secure_messaging: InterindustryExtendedSecureMessaging,
    // Logical channel number from four to nineteen, inclusive
    channel: u8,
}
impl InterindustryExtended {
    /// channel must be between 4 and 19, inclusive.
    pub fn new(
        is_last: bool,
        secure_messaging: InterindustryExtendedSecureMessaging,
        channel: u8,
    ) -> Result<Self, InvalidChannelError> {
        if channel < 4 || channel > 19 {
            return Err(InvalidChannelError);
        }
        Ok(InterindustryExtended {
            is_last,
            secure_messaging,
            channel,
        })
    }

    /// Convert to single-byte CLA representation according to ISO 7816-3 and 7816-4
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0b0100_0000;
        match self.secure_messaging {
            InterindustryExtendedSecureMessaging::None => {}
            InterindustryExtendedSecureMessaging::NotAuthenticated => {
                byte |= 0b0010_0000;
            }
        }
        if self.is_last {
            byte |= 0b0001_0000;
        }
        // Offset channel by -4 because in this mode, counting starts at 4,
        // and we want to fit into only 4 bits.
        let channel = self.channel - 4u8;
        // Because channel is between 4 and 19 (as maintained by the constructor),
        // its range of options can be represented by 4 bits.
        // Clear out the remaining bits just to be safe.
        let channel = channel & 0b0000_1111;
        byte |= channel;
        byte
    }
}

impl Class {
    pub fn to_byte(&self) -> u8 {
        match self {
            Class::Proprietary => 0b1000_0000,
            Class::Interindustry(i) => i.to_byte(),
            Class::InterindustryExtended(i) => i.to_byte(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::hash_set::HashSet;

    #[test]
    fn interindustry_to_byte_channel_encoded() {
        for c in 0..=3 {
            let i: Interindustry = Interindustry::new(false, InterindustrySecureMessaging::None, c)
                .expect("should selected a valid channel in 0..=3");
            assert_eq!(c, extract_last_two_bits(i.to_byte()));
        }
    }

    #[test]
    fn interindustry_invalid_channel() {
        let r = Interindustry::new(false, InterindustrySecureMessaging::None, 4);
        assert_eq!(Err(InvalidChannelError), r);
    }

    fn extract_last_two_bits(a: u8) -> u8 {
        let b = a << 6;
        b >> 6
    }

    fn extract_last_four_bits(a: u8) -> u8 {
        let b = a << 4;
        b >> 4
    }
    const INVALID_CLASS_BYTE: u8 = 0xFF;

    #[test]
    fn exhaustive_interindustry_to_byte_valid_and_distinct() {
        let mut extant_values = HashSet::new();

        fn assert_valid_class_byte(
            extant_values: &mut HashSet<u8>,
            is_last: bool,
            secure_messaging: InterindustrySecureMessaging,
            channel: u8,
        ) {
            let i: Interindustry =
                Interindustry::new(is_last, secure_messaging, channel).expect("Invalid channel");
            assert!(extant_values.insert(i.to_byte()));
            assert_ne!(INVALID_CLASS_BYTE, i.to_byte());
            assert_eq!(
                channel,
                extract_last_two_bits(i.to_byte()),
                "Extracted channel should match input channel"
            );
        }

        for is_last in [true, false].iter() {
            for ism in [
                InterindustrySecureMessaging::None,
                InterindustrySecureMessaging::Authenticated,
                InterindustrySecureMessaging::NotAuthenticated,
                InterindustrySecureMessaging::Proprietary,
            ]
            .iter()
            {
                for channel in 0..=3 {
                    assert_valid_class_byte(&mut extant_values, *is_last, *ism, channel);
                }
            }
        }
    }

    #[test]
    fn exhaustive_interindustry_extended_to_byte_valid_and_distinct() {
        let mut extant_values = HashSet::new();

        fn assert_valid_class_byte(
            extant_values: &mut HashSet<u8>,
            is_last: bool,
            secure_messaging: InterindustryExtendedSecureMessaging,
            channel: u8,
        ) {
            let i: InterindustryExtended =
                InterindustryExtended::new(is_last, secure_messaging, channel)
                    .expect("Invalid channel");
            assert!(
                extant_values.insert(i.to_byte()),
                "Should not add duplicate classes"
            );
            assert_ne!(
                INVALID_CLASS_BYTE,
                i.to_byte(),
                "CLA should not match the explicitly forbidden value"
            );
            assert_eq!(
                channel,
                extract_last_four_bits(i.to_byte()) + 4,
                "Extracted channel should match input channel"
            ); // Add 4 to account for encoded offset
        }

        for is_last in [true, false].iter() {
            for ism in [
                InterindustryExtendedSecureMessaging::None,
                InterindustryExtendedSecureMessaging::NotAuthenticated,
            ]
            .iter()
            {
                for channel in 4..=19 {
                    assert_valid_class_byte(&mut extant_values, *is_last, *ism, channel);
                }
            }
        }
    }

}
