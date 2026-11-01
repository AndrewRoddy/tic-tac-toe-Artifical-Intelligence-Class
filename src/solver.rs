use std::time::Instant;

use crate::bits::Bits;
use crate::board::{Board, COLUMN_LIMIT, Geometry};
use crate::clock::Clock;
use crate::outcome::Outcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub result: Outcome,
    pub columns: Vec<Option<Outcome>>,
}

#[derive(Clone, Copy)]
struct Entry<B> {
    key: B,
    lower: Outcome,
    upper: Outcome,
    effort: u32,
}

impl<B> Entry<B> {
    fn new(key: B, value: Outcome, low: Outcome, high: Outcome) -> Self {
        let lower = if value > low { value } else { Outcome::Loss };
        let upper = if value < high { value } else { Outcome::Win };
        Self {
            key,
            lower,
            upper,
            effort: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Bucket<B> {
    deep: Option<Entry<B>>,
    always: Option<Entry<B>>,
}

impl<B: Bits> Bucket<B> {
    fn probe(&self, key: B) -> Option<Entry<B>> {
        let matches = |entry: &Entry<B>| entry.key == key;
        self.deep
            .filter(matches)
            .or_else(|| self.always.filter(matches))
    }

    fn store(&mut self, entry: Entry<B>) {
        match self.deep {
            Some(deep) if deep.key == entry.key => {
                self.deep = Some(Entry {
                    lower: deep.lower.max(entry.lower),
                    upper: deep.upper.min(entry.upper),
                    effort: deep.effort.max(entry.effort),
                    ..deep
                });
            }
            Some(deep) if deep.effort > entry.effort => self.always = Some(entry),
            _ => self.deep = Some(entry),
        }
    }
}

pub struct Solver<G: Geometry> {
    buckets: Vec<Bucket<G::Bits>>,
    clock: Clock,
}

impl<G: Geometry> Solver<G> {
    #[must_use]
    pub fn new(table_bits: u32) -> Self {
        debug_assert!((1..usize::BITS).contains(&table_bits));
        Self {
            buckets: vec![
                Bucket {
                    deep: None,
                    always: None,
                };
                1 << table_bits
            ],
            clock: Clock::new(None),
        }
    }

    #[must_use]
    pub fn solve(&mut self, board: Board<G>, deadline: Option<Instant>) -> Option<Outcome> {
        self.clock = Clock::new(deadline);
        self.negamax(board, Outcome::Loss, Outcome::Win)
    }

    #[must_use]
    pub fn label(&mut self, board: Board<G>, deadline: Option<Instant>) -> Option<Label> {
        let columns = (0..G::COLUMNS)
            .map(|col| {
                board
                    .can_play(col)
                    .then(|| self.reply(board, col, deadline))
                    .map_or(Some(None), |reply| reply.map(Some))
            })
            .collect::<Option<Vec<_>>>()?;
        let result = columns
            .iter()
            .flatten()
            .max()
            .copied()
            .unwrap_or(Outcome::Draw);
        Some(Label { result, columns })
    }

    #[must_use]
    pub fn reply(
        &mut self,
        mut board: Board<G>,
        col: u32,
        deadline: Option<Instant>,
    ) -> Option<Outcome> {
        if board.wins(col) {
            return Some(Outcome::Win);
        }
        board.play(col);
        self.solve(board, deadline).map(Outcome::flip)
    }

    fn negamax(&mut self, board: Board<G>, low: Outcome, high: Outcome) -> Option<Outcome> {
        if board.playable().next().is_none() {
            return Some(Outcome::Draw);
        }
        if board.can_win() {
            return Some(Outcome::Win);
        }
        let key = board.key();
        let index = key.mix() & (self.buckets.len() - 1);
        let cached = self.buckets[index].probe(key);
        let alpha = cached.map_or(low, |entry| low.max(entry.lower));
        let beta = cached.map_or(high, |entry| high.min(entry.upper));
        if alpha >= beta {
            return Some(alpha);
        }
        if self.clock.expired() {
            return None;
        }
        let before = self.clock.nodes();
        let alpha = search(self, board, alpha, beta)?;
        let effort = u32::try_from(self.clock.nodes() - before).unwrap_or(u32::MAX);
        let entry = Entry {
            effort,
            ..Entry::new(key, alpha, low, high)
        };
        self.buckets[index].store(entry);
        Some(alpha)
    }
}

fn search<G: Geometry>(
    solver: &mut Solver<G>,
    board: Board<G>,
    mut alpha: Outcome,
    beta: Outcome,
) -> Option<Outcome> {
    let mut order = [(0, 0); COLUMN_LIMIT];
    let mut count = 0;
    for col in board.centre_order() {
        let score = board.threat_count_after(col);
        let mut at = count;
        while at > 0 && order[at - 1].0 < score {
            order[at] = order[at - 1];
            at -= 1;
        }
        order[at] = (score, col);
        count += 1;
    }
    for &(_, col) in &order[..count] {
        let mut child = board;
        child.play(col);
        alpha = alpha.max(solver.negamax(child, beta.flip(), alpha.flip())?.flip());
        if alpha >= beta {
            break;
        }
    }
    Some(alpha)
}
