use std::f64::consts::LN_10;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use crate::agent::{Agent, Allis, AlphaBeta, Mcts, Network, Perfect, SafeRandom};
use crate::arena::{Pace, play};
use crate::board::{Board, Geometry};
use crate::dataset::Sample;
use crate::learn::{Row, fit};
use crate::outcome::Outcome;
use crate::parallel;
use crate::search::Weights;

const ELO_SCALE: f64 = 400.0;
const ANCHOR: Entrant = Entrant::SafeRandom;
const ELO_PER_LOGIT: f64 = ELO_SCALE / LN_10;
pub const PERFECT_TABLE_BITS: u32 = 21;

pub const ROSTER: [Entrant; 8] = [
    Entrant::SafeRandom,
    Entrant::Handcrafted,
    Entrant::Learned,
    Entrant::Perfect,
    Entrant::Mcts,
    Entrant::Network,
    Entrant::NetworkMcts,
    Entrant::Allis,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Entrant {
    SafeRandom,
    Handcrafted,
    Learned,
    Perfect,
    Mcts,
    Network,
    NetworkMcts,
    Allis,
}

impl Entrant {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SafeRandom => "safe-random",
            Self::Handcrafted => "alpha-beta-handcrafted",
            Self::Learned => "alpha-beta-learned",
            Self::Perfect => "perfect",
            Self::Mcts => "mcts",
            Self::Network => "network",
            Self::NetworkMcts => "network-mcts",
            Self::Allis => "allis",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        ROSTER
            .iter()
            .copied()
            .find(|entrant| entrant.name() == name)
    }

    #[must_use]
    pub fn build<G: Geometry + 'static>(
        self,
        seed: u64,
        perfect_table_bits: u32,
    ) -> Option<Box<dyn Agent<G>>> {
        let agent: Box<dyn Agent<G>> = match self {
            Self::SafeRandom => Box::new(SafeRandom::new(seed)),
            Self::Handcrafted => Box::new(AlphaBeta::new(G::COLUMNS * G::ROWS, Weights::DEFAULT)),
            Self::Learned => Box::new(AlphaBeta::new(G::COLUMNS * G::ROWS, Weights::LEARNED)),
            Self::Perfect => Box::new(Perfect::<G>::new(perfect_table_bits)),
            Self::Mcts => Box::new(Mcts::<G>::new(seed)),
            Self::Network => Box::new(Network::shipped()?),
            Self::NetworkMcts => Box::new(Mcts::<G, Network>::shipped()?),
            Self::Allis => Box::new(Allis::<G>::default()),
        };
        Some(agent)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub perfect_table_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seat {
    pub entrant: Entrant,
    pub per_move: Duration,
}

#[derive(Clone, Copy)]
pub struct Fixture<G: Geometry> {
    pub index: u64,
    pub opening: usize,
    pub start: Board<G>,
    pub first: Seat,
    pub second: Seat,
}

fn ordered_pairs(roster: &[Entrant]) -> impl Iterator<Item = (Entrant, Entrant)> + '_ {
    roster.iter().flat_map(move |&first| {
        roster
            .iter()
            .filter(move |&&second| second != first)
            .map(move |&second| (first, second))
    })
}

#[must_use]
pub fn schedule<G: Geometry>(
    roster: &[Entrant],
    openings: &[Sample<G>],
    per_move: Duration,
) -> Vec<Fixture<G>> {
    let games = openings
        .iter()
        .enumerate()
        .flat_map(move |(opening, sample)| {
            ordered_pairs(roster).map(move |(first, second)| (opening, sample.board, first, second))
        });
    games
        .zip(0_u64..)
        .map(|((opening, start, first, second), index)| Fixture {
            index,
            opening,
            start,
            first: Seat {
                entrant: first,
                per_move,
            },
            second: Seat {
                entrant: second,
                per_move,
            },
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deadline {
    Met,
    Missed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub game: u64,
    pub opening: usize,
    pub first: Entrant,
    pub second: Entrant,
    pub outcome: Outcome,
    pub moves: Vec<u32>,
    pub first_deadline: Deadline,
    pub second_deadline: Deadline,
}

struct Watched<G: Geometry> {
    agent: Box<dyn Agent<G>>,
    status: Deadline,
}

fn watch<G: Geometry>(agent: Box<dyn Agent<G>>) -> Watched<G> {
    Watched {
        agent,
        status: Deadline::Met,
    }
}

impl<G: Geometry> Agent<G> for Watched<G> {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32 {
        let column = self.agent.choose(board, deadline);
        if Instant::now() >= deadline {
            self.status = Deadline::Missed;
        }
        column
    }
}

fn play_fixture<G: Geometry + 'static>(fixture: &Fixture<G>, settings: Settings) -> Option<Record> {
    let table_bits = settings.perfect_table_bits;
    let index = fixture.index;
    let mut first = watch(fixture.first.entrant.build::<G>(index, table_bits)?);
    let mut second = watch(fixture.second.entrant.build::<G>(index, table_bits)?);
    let pace = Pace {
        first: fixture.first.per_move,
        second: fixture.second.per_move,
    };
    let game = play(fixture.start, &mut first, &mut second, pace);
    Some(Record {
        game: fixture.index,
        opening: fixture.opening,
        first: fixture.first.entrant,
        second: fixture.second.entrant,
        outcome: game.outcome,
        moves: game.moves,
        first_deadline: first.status,
        second_deadline: second.status,
    })
}

pub fn run<G: Geometry + 'static>(
    fixtures: &[Fixture<G>],
    settings: Settings,
    threads: usize,
    mut sink: impl FnMut(Record) -> ControlFlow<()>,
) -> Option<()> {
    let mut complete = true;
    parallel::map_each(
        fixtures,
        threads,
        |fixture| play_fixture(fixture, settings),
        |record| {
            let Some(record) = record else {
                complete = false;
                return ControlFlow::Break(());
            };
            sink(record)
        },
    );
    complete.then_some(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub entrant: Entrant,
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl Standing {
    const fn new(entrant: Entrant) -> Self {
        Self {
            entrant,
            games: 0,
            wins: 0,
            draws: 0,
            losses: 0,
        }
    }

    const fn add(&mut self, outcome: Outcome) {
        self.games += 1;
        match outcome {
            Outcome::Win => self.wins += 1,
            Outcome::Draw => self.draws += 1,
            Outcome::Loss => self.losses += 1,
        }
    }

    const fn half_points(&self) -> u32 {
        Outcome::Win.half_points() * self.wins + Outcome::Draw.half_points() * self.draws
    }

    #[must_use]
    pub fn points(&self) -> f64 {
        f64::from(self.half_points()) / f64::from(Outcome::Win.half_points())
    }

    #[must_use]
    pub fn win_rate(&self) -> f64 {
        f64::from(self.wins) / f64::from(self.games)
    }
}

#[must_use]
pub fn standings(roster: &[Entrant], records: &[Record]) -> Vec<Standing> {
    let mut table: Vec<Standing> = roster
        .iter()
        .map(|&entrant| Standing::new(entrant))
        .collect();
    for record in records {
        credit(&mut table, record.first, record.outcome);
        credit(&mut table, record.second, record.outcome.flip());
    }
    table
}

fn credit(table: &mut [Standing], entrant: Entrant, outcome: Outcome) {
    if let Some(standing) = table
        .iter_mut()
        .find(|standing| standing.entrant == entrant)
    {
        standing.add(outcome);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rating {
    pub entrant: Entrant,
    pub elo: f64,
}

#[must_use]
pub fn elo_ratings(roster: &[Entrant], records: &[Record]) -> Option<Vec<Rating>> {
    let anchor = index_of(roster, ANCHOR)?;
    let model = fit(&elo_rows(roster, records, anchor))?;
    let mut logits = model.weights;
    logits.insert(anchor, 0.0);
    Some(
        roster
            .iter()
            .zip(logits)
            .map(|(&entrant, logit)| Rating {
                entrant,
                elo: ELO_PER_LOGIT * logit,
            })
            .collect(),
    )
}

fn index_of(roster: &[Entrant], entrant: Entrant) -> Option<usize> {
    roster.iter().position(|&listed| listed == entrant)
}

fn elo_rows(roster: &[Entrant], records: &[Record], anchor: usize) -> Vec<Row> {
    let width = roster.len() - 1;
    let mut rows = Vec::new();
    for record in records {
        let (Some(first), Some(second)) = (
            index_of(roster, record.first),
            index_of(roster, record.second),
        ) else {
            continue;
        };
        let mut inputs = vec![0.0; width];
        add_seat(&mut inputs, first, anchor, 1.0);
        add_seat(&mut inputs, second, anchor, -1.0);
        rows.push(Row {
            inputs,
            outcome: record.outcome,
        });
    }
    for index in (0..roster.len()).filter(|&index| index != anchor) {
        let mut inputs = vec![0.0; width];
        add_seat(&mut inputs, index, anchor, 1.0);
        rows.push(Row {
            inputs,
            outcome: Outcome::Draw,
        });
    }
    rows
}

fn add_seat(inputs: &mut [f64], index: usize, anchor: usize, sign: f64) {
    if index == anchor {
        return;
    }
    let column = if index < anchor { index } else { index - 1 };
    debug_assert!(column < inputs.len());
    inputs[column] += sign;
}
