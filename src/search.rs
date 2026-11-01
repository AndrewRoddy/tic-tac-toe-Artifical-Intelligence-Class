use std::time::Instant;

use crate::board::{Board, Geometry};
use crate::clock::Clock;

const WIN_SCORE: i64 = 1_000_000;
const DRAW_SCORE: i64 = 0;
const INFINITY: i64 = 2 * WIN_SCORE;
const GOOD_ROW_THREAT: i32 = 4;
const OTHER_ROW_THREAT: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Features(pub [u32; 4]);

impl Features {
    #[must_use]
    pub fn of<G: Geometry>(board: Board<G>) -> Self {
        let threats = board.threat_rows();
        Self([
            threats.mover_good,
            threats.mover_other,
            threats.opponent_good,
            threats.opponent_other,
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights(pub [i32; 4]);

impl Weights {
    pub const DEFAULT: Self = Self([
        GOOD_ROW_THREAT,
        OTHER_ROW_THREAT,
        -GOOD_ROW_THREAT,
        -OTHER_ROW_THREAT,
    ]);
    pub const NONE: Self = Self([0; 4]);
    pub const LEARNED: Self = Self([1010, 662, -1152, -895]);

    fn score(self, features: Features) -> i64 {
        self.0
            .iter()
            .zip(features.0)
            .map(|(&weight, count)| i64::from(weight) * i64::from(count))
            .sum()
    }
}

#[derive(Clone, Copy)]
struct Node {
    depth: u32,
    alpha: i64,
    beta: i64,
}

struct Search {
    clock: Clock,
    weights: Weights,
}

fn root<G: Geometry>(search: &mut Search, board: Board<G>, depth: u32) -> Option<(i64, u32)> {
    let mut best: Option<(i64, u32)> = None;
    let mut alpha = -INFINITY;
    for col in board.centre_order() {
        let node = Node {
            depth,
            alpha,
            beta: INFINITY,
        };
        let score = reply(search, board, col, node)?;
        if best.is_none_or(|(top, _)| score > top) {
            best = Some((score, col));
        }
        alpha = alpha.max(score);
    }
    best
}

fn negamax<G: Geometry>(search: &mut Search, board: Board<G>, node: Node) -> Option<i64> {
    if search.clock.expired() {
        return None;
    }
    if board.can_win() {
        return Some(WIN_SCORE + i64::from(node.depth));
    }
    if node.depth == 0 {
        return Some(evaluate(search.weights, board));
    }
    let mut best = None;
    let mut alpha = node.alpha;
    for col in board.centre_order() {
        let score = reply(search, board, col, Node { alpha, ..node })?;
        best = Some(best.map_or(score, |top: i64| top.max(score)));
        alpha = alpha.max(score);
        if alpha >= node.beta {
            break;
        }
    }
    Some(best.unwrap_or(DRAW_SCORE))
}

fn reply<G: Geometry>(search: &mut Search, board: Board<G>, col: u32, node: Node) -> Option<i64> {
    if board.wins(col) {
        return Some(WIN_SCORE + i64::from(node.depth));
    }
    let mut next = board;
    next.play(col);
    let child = Node {
        depth: node.depth.saturating_sub(1),
        alpha: -node.beta,
        beta: -node.alpha,
    };
    negamax(search, next, child).map(|score| -score)
}

fn evaluate<G: Geometry>(weights: Weights, board: Board<G>) -> i64 {
    weights.score(Features::of(board))
}

#[must_use]
pub fn best_column<G: Geometry>(
    board: Board<G>,
    max_depth: u32,
    deadline: Instant,
    weights: Weights,
) -> Option<u32> {
    let mut search = Search {
        clock: Clock::new(Some(deadline)),
        weights,
    };
    let mut best = None;
    for depth in 1..=max_depth.min(board.empty_cells()) {
        let Some((score, col)) = root(&mut search, board, depth) else {
            break;
        };
        best = Some(col);
        if score >= WIN_SCORE {
            break;
        }
    }
    best.or_else(|| board.centre_order().next())
}
