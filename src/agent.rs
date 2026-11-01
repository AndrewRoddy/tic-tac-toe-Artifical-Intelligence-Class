use std::time::Instant;

use burn::backend::Flex;
use burn::backend::flex::FlexDevice;

use crate::board::{Board, Geometry};
use crate::mcts::{Budget, Guide, Playouts, Uct};
use crate::net::{self, Net};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};
use crate::rules;
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

pub struct Mcts<G: Geometry, P: Guide<G> = Playouts> {
    tree: Uct<G, P>,
}

impl<G: Geometry> Mcts<G, Playouts> {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            tree: Uct::new(seed),
        }
    }
}

impl<G: Geometry> Mcts<G, Network> {
    #[must_use]
    pub fn shipped() -> Option<Self> {
        Network::shipped().map(|network| Self {
            tree: Uct::with_guide(network),
        })
    }
}

impl<G: Geometry, P: Guide<G>> Agent<G> for Mcts<G, P> {
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

pub struct Network {
    net: Net<Flex>,
    device: FlexDevice,
}

impl Network {
    #[must_use]
    pub fn shipped() -> Option<Self> {
        let device = FlexDevice;
        net::shipped::<Flex>(&device).map(|net| Self { net, device })
    }
}

impl<G: Geometry> Agent<G> for Network {
    fn choose(&mut self, board: Board<G>, _deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        self.net
            .forecast(board, &self.device)
            .and_then(|forecast| net::choose(board, &forecast))
            .unwrap_or(0)
    }
}

// Schrittwieser et al. 2020, Appendix B (MuZero pseudocode): value_score is 0 for an unvisited
// child; pb_c uses C_INIT = 1.25 and C_BASE = 19652.
const FIRST_PLAY_VALUE: f64 = 0.0;
const C_INIT: f64 = 1.25;
const C_BASE: f64 = 19652.0;

impl<G: Geometry> Guide<G> for Network {
    fn evaluate(&mut self, board: Board<G>, priors: &mut [f64]) -> Option<f64> {
        let forecast = self.net.forecast(board, &self.device)?;
        let mut total = 0.0;
        let mut value = 0.0_f64;
        for (column, prior) in board.playable().zip(priors.iter_mut()) {
            let expected = f64::from(forecast.expected_at(column)?);
            *prior = expected;
            total += expected;
            value = value.max(expected);
        }
        let scale = if total > 0.0 { total.recip() } else { 0.0 };
        for prior in priors {
            *prior *= scale;
        }
        Some(value)
    }

    fn first_play_value(&self) -> f64 {
        FIRST_PLAY_VALUE
    }

    fn bonus(&self, parent_visits: u32, child_visits: u32, prior: f64) -> f64 {
        let parent = f64::from(parent_visits);
        let scale = ((parent + C_BASE + 1.0) / C_BASE).ln() + C_INIT;
        scale * prior * parent.sqrt() / (1.0 + f64::from(child_visits))
    }
}

pub struct Allis<G: Geometry> {
    left: Option<Board<G>>,
}

impl<G: Geometry> Default for Allis<G> {
    fn default() -> Self {
        Self { left: None }
    }
}

impl<G: Geometry> Agent<G> for Allis<G> {
    fn choose(&mut self, board: Board<G>, _deadline: Instant) -> u32 {
        debug_assert!(board.playable().next().is_some());
        let last = self.left.and_then(|left| opponent_move(left, board));
        let col = rules::choose(board, last).unwrap_or(0);
        let mut next = board;
        next.play(col);
        self.left = Some(next);
        col
    }
}

fn opponent_move<G: Geometry>(left: Board<G>, board: Board<G>) -> Option<u32> {
    let mut moves = left.playable().filter(|&col| {
        let mut next = left;
        next.play(col);
        next.key() == board.key()
    });
    let col = moves.next()?;
    moves.next().is_none().then_some(col)
}
