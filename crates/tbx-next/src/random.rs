/// Session-owned pseudo-random state.
///
/// The explicit-seed sequence is a compatibility contract for `--seed` (ADR
/// #1889). Keep its integer width, state transitions, wrapping multiplication,
/// and modulo operation unchanged: users rely on a seed retaining the same
/// meaning across TBX Next versions and platforms. The unseeded host path may
/// choose a different generator in the future, but it must preserve this
/// sequence for explicit seeds.
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

#[cfg(test)]
mod tests {
    use super::RandomState;

    fn series(seed: u64, upper_bounds: &[i16]) -> Vec<i16> {
        let mut random = RandomState::seeded(seed);
        upper_bounds
            .iter()
            .map(|&upper_bound| {
                random
                    .next_inclusive(upper_bound)
                    .expect("golden test bounds are positive")
            })
            .collect()
    }

    #[test]
    fn explicit_seed_series_is_stable_for_zero_and_representative_seeds() {
        let upper_bounds = [10, 100, 97, 32_767, 10];

        assert_eq!(RandomState::seeded(0).state, 0x9E37_79B9_7F4A_7C15);
        assert_eq!(series(0, &upper_bounds), [1, 88, 1, 30_210, 8]);
        assert_eq!(series(1, &upper_bounds), [6, 18, 43, 18_428, 9]);
        assert_eq!(series(42, &upper_bounds), [1, 99, 38, 12_313, 9]);
        assert_eq!(series(u64::MAX, &upper_bounds), [7, 80, 94, 16_256, 8]);
    }
}
