use std::collections::HashSet;

use crate::board::{Board, Geometry};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};

pub struct Sample<G: Geometry> {
    pub columns: Vec<u32>,
    pub board: Board<G>,
}

#[must_use]
pub fn sample<G: Geometry>(rng: &mut SplitMix, stones: usize) -> Option<Sample<G>> {
    let mut board = Board::<G>::default();
    let mut columns = Vec::with_capacity(stones);
    while columns.len() < stones {
        let col = pick(rng, board, |col| !board.allows_win(col))?;
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

pub struct ColumnLabelled<G: Geometry> {
    pub position: Labelled<G>,
    pub columns: Vec<Option<Outcome>>,
}

pub struct Split<T> {
    pub train: Vec<T>,
    pub held: Vec<T>,
}

#[must_use]
pub fn split<T>(lines: Vec<T>) -> Split<T> {
    let mut parts = Split {
        train: Vec::new(),
        held: Vec::new(),
    };
    for (index, line) in lines.into_iter().enumerate() {
        if held_out(index) {
            parts.held.push(line);
        } else {
            parts.train.push(line);
        }
    }
    parts
}

const TRAIN_LINES: usize = 90_000;

const fn held_out(index: usize) -> bool {
    index >= TRAIN_LINES
}

#[must_use]
pub fn parse_line<G: Geometry>(line: &str) -> Option<Labelled<G>> {
    parse_column_labelled(line).map(|labelled| labelled.position)
}

#[must_use]
pub fn parse_column_labelled<G: Geometry>(line: &str) -> Option<ColumnLabelled<G>> {
    let fields: Vec<&str> = line.split(' ').collect();
    let [moves, result, columns] = fields.as_slice() else {
        return None;
    };
    let board = Board::from_moves(moves)?;
    let result = Outcome::from_symbol(single_char(result)?)?;
    let columns = columns
        .chars()
        .map(|symbol| match symbol {
            '.' => Some(None),
            _ => Outcome::from_symbol(symbol).map(Some),
        })
        .collect::<Option<Vec<_>>>()?;
    if u32::try_from(columns.len()).ok()? != G::COLUMNS {
        return None;
    }
    Some(ColumnLabelled {
        position: Labelled { board, result },
        columns,
    })
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
