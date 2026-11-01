use std::time::{Duration, Instant};

use crate::agent::Agent;
use crate::board::{Board, Geometry};
use crate::outcome::Outcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub outcome: Outcome,
    pub moves: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    pub first: Duration,
    pub second: Duration,
}

pub fn play<'a, G: Geometry>(
    start: Board<G>,
    first: &'a mut dyn Agent<G>,
    second: &'a mut dyn Agent<G>,
    pace: Pace,
) -> Game {
    let mut board = start;
    let mut ply = 0_u32;
    let mut moves = Vec::new();
    while board.playable().next().is_some() {
        let (agent, per_move) = if ply.is_multiple_of(2) {
            (&mut *first, pace.first)
        } else {
            (&mut *second, pace.second)
        };
        let col = agent.choose(board, Instant::now() + per_move);
        moves.push(col);
        if let Some(outcome) = verdict(board, col) {
            return Game {
                outcome: for_first(ply, outcome),
                moves,
            };
        }
        board.play(col);
        ply += 1;
    }
    Game {
        outcome: Outcome::Draw,
        moves,
    }
}

fn verdict<G: Geometry>(board: Board<G>, col: u32) -> Option<Outcome> {
    if col >= G::COLUMNS || !board.can_play(col) {
        return Some(Outcome::Loss);
    }
    board.wins(col).then_some(Outcome::Win)
}

const fn for_first(ply: u32, outcome: Outcome) -> Outcome {
    if ply.is_multiple_of(2) {
        outcome
    } else {
        outcome.flip()
    }
}
