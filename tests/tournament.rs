use std::collections::HashSet;
use std::ops::ControlFlow;
use std::time::Duration;

use c4::board::{B76, B98, Board, Geometry};
use c4::dataset::unique_samples;
use c4::outcome::Outcome;
use c4::tournament::{Entrant, ROSTER, Record, Settings, Standing, run, schedule, standings};

const SEED: u64 = 7;
const OPENING_COUNT: usize = 3;
const LOW: usize = 2;
const HIGH: usize = 5;
const PER_MOVE: Duration = Duration::from_millis(100);
const WORKERS: usize = 4;
const PERFECT_TABLE_BITS: u32 = 16;
const PAIR_ROSTER: [Entrant; 2] = [Entrant::SafeRandom, Entrant::Perfect];
const SETTINGS: Settings = Settings {
    per_move: PER_MOVE,
    perfect_table_bits: PERFECT_TABLE_BITS,
};

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
    let fixtures = schedule(&ROSTER, &openings);
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
            .map(|fixture| (fixture.first, fixture.second))
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
    let fixtures = schedule(&PAIR_ROSTER, &openings);
    let mut records: Vec<Record> = Vec::new();
    run(&fixtures, SETTINGS, WORKERS, |record| {
        records.push(record);
        ControlFlow::Continue(())
    });
    assert_eq!(records.len(), fixtures.len());
    records.sort_by_key(|record| record.game);
    for (fixture, record) in fixtures.iter().zip(&records) {
        assert_eq!(record.game, fixture.index);
        assert_eq!(record.opening, fixture.opening);
        assert_eq!(record.first, fixture.first);
        assert_eq!(record.second, fixture.second);
        replay(fixture.start, &record.moves);
    }
}

on_both_widths!(run_plays_every_fixture_legally, check_run);

fn check_early_stop<G: Geometry + 'static>() {
    let openings = unique_samples::<G>(SEED, OPENING_COUNT, LOW, HIGH);
    let fixtures = schedule(&PAIR_ROSTER, &openings);
    let mut seen = 0_usize;
    run(&fixtures, SETTINGS, WORKERS, |_| {
        seen += 1;
        ControlFlow::Break(())
    });
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

#[test]
fn standings_hand_computed() {
    let record = |first, second, outcome| Record {
        game: 0,
        opening: 0,
        first,
        second,
        outcome,
        moves: Vec::new(),
    };
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
    ];
    assert_eq!(table, expected);
    assert!((table[0].points() - 1.5).abs() < f64::EPSILON);
    assert!((table[3].points() - 0.5).abs() < f64::EPSILON);
}
