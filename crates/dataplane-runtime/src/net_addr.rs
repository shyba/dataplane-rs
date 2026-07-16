//! Wire-friendly network address, independent of any NIF or transport type.

/// Network address in wire-friendly form.
#[derive(Clone, Debug)]
pub enum NetAddr {
    V4([u8; 4], u16),
    V6([u16; 8], u16),
}
