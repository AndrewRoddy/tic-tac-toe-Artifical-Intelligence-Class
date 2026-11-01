#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Loss,
    Draw,
    Win,
}

const HALF_POINTS_PER_WIN: u32 = 2;
const HALF_POINTS_PER_DRAW: u32 = 1;

impl Outcome {
    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::Loss => Self::Win,
            Self::Draw => Self::Draw,
            Self::Win => Self::Loss,
        }
    }

    #[must_use]
    pub const fn half_points(self) -> u32 {
        match self {
            Self::Win => HALF_POINTS_PER_WIN,
            Self::Draw => HALF_POINTS_PER_DRAW,
            Self::Loss => 0,
        }
    }

    #[must_use]
    pub fn score(self) -> f64 {
        f64::from(self.half_points()) / f64::from(HALF_POINTS_PER_WIN)
    }

    #[must_use]
    pub const fn symbol(self) -> char {
        match self {
            Self::Win => '+',
            Self::Draw => '=',
            Self::Loss => '-',
        }
    }

    #[must_use]
    pub const fn from_symbol(symbol: char) -> Option<Self> {
        match symbol {
            '+' => Some(Self::Win),
            '=' => Some(Self::Draw),
            '-' => Some(Self::Loss),
            _ => None,
        }
    }
}
