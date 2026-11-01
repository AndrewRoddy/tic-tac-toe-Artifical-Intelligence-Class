use std::cmp::Ordering;
use std::time::Duration;

use crate::accuracy::{Interval, wilson};
use crate::board::Geometry;
use crate::dataset::Labelled;
use crate::outcome::Outcome;
use crate::tournament::{Deadline, Entrant, Fixture, Record, Seat};

#[must_use]
pub fn fixtures<G: Geometry>(
    openings: &[Labelled<G>],
    agent: Seat,
    solver_per_move: Duration,
) -> Vec<Fixture<G>> {
    let solver = Seat {
        entrant: Entrant::Perfect,
        per_move: solver_per_move,
    };
    openings
        .iter()
        .enumerate()
        .flat_map(|(opening, labelled)| {
            [(agent, solver), (solver, agent)]
                .map(|(first, second)| (opening, labelled.board, first, second))
        })
        .zip(0_u64..)
        .map(|((opening, start, first, second), index)| Fixture {
            index,
            opening,
            start,
            first,
            second,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    SolverExpired,
    SolverError,
    Scored { value: Outcome, kept: bool },
}

fn judge(record: &Record, opening_value: Outcome) -> Verdict {
    let agent_first = record.game.is_multiple_of(2);
    let (solver_deadline, outcome, value) = if agent_first {
        (record.second_deadline, record.outcome, opening_value)
    } else {
        (
            record.first_deadline,
            record.outcome.flip(),
            opening_value.flip(),
        )
    };
    if solver_deadline == Deadline::Missed {
        return Verdict::SolverExpired;
    }
    match outcome.cmp(&value) {
        Ordering::Greater => Verdict::SolverError,
        Ordering::Equal => Verdict::Scored { value, kept: true },
        Ordering::Less => Verdict::Scored { value, kept: false },
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub games: u32,
    pub kept: u32,
}

impl Tally {
    #[must_use]
    pub fn rate(&self) -> Option<f64> {
        (self.games > 0).then(|| f64::from(self.kept) / f64::from(self.games))
    }

    #[must_use]
    pub fn interval(&self) -> Option<Interval> {
        wilson(self.kept, self.games)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub win: Tally,
    pub draw: Tally,
    pub loss: Tally,
    pub solver_errors: u32,
    pub expired: u32,
}

impl Summary {
    pub fn add(&mut self, record: &Record, opening_value: Outcome) -> Verdict {
        let verdict = judge(record, opening_value);
        match verdict {
            Verdict::SolverExpired => self.expired += 1,
            Verdict::SolverError => self.solver_errors += 1,
            Verdict::Scored { value, kept } => {
                let tally = match value {
                    Outcome::Win => &mut self.win,
                    Outcome::Draw => &mut self.draw,
                    Outcome::Loss => &mut self.loss,
                };
                tally.games += 1;
                tally.kept += u32::from(kept);
            }
        }
        verdict
    }

    #[must_use]
    pub const fn headline(&self) -> Tally {
        Tally {
            games: self.win.games + self.draw.games,
            kept: self.win.kept + self.draw.kept,
        }
    }
}
