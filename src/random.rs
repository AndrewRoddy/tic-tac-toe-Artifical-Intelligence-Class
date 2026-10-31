use crate::board::{Board, Geometry};

// SplitMix64, Steele, Lea and Flood (2014); constants from Vigna's splitmix64.c.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX_FIRST: u64 = 0xBF58_476D_1CE4_E5B9;
const MIX_SECOND: u64 = 0x94D0_49BB_1331_11EB;

pub struct SplitMix(u64);

impl SplitMix {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    const fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GOLDEN_GAMMA);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(MIX_FIRST);
        value = (value ^ (value >> 27)).wrapping_mul(MIX_SECOND);
        value ^ (value >> 31)
    }

    fn below(&mut self, bound: usize) -> Option<usize> {
        let size = u64::try_from(bound).ok().filter(|&count| count > 0)?;
        usize::try_from(self.draw() % size).ok()
    }
}

#[must_use]
pub fn pick<G: Geometry>(
    rng: &mut SplitMix,
    board: Board<G>,
    wanted: impl Fn(u32) -> bool,
) -> Option<u32> {
    let count = board.playable().filter(|&col| wanted(col)).count();
    let index = rng.below(count)?;
    board.playable().filter(|&col| wanted(col)).nth(index)
}
