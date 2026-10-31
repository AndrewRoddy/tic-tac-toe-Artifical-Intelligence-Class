use crate::board::{Board, Geometry};
use crate::outcome::Outcome;

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

pub struct Sample<G: Geometry> {
    pub columns: Vec<u32>,
    pub board: Board<G>,
}

fn is_safe<G: Geometry>(board: Board<G>, col: u32) -> bool {
    let mut after = board;
    after.play(col);
    !after.can_win()
}

#[must_use]
pub fn sample<G: Geometry>(rng: &mut SplitMix, stones: usize) -> Option<Sample<G>> {
    let mut board = Board::<G>::default();
    let mut columns = Vec::with_capacity(stones);
    while columns.len() < stones {
        let safe = || board.playable().filter(|&col| is_safe(board, col));
        let index = rng.below(safe().count())?;
        let col = safe().nth(index)?;
        board.play(col);
        columns.push(col);
    }
    Some(Sample { columns, board })
}

pub struct Labelled<G: Geometry> {
    pub board: Board<G>,
    pub result: Outcome,
}

#[must_use]
pub fn parse_line<G: Geometry>(line: &str) -> Option<Labelled<G>> {
    let fields: Vec<&str> = line.split(' ').collect();
    let [moves, result, columns] = fields.as_slice() else {
        return None;
    };
    let board = Board::from_moves(moves)?;
    let result = Outcome::from_symbol(single_char(result)?)?;
    let columns_valid = columns
        .chars()
        .all(|symbol| symbol == '.' || Outcome::from_symbol(symbol).is_some());
    columns_valid.then_some(Labelled { board, result })
}
#[must_use]
pub fn parse_pons<G: Geometry>(line: &str) -> Option<Labelled<G>> {
    let (moves, score) = line.split_once(' ')?;
    let points = score.parse::<i32>().ok()?;
    let result = match points {
        ..0 => Outcome::Loss,
        0 => Outcome::Draw,
        1.. => Outcome::Win,
    };
    Some(Labelled {
        board: Board::from_moves(moves)?,
        result,
    })
}

fn single_char(field: &str) -> Option<char> {
    let mut chars = field.chars();
    let only = chars.next()?;
    chars.next().is_none().then_some(only)
}
