/// Utilities relating to data representation

pub fn split_u16(u: u16) -> [u8; 2] {
    // TODO - we may eventually need to pipe through endianness configuration
    u.to_ne_bytes()
}
