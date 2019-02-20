//#![no_std] // TODO - RESTORE
// Necessary to use core helpers for splitting unsigned integers into parts
#![feature(int_to_from_bytes)]
#![feature(extern_crate_item_prelude)]

extern crate typenum;

#[cfg(test)]
#[macro_use]
extern crate std;

pub mod interchange;
pub mod protocol;
pub(crate) mod repr;

use core::marker::PhantomData;

use typenum::consts::{True, U0, U19, U3, U4};
use typenum::{IsGreaterOrEqual, IsLessOrEqual, Unsigned};

// TODO - expect to be switching to using a trait that also involves
// dragging meaning out of the response as well
pub struct Command<I: Instruction, ClassChannel: Unsigned, ClassChannelExtended: Unsigned>
where
    ClassChannel: IsLessOrEqual<U3, Output = True>,
    ClassChannelExtended: IsLessOrEqual<U19, Output = True>,
    ClassChannelExtended: IsGreaterOrEqual<U4, Output = True>,
{
    class: Class<ClassChannel, ClassChannelExtended>,
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
pub enum Class<Channel: Unsigned, ChannelExtended: Unsigned>
where
    Channel: IsLessOrEqual<U3, Output = True>,
    ChannelExtended: IsLessOrEqual<U19, Output = True>,
    ChannelExtended: IsGreaterOrEqual<U4, Output = True>,
{
    Proprietary,
    Interindustry(Interindustry<Channel>),
    InterindustryExtended(InterindustryExtended<ChannelExtended>),
}

pub struct Interindustry<Channel: Unsigned>
where
    Channel: IsLessOrEqual<U3, Output = True>,
{
    // The command is the last or only command of a chain
    is_last: bool,
    secure_messaging: InterindustrySecureMessaging,
    // Logical channel number from zero to three
    _channel: PhantomData<Channel>,
}

pub struct InterindustryExtended<Channel: Unsigned>
where
    Channel: IsLessOrEqual<U19, Output = True>,
    Channel: IsGreaterOrEqual<U4, Output = True>,
{
    // The command is the last or only command of a chain
    is_last: bool,
    secure_messaging: InterindustryExtendedSecureMessaging,
    // Logical channel number from four to nineteen
    _channel: PhantomData<Channel>,
}

impl<Channel: Unsigned, ChannelExtended: Unsigned> Class<Channel, ChannelExtended>
where
    Channel: IsLessOrEqual<U3, Output = True>,
    ChannelExtended: IsLessOrEqual<U19, Output = True>,
    ChannelExtended: IsGreaterOrEqual<U4, Output = True>,
{
    pub fn to_byte(&self) -> u8 {
        match self {
            Class::Proprietary => 0b1000_0000,
            Class::Interindustry(i) => i.to_byte(),
            Class::InterindustryExtended(i) => i.to_byte(),
        }
    }
}

impl<Channel: Unsigned> Interindustry<Channel>
where
    Channel: IsLessOrEqual<U3, Output = True>,
{
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
        byte |= Channel::U8;
        byte
    }
}

impl<ChannelExtended: Unsigned> InterindustryExtended<ChannelExtended>
where
    ChannelExtended: IsLessOrEqual<U19, Output = True>,
    ChannelExtended: IsGreaterOrEqual<U4, Output = True>,
{
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
        // Offset channel by 4 to allow fitting into 4 bits
        let channel = ChannelExtended::U8 - 4u8;
        byte |= channel;
        byte
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use std::collections::hash_set::HashSet;
    use typenum::{
        U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U3, U4, U5, U6, U7, U8, U9,
    };

    #[test]
    fn interindustry_to_byte_channel_encoded() {
        let i0: Interindustry<U0> = Interindustry {
            is_last: false,
            secure_messaging: InterindustrySecureMessaging::None,
            _channel: PhantomData,
        };
        assert_eq!(0, extract_last_two_bits(i0.to_byte()));

        let i1: Interindustry<U1> = Interindustry {
            is_last: false,
            secure_messaging: InterindustrySecureMessaging::None,
            _channel: PhantomData,
        };
        assert_eq!(1, extract_last_two_bits(i1.to_byte()));

        let i2: Interindustry<U2> = Interindustry {
            is_last: false,
            secure_messaging: InterindustrySecureMessaging::None,
            _channel: PhantomData,
        };
        assert_eq!(2, extract_last_two_bits(i2.to_byte()));

        let i3: Interindustry<U3> = Interindustry {
            is_last: false,
            secure_messaging: InterindustrySecureMessaging::None,
            _channel: PhantomData,
        };
        assert_eq!(3, extract_last_two_bits(i3.to_byte()));
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

        fn assert_valid_class_byte<C: Unsigned>(
            extant_values: &mut HashSet<u8>,
            is_last: bool,
            secure_messaging: InterindustrySecureMessaging,
        ) where
            C: IsLessOrEqual<U3, Output = True>,
        {
            let i: Interindustry<C> = Interindustry {
                is_last: is_last,
                secure_messaging: secure_messaging,
                _channel: PhantomData,
            };
            assert!(extant_values.insert(i.to_byte()));
            assert_ne!(INVALID_CLASS_BYTE, i.to_byte());
            assert_eq!(C::U8, extract_last_two_bits(i.to_byte()));
        }

        for b in [true, false].iter() {
            for ism in [
                InterindustrySecureMessaging::None,
                InterindustrySecureMessaging::Authenticated,
                InterindustrySecureMessaging::NotAuthenticated,
                InterindustrySecureMessaging::Proprietary,
            ]
            .iter()
            {
                assert_valid_class_byte::<U0>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U1>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U2>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U3>(&mut extant_values, *b, *ism);
            }
        }
    }

    #[test]
    fn exhaustive_interindustry_extended_to_byte_valid_and_distinct() {
        let mut extant_values = HashSet::new();

        fn assert_valid_class_byte<C: Unsigned>(
            extant_values: &mut HashSet<u8>,
            is_last: bool,
            secure_messaging: InterindustryExtendedSecureMessaging,
        ) where
            C: IsLessOrEqual<U19, Output = True>,
            C: IsGreaterOrEqual<U4, Output = True>,
        {
            let i: InterindustryExtended<C> = InterindustryExtended {
                is_last: is_last,
                secure_messaging: secure_messaging,
                _channel: PhantomData,
            };
            assert!(extant_values.insert(i.to_byte()));
            assert_ne!(INVALID_CLASS_BYTE, i.to_byte());
            assert_eq!(C::U8, extract_last_four_bits(i.to_byte()) + 4); // Add 4 to account for encoded offset
        }

        for b in [true, false].iter() {
            for ism in [
                InterindustryExtendedSecureMessaging::None,
                InterindustryExtendedSecureMessaging::NotAuthenticated,
            ]
            .iter()
            {
                assert_valid_class_byte::<U4>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U5>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U6>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U7>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U8>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U9>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U10>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U11>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U12>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U13>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U14>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U15>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U16>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U17>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U18>(&mut extant_values, *b, *ism);
                assert_valid_class_byte::<U19>(&mut extant_values, *b, *ism);
            }
        }
    }

}
