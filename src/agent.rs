use std::time::Instant;

use crate::board::{Board, Geometry};
use crate::mcts::{Budget, Uct};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};
use crate::search::{self, Weights};
use crate::solver::Solver;

pub trait Agent<G: Geometry> {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32;
}

pub struct Perfect<G: Geometry> {
    solver: Solver<G>,
}

impl<G: Geometry> Perfect<G> {
    #[must_use]
    pub fn new(table_bits: u32) -> Self {
        Self {
            solver: Solver::new(table_bits),
        }
    }
}

impl<G: Geometry> Agent<G> for Perfect<G> {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        board
            .playable()
            .max_by_key(|&col| {
                let reply = self.solver.reply(board, col, Some(deadline));
                let not_losing_or_unsolved = reply.is_none_or(|outcome| outcome > Outcome::Loss);
                (
                    not_losing_or_unsolved,
                    reply,
                    std::cmp::Reverse(col.abs_diff(G::COLUMNS / 2)),
                )
            })
            .unwrap_or(0)
    }
}

pub struct AlphaBeta {
    max_depth: u32,
    weights: Weights,
}

impl AlphaBeta {
    #[must_use]
    pub const fn new(max_depth: u32, weights: Weights) -> Self {
        Self { max_depth, weights }
    }
}

impl<G: Geometry> Agent<G> for AlphaBeta {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        search::best_column(board, self.max_depth, deadline, self.weights).unwrap_or(0)
    }
}

pub struct SafeRandom {
    rng: SplitMix,
}

impl SafeRandom {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix::new(seed),
        }
    }
}

impl<G: Geometry> Agent<G> for SafeRandom {
    fn choose(&mut self, board: Board<G>, _deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        pick(&mut self.rng, board, |col| board.wins(col))
            .or_else(|| pick(&mut self.rng, board, |col| board.opponent_wins(col)))
            .or_else(|| pick(&mut self.rng, board, |_| true))
            .unwrap_or(0)
    }
}

pub struct Mcts<G: Geometry> {
    tree: Uct<G>,
}

impl<G: Geometry> Mcts<G> {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            tree: Uct::new(seed),
        }
    }
}

impl<G: Geometry> Agent<G> for Mcts<G> {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        self.tree
            .search(
                board,
                Budget {
                    deadline,
                    simulations: None,
                },
            )
            .unwrap_or(0)
    }
}
