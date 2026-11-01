use std::ops::{ControlFlow, Range};
use std::time::Instant;

use crate::board::{Board, Geometry};
use crate::outcome::Outcome;
use crate::random::{SplitMix, pick};

// UCB1 exploration constant: Auer, Cesa-Bianchi, Fischer 2002.
const EXPLORATION: f64 = std::f64::consts::SQRT_2;
const FIRST_PLAY_VALUE: f64 = 0.0;
const ROOT: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub deadline: Instant,
    pub simulations: Option<u32>,
}

pub trait Guide<G: Geometry> {
    fn evaluate(&mut self, board: Board<G>, priors: &mut [f64]) -> Option<f64>;
    fn bonus(&self, parent_visits: u32, child_visits: u32, prior: f64) -> f64;
    fn first_play_value(&self) -> f64;
}

pub struct Playouts {
    rng: SplitMix,
}

impl Playouts {
    const fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix::new(seed),
        }
    }
}

impl<G: Geometry> Guide<G> for Playouts {
    fn evaluate(&mut self, board: Board<G>, _priors: &mut [f64]) -> Option<f64> {
        Some(playout(&mut self.rng, board).score())
    }

    fn first_play_value(&self) -> f64 {
        FIRST_PLAY_VALUE
    }

    fn bonus(&self, parent_visits: u32, child_visits: u32, _prior: f64) -> f64 {
        if child_visits == 0 {
            return f64::INFINITY;
        }
        EXPLORATION * (f64::from(parent_visits).ln() / f64::from(child_visits)).sqrt()
    }
}

struct Node<G: Geometry> {
    board: Board<G>,
    column: Option<u32>,
    parent: Option<usize>,
    children: Range<usize>,
    prior: f64,
    visits: u32,
    value_sum: f64,
    ended: Option<Outcome>,
}

pub struct Uct<G: Geometry, P: Guide<G> = Playouts> {
    nodes: Vec<Node<G>>,
    guide: P,
    priors: Vec<f64>,
}

impl<G: Geometry> Uct<G, Playouts> {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self::with_guide(Playouts::new(seed))
    }
}

impl<G: Geometry, P: Guide<G>> Uct<G, P> {
    #[must_use]
    pub const fn with_guide(guide: P) -> Self {
        Self {
            nodes: Vec::new(),
            guide,
            priors: Vec::new(),
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
            children: 0..0,
            prior: 0.0,
            visits: 0,
            value_sum: 0.0,
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
        let value = match self.nodes[leaf].ended {
            Some(outcome) => outcome.score(),
            None => self.expand(leaf)?,
        };
        self.backpropagate(leaf, value);
        Some(())
    }

    fn descend(&self, mut node: usize) -> Option<usize> {
        while self.nodes[node].ended.is_none() && !self.nodes[node].children.is_empty() {
            node = self.select_child(node)?;
        }
        Some(node)
    }

    fn expand(&mut self, leaf: usize) -> Option<f64> {
        let board = self.nodes[leaf].board;
        self.priors.clear();
        self.priors.resize(board.playable().count(), 0.0);
        let value = self.guide.evaluate(board, &mut self.priors)?;
        let first = self.nodes.len();
        for (column, &prior) in board.playable().zip(&self.priors) {
            let mut next = board;
            next.play(column);
            self.nodes.push(Node {
                board: next,
                column: Some(column),
                parent: Some(leaf),
                children: 0..0,
                prior,
                visits: 0,
                value_sum: 0.0,
                ended: child_ended(board, column),
            });
        }
        self.nodes[leaf].children = first..self.nodes.len();
        Some(value)
    }

    fn select_child(&self, parent: usize) -> Option<usize> {
        let parent_visits = self.nodes[parent].visits;
        self.nodes[parent].children.clone().max_by(|&left, &right| {
            self.score(left, parent_visits)
                .total_cmp(&self.score(right, parent_visits))
        })
    }

    fn value(&self, child: usize) -> f64 {
        let node = &self.nodes[child];
        if node.visits == 0 {
            return self.guide.first_play_value();
        }
        node.value_sum / f64::from(node.visits)
    }

    fn score(&self, child: usize, parent_visits: u32) -> f64 {
        let node = &self.nodes[child];
        match node.ended {
            Some(Outcome::Loss) => f64::INFINITY,
            Some(Outcome::Win) => f64::NEG_INFINITY,
            Some(Outcome::Draw) | None => {
                self.value(child) + self.guide.bonus(parent_visits, node.visits, node.prior)
            }
        }
    }

    fn backpropagate(&mut self, leaf: usize, value: f64) {
        let mut node = Some(leaf);
        let mut side_to_move = value;
        while let Some(current) = node {
            let stats = &mut self.nodes[current];
            stats.visits += 1;
            stats.value_sum += 1.0 - side_to_move;
            node = stats.parent;
            side_to_move = 1.0 - side_to_move;
        }
    }

    fn most_visited(&self) -> Option<u32> {
        self.nodes[ROOT]
            .children
            .clone()
            .max_by_key(|&child| self.nodes[child].visits)
            .and_then(|child| self.nodes[child].column)
    }
}

fn after<G: Geometry>(board: Board<G>, column: u32) -> Option<Outcome> {
    if board.wins(column) {
        return Some(Outcome::Win);
    }
    let mut next = board;
    next.play(column);
    next.playable().next().is_none().then_some(Outcome::Draw)
}

fn child_ended<G: Geometry>(board: Board<G>, column: u32) -> Option<Outcome> {
    after(board, column).map(Outcome::flip).or_else(|| {
        let mut next = board;
        next.play(column);
        next.can_win().then_some(Outcome::Win)
    })
}

fn playout<G: Geometry>(rng: &mut SplitMix, board: Board<G>) -> Outcome {
    (0..)
        .try_fold(board, |current, ply| step(rng, current, ply))
        .break_value()
        .unwrap_or(Outcome::Draw)
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
