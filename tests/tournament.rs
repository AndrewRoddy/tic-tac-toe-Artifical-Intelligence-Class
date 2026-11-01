use std::collections::HashSet;
use std::ops::ControlFlow;
use std::time::Duration;

use c4::board::{B76, B98, Board, Geometry};
use c4::dataset::unique_samples;
use c4::outcome::Outcome;
use c4::tournament::{
    Deadline, Entrant, ROSTER, Rating, Record, Seat, Settings, Standing, elo_ratings, run,
    schedule, standings,
};

const SEED: u64 = 7;
const OPENING_COUNT: usize = 3;
const LOW: usize = 2;
const HIGH: usize = 5;
const PER_MOVE: Duration = Duration::from_millis(100);
const WORKERS: usize = 4;
const PERFECT_TABLE_BITS: u32 = 16;
const PAIR_ROSTER: [Entrant; 2] = [Entrant::SafeRandom, Entrant::Perfect];
const fn settings() -> Settings {
    Settings {
        perfect_table_bits: PERFECT_TABLE_BITS,
    }
}
const ELO_SCALE: f64 = 400.0;
const ELO_TOLERANCE: f64 = 0.5;
const TRANSITIVE_GAMES: u32 = 10_000;
const TRANSITIVE: [(Entrant, f64); 3] = [
    (Entrant::SafeRandom, 0.0),
    (Entrant::Learned, 120.0),
    (Entrant::Perfect, 310.0),
];

macro_rules! on_both_widths {
    ($name:ident, $check:ident) => {
        #[test]
        fn $name() {
            $check::<B76>();
            $check::<B98>();
        }
    };
}

fn replay<G: Geometry>(start: Board<G>, columns: &[u32]) -> Board<G> {
    let mut board = start;
    for &col in columns {
        assert!(
            col < G::COLUMNS && board.can_play(col),
            "illegal column {col}"
        );
        board.play(col);
    }
    board
}

fn check_schedule<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    let fixtures = schedule(&ROSTER, &openings, PER_MOVE);
    let per_opening = ROSTER.len() * (ROSTER.len() - 1);
    assert_eq!(fixtures.len(), OPENING_COUNT * per_opening);
    for (position, (fixture, expected_index)) in fixtures.iter().zip(0_u64..).enumerate() {
        assert_eq!(fixture.index, expected_index);
        assert_eq!(fixture.opening, position / per_opening);
    }
    for opening in 0..OPENING_COUNT {
        let pairs: HashSet<(Entrant, Entrant)> = fixtures
            .iter()
            .filter(|fixture| fixture.opening == opening)
            .map(|fixture| (fixture.first.entrant, fixture.second.entrant))
            .collect();
        assert_eq!(pairs.len(), per_opening);
        assert!(pairs.iter().all(|(first, second)| first != second));
    }
}

on_both_widths!(schedule_covers_pairs, check_schedule);

fn check_openings<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    assert_eq!(openings.len(), OPENING_COUNT);
    let distinct: HashSet<Vec<u32>> = openings
        .iter()
        .map(|sample| sample.columns.clone())
        .collect();
    assert_eq!(distinct.len(), OPENING_COUNT);
    for sample in &openings {
        assert!((LOW..=HIGH).contains(&sample.columns.len()));
        let replayed = replay(Board::<G>::default(), &sample.columns);
        assert!(sample.board.playable().eq(replayed.playable()));
        assert_eq!(sample.board.empty_cells(), replayed.empty_cells());
    }
}

on_both_widths!(openings_unique_and_in_range, check_openings);

fn check_run<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    let fixtures = schedule(&PAIR_ROSTER, &openings, PER_MOVE);
    let mut records: Vec<Record> = Vec::new();
    let finished = run(&fixtures, settings(), WORKERS, |record| {
        records.push(record);
        ControlFlow::Continue(())
    });
    assert!(finished.is_some());
    assert_eq!(records.len(), fixtures.len());
    records.sort_by_key(|record| record.game);
    for (fixture, record) in fixtures.iter().zip(&records) {
        assert_eq!(record.game, fixture.index);
        assert_eq!(record.opening, fixture.opening);
        assert_eq!(record.first, fixture.first.entrant);
        assert_eq!(record.second, fixture.second.entrant);
        replay(fixture.start, &record.moves);
    }
}

on_both_widths!(run_plays_every_fixture_legally, check_run);

fn check_early_stop<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    let fixtures = schedule(&PAIR_ROSTER, &openings, PER_MOVE);
    let mut seen = 0_usize;
    let finished = run(&fixtures, settings(), WORKERS, |_| {
        seen += 1;
        ControlFlow::Break(())
    });
    assert!(finished.is_some());
    assert_eq!(seen, 1);
}

on_both_widths!(sink_break_stops_run, check_early_stop);

const fn row(entrant: Entrant, [games, wins, draws, losses]: [u32; 4]) -> Standing {
    Standing {
        entrant,
        games,
        wins,
        draws,
        losses,
    }
}

const fn record(first: Entrant, second: Entrant, outcome: Outcome) -> Record {
    Record {
        game: 0,
        opening: 0,
        first,
        second,
        outcome,
        moves: Vec::new(),
        first_deadline: Deadline::Met,
        second_deadline: Deadline::Met,
    }
}

fn repeat(first: Entrant, second: Entrant, outcome: Outcome, count: u32) -> Vec<Record> {
    (0..count).map(|_| record(first, second, outcome)).collect()
}

fn fractional(first: Entrant, second: Entrant, score: f64, games: u32) -> Vec<Record> {
    (0..games)
        .map(|game| {
            let outcome = if f64::from(game) + 0.5 < score * f64::from(games) {
                Outcome::Win
            } else {
                Outcome::Loss
            };
            record(first, second, outcome)
        })
        .collect()
}

fn expected_score(rating: f64, other: f64) -> f64 {
    1.0 / (1.0 + 10_f64.powf((other - rating) / ELO_SCALE))
}

fn transitive_roster() -> Vec<Entrant> {
    TRANSITIVE.iter().map(|&(entrant, _)| entrant).collect()
}

fn transitive_records() -> Vec<Record> {
    let mut records = Vec::new();
    for (index, &(first, first_elo)) in TRANSITIVE.iter().enumerate() {
        for &(second, second_elo) in &TRANSITIVE[index + 1..] {
            let score = expected_score(first_elo, second_elo);
            records.extend(fractional(first, second, score, TRANSITIVE_GAMES));
        }
    }
    records
}

fn fit(roster: &[Entrant], records: &[Record]) -> Vec<Rating> {
    elo_ratings(roster, records).unwrap_or_default()
}

fn elo_of(ratings: &[Rating], entrant: Entrant) -> f64 {
    ratings
        .iter()
        .find(|rating| rating.entrant == entrant)
        .map_or(f64::NAN, |rating| rating.elo)
}

#[test]
fn standings_hand_computed() {
    let records = [
        record(Entrant::SafeRandom, Entrant::Perfect, Outcome::Win),
        record(Entrant::Perfect, Entrant::SafeRandom, Outcome::Draw),
        record(Entrant::SafeRandom, Entrant::Learned, Outcome::Loss),
    ];
    let table = standings(&ROSTER, &records);
    let expected = vec![
        row(Entrant::SafeRandom, [3, 1, 1, 1]),
        row(Entrant::Handcrafted, [0, 0, 0, 0]),
        row(Entrant::Learned, [1, 1, 0, 0]),
        row(Entrant::Perfect, [2, 0, 1, 1]),
        row(Entrant::Mcts, [0, 0, 0, 0]),
        row(Entrant::Network, [0, 0, 0, 0]),
        row(Entrant::NetworkMcts, [0, 0, 0, 0]),
        row(Entrant::Allis, [0, 0, 0, 0]),
    ];
    assert_eq!(table, expected);
    assert!((table[0].points() - 1.5).abs() < f64::EPSILON);
    assert!((table[3].points() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn elo_gap_closed_form() {
    let mut records = repeat(Entrant::Learned, Entrant::SafeRandom, Outcome::Win, 2000);
    records.extend(repeat(
        Entrant::Learned,
        Entrant::SafeRandom,
        Outcome::Draw,
        2000,
    ));
    let ratings = fit(&[Entrant::SafeRandom, Entrant::Learned], &records);
    let score = 0.75_f64;
    let expected = ELO_SCALE * (score / (1.0 - score)).log10();
    assert!((elo_of(&ratings, Entrant::Learned) - expected).abs() < ELO_TOLERANCE);
    assert!(elo_of(&ratings, Entrant::SafeRandom).abs() < ELO_TOLERANCE);
}

#[test]
fn transitive_recovers_ratings() {
    let ratings = fit(&transitive_roster(), &transitive_records());
    for (entrant, elo) in TRANSITIVE {
        assert!(
            (elo_of(&ratings, entrant) - elo).abs() < ELO_TOLERANCE,
            "{entrant:?}"
        );
    }
}

#[test]
fn wins_losses_closed_form() {
    let mut records = repeat(Entrant::Learned, Entrant::SafeRandom, Outcome::Win, 100);
    records.extend(repeat(
        Entrant::Perfect,
        Entrant::SafeRandom,
        Outcome::Loss,
        100,
    ));
    let roster = [Entrant::SafeRandom, Entrant::Learned, Entrant::Perfect];
    let ratings = fit(&roster, &records);
    let expected = ELO_SCALE * 201_f64.log10();
    assert!((elo_of(&ratings, Entrant::Learned) - expected).abs() < ELO_TOLERANCE);
    assert!((elo_of(&ratings, Entrant::Perfect) + expected).abs() < ELO_TOLERANCE);
}

fn check_deadlines<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    let mut fixtures = schedule(&PAIR_ROSTER, &openings, PER_MOVE);
    for fixture in &mut fixtures {
        fixture.first = Seat {
            entrant: Entrant::SafeRandom,
            per_move: Duration::ZERO,
        };
        fixture.second = Seat {
            entrant: Entrant::SafeRandom,
            per_move: Duration::from_secs(3600),
        };
    }
    let mut records: Vec<Record> = Vec::new();
    let finished = run(&fixtures, settings(), WORKERS, |record| {
        records.push(record);
        ControlFlow::Continue(())
    });
    assert!(finished.is_some());
    assert_eq!(records.len(), fixtures.len());
    for record in &records {
        assert_eq!(record.first_deadline, Deadline::Missed);
        assert_eq!(record.second_deadline, Deadline::Met);
    }
}

on_both_widths!(deadlines_follow_seat_pace, check_deadlines);
