/// Session-owned pseudo-random state.
///
/// The generator is deliberately an implementation detail. Its public
/// language contract is only the positive inclusive upper-bound operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RandomState {
    state: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RandomError {
    InvalidUpperBound { upper_bound: i16 },
}

impl RandomState {
    pub(crate) const fn seeded(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    pub(crate) fn next_inclusive(&mut self, upper_bound: i16) -> Result<i16, RandomError> {
        if upper_bound <= 0 {
            return Err(RandomError::InvalidUpperBound { upper_bound });
        }
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let value = self.state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let upper_bound = u64::try_from(upper_bound).expect("positive upper bound fits u64");
        Ok((value % upper_bound + 1) as i16)
    }
}
