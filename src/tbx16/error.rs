use std::fmt;

use crate::tbx16::address::Address;
use crate::tbx16::cell::Cell;

/// Trap and configuration errors for the tbx16 execution substrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tbx16Error {
    DataStackUnderflow,
    DataStackOverflow,
    ReturnStackUnderflow,
    ReturnStackOverflow,
    InvalidReturnCount {
        count: Cell,
    },
    StepLimitExceeded,
    InvalidMemoryAccess {
        addr: Address,
        operation: &'static str,
    },
    InvalidStackRegion {
        start: Address,
        end: Address,
        reason: &'static str,
    },
    MisalignedStackPointer {
        stack: &'static str,
        addr: Address,
    },
    DivisionByZero,
    InvalidExecutionToken {
        xt: Cell,
    },
    InvalidExecutionState,
    DirtyExecutionState,
    InstructionPointerOutOfRange {
        ip: Address,
    },
    ExplicitTrap {
        code: Cell,
    },
}

impl fmt::Display for Tbx16Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tbx16Error::DataStackUnderflow => write!(f, "data stack underflow"),
            Tbx16Error::DataStackOverflow => write!(f, "data stack overflow"),
            Tbx16Error::ReturnStackUnderflow => write!(f, "return stack underflow"),
            Tbx16Error::ReturnStackOverflow => write!(f, "return stack overflow"),
            Tbx16Error::InvalidReturnCount { count } => {
                write!(f, "invalid return count ${:04x}", count.raw())
            }
            Tbx16Error::StepLimitExceeded => write!(f, "step limit exceeded"),
            Tbx16Error::InvalidMemoryAccess { addr, operation } => {
                write!(f, "invalid memory access during {operation} at {addr}")
            }
            Tbx16Error::InvalidStackRegion { start, end, reason } => {
                write!(f, "invalid stack region {start}..{end}: {reason}")
            }
            Tbx16Error::MisalignedStackPointer { stack, addr } => {
                write!(f, "misaligned {stack} stack pointer at {addr}")
            }
            Tbx16Error::DivisionByZero => write!(f, "division by zero"),
            Tbx16Error::InvalidExecutionToken { xt } => {
                write!(f, "invalid execution token ${:04x}", xt.raw())
            }
            Tbx16Error::InvalidExecutionState => write!(f, "invalid execution state"),
            Tbx16Error::DirtyExecutionState => write!(f, "dirty execution state"),
            Tbx16Error::InstructionPointerOutOfRange { ip } => {
                write!(f, "instruction pointer out of range at {ip}")
            }
            Tbx16Error::ExplicitTrap { code } => {
                write!(f, "explicit trap ${:04x}", code.raw())
            }
        }
    }
}

impl std::error::Error for Tbx16Error {}
