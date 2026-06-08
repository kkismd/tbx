use crate::tbx16::address::Address;

/// Byte-addressed VM registers for tbx16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    pub ip: Option<Address>,
    pub dsp: Address,
    pub rsp: Address,
    pub bp: Address,
    pub w: Option<Address>,
}
