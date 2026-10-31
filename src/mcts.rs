use std::iter::successors;
use std::ops::ControlFlow;
use std::time::Instant;

use crate::board::{Board, Geometry};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};

// UCB1 exploration constant: Auer, Cesa-Bianchi, Fischer 2002.
const EXPLORATION: f64 = std::f64::consts::SQRT_2;
const ROOT: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub deadline: Instant,
    pub simulations: Option<u32>,
}

#[derive(Clone, Copy)]
struct Node<G: Geometry> {
    board: Board<G>,
    column: Option<u32>,
    parent: Option<usize>,
    first_child: Option<usize>,
    next_sibling: Option<usize>,
    untried: u32,
    visits: u32,
    half_points: u32,
    ended: Option<Outcome>,
}

pub struct Uct<G: Geometry> {
    nodes: Vec<Node<G>>,
    rng: SplitMix,
}

impl<G: Geometry> Uct<G> {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        const { assert!(G::COLUMNS <= u32::BITS, "columns must fit a u32 mask") };
        Self {
            nodes: Vec::new(),
            rng: SplitMix::new(seed),
        }
    }

    #[must_use]
    pub fn search(&mut self, board: Board<G>, budget: Budget) -> Option<u32> {
        let fallback = board.playable().next()?;
        self.nodes.clear();
        self.nodes.push(Node {
            board,
            column: None,
            parent: None,
            first_child: None,
            next_sibling: None,
            untried: playable_mask(board),
            visits: 0,
            half_points: 0,
            ended: None,
        });
        let mut done: u32 = 0;
        while budget.simulations.is_none_or(|cap| done < cap)
            && Instant::now() < budget.deadline
            && self.simulate().is_some()
        {
            done += 1;
        }
        self.most_visited().or(Some(fallback))
    }

    fn simulate(&mut self) -> Option<()> {
        let leaf = self.descend(ROOT)?;
        let ended = self.nodes[leaf].ended;
        let board = self.nodes[leaf].board;
        let outcome = ended.unwrap_or_else(|| self.playout(board));
        self.backpropagate(leaf, outcome);
        Some(())
    }

    fn descend(&mut self, mut node: usize) -> Option<usize> {
        while self.nodes[node].ended.is_none() && self.nodes[node].untried == 0 {
            node = self.select_child(node)?;
        }
        if self.nodes[node].ended.is_some() {
            return Some(node);
        }
        self.expand(node)
    }

    fn expand(&mut self, parent: usize) -> Option<usize> {
        let board = self.nodes[parent].board;
        let untried = self.nodes[parent].untried;
        let column = pick(&mut self.rng, board, |col| untried & bit(col) != 0)?;
        let mut next = board;
        next.play(column);
        let child = self.nodes.len();
        let next_sibling = self.nodes[parent].first_child;
        self.nodes[parent].untried &= !bit(column);
        self.nodes.push(Node {
            board: next,
            column: Some(column),
            parent: Some(parent),
            first_child: None,
            next_sibling,
            untried: playable_mask(next),
            visits: 0,
            half_points: 0,
            ended: after(board, column).map(Outcome::flip),
        });
        self.nodes[parent].first_child = Some(child);
        Some(child)
    }

    fn select_child(&self, parent: usize) -> Option<usize> {
        let ln_parent = f64::from(self.nodes[parent].visits).ln();
        children(&self.nodes, parent).max_by(|&left, &right| {
            self.score(left, ln_parent)
                .total_cmp(&self.score(right, ln_parent))
        })
    }

    fn score(&self, child: usize, ln_parent: f64) -> f64 {
        let node = &self.nodes[child];
        let visits = f64::from(node.visits);
        let mean = f64::from(node.half_points) / (f64::from(Outcome::Win.half_points()) * visits);
        EXPLORATION.mul_add((ln_parent / visits).sqrt(), mean)
    }

    fn playout(&mut self, board: Board<G>) -> Outcome {
        let rng = &mut self.rng;
        (0..)
            .try_fold(board, |current, ply| step(rng, current, ply))
            .break_value()
            .unwrap_or(Outcome::Draw)
    }

    fn backpropagate(&mut self, leaf: usize, outcome: Outcome) {
        let mut node = Some(leaf);
        let mut value = outcome;
        while let Some(current) = node {
            let stats = &mut self.nodes[current];
            stats.visits += 1;
            stats.half_points += value.flip().half_points();
            node = stats.parent;
            value = value.flip();
        }
    }

    fn most_visited(&self) -> Option<u32> {
        children(&self.nodes, ROOT)
            .max_by_key(|&child| self.nodes[child].visits)
            .and_then(|child| self.nodes[child].column)
    }
}

fn children<G: Geometry>(nodes: &[Node<G>], parent: usize) -> impl Iterator<Item = usize> + '_ {
    successors(nodes[parent].first_child, move |&child| {
        nodes[child].next_sibling
    })
}

fn after<G: Geometry>(board: Board<G>, column: u32) -> Option<Outcome> {
    if board.wins(column) {
        return Some(Outcome::Win);
    }
    let mut next = board;
    next.play(column);
    next.playable().next().is_none().then_some(Outcome::Draw)
}

const fn seen_from_start(result: Outcome, ply: u32) -> Outcome {
    if ply.is_multiple_of(2) {
        result
    } else {
        result.flip()
    }
}

fn step<G: Geometry>(
    rng: &mut SplitMix,
    board: Board<G>,
    ply: u32,
) -> ControlFlow<Outcome, Board<G>> {
    pick(rng, board, |_| true).map_or(ControlFlow::Break(Outcome::Draw), |column| {
        let mut next = board;
        next.play(column);
        after(board, column).map_or(ControlFlow::Continue(next), |result| {
            ControlFlow::Break(seen_from_start(result, ply))
        })
    })
}

fn playable_mask<G: Geometry>(board: Board<G>) -> u32 {
    board.playable().fold(0, |mask, col| mask | bit(col))
}

const fn bit(column: u32) -> u32 {
    1 << column
}
