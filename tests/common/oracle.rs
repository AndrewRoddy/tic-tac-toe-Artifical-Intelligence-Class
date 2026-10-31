use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use c4::board::{Board, Geometry};
use c4::outcome::Outcome;

pub type Cells = HashSet<(i64, i64)>;

#[derive(Clone, Default)]
pub struct Partial {
    pub stones: [Cells; 2],
    pub heights: HashMap<i64, i64>,
}

impl Partial {
    #[must_use]
    pub fn cell(&self, column: u32) -> (i64, i64) {
        let row = self.heights.get(&i64::from(column)).copied().unwrap_or(0);
        (i64::from(column), row)
    }

    pub fn place(&mut self, side: usize, column: u32) {
        let cell = self.cell(column);
        self.stones[side].insert(cell);
        self.heights.insert(i64::from(column), cell.1 + 1);
    }
}

#[must_use]
pub fn grid_wins(own: &Cells, (column, row): (i64, i64)) -> bool {
    let run = |(dcol, drow): (i64, i64)| {
        (1..4)
            .take_while(|&step| own.contains(&(column + step * dcol, row + step * drow)))
            .count()
    };
    [(1, 0), (0, 1), (1, 1), (1, -1)]
        .into_iter()
        .any(|(dcol, drow)| 1 + run((dcol, drow)) + run((-dcol, -drow)) >= 4)
}

#[must_use]
pub fn solve<G: Geometry, S: BuildHasher>(
    board: Board<G>,
    memo: &mut HashMap<G::Bits, Outcome, S>,
) -> Outcome {
    let key = board.key();
    if let Some(&known) = memo.get(&key) {
        return known;
    }
    let outcome = board
        .playable()
        .map(|col| column(board, col, memo))
        .max()
        .unwrap_or(Outcome::Draw);
    memo.insert(key, outcome);
    outcome
}

#[must_use]
pub fn column<G: Geometry, S: BuildHasher>(
    board: Board<G>,
    col: u32,
    memo: &mut HashMap<G::Bits, Outcome, S>,
) -> Outcome {
    if board.wins(col) {
        return Outcome::Win;
    }
    let mut child = board;
    child.play(col);
    solve(child, memo).flip()
}
