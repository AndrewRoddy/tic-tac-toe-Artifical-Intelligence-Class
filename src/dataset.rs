use std::collections::HashSet;

use crate::board::{Board, Geometry};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};

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
        let col = pick(rng, board, |col| is_safe(board, col))?;
        board.play(col);
        columns.push(col);
    }
    Some(Sample { columns, board })
}

#[must_use]
pub fn unique_samples<G: Geometry>(
    seed: u64,
    count: usize,
    low: usize,
    high: usize,
) -> Vec<Sample<G>> {
    debug_assert!((1..=high).contains(&low));
    let mut rng = SplitMix::new(seed);
    let mut distinct = HashSet::new();
    let mut found = Vec::with_capacity(count);
    let mut targets = (low..=high).cycle().skip(1);
    let mut stones = low;
    while found.len() < count {
        let Some(drawn) = sample::<G>(&mut rng, stones) else {
            continue;
        };
        stones = targets.next().unwrap_or(low);
        if distinct.insert(drawn.board.canonical()) {
            found.push(drawn);
        }
    }
    found
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
