use std::collections::HashMap;
use std::error::Error;
use std::time::{Duration, Instant};

use c4::accuracy::Tally;
use c4::agent::{Agent, Perfect};
use c4::board::{Board, Geometry, Grid};
use c4::outcome::Outcome;

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::column;

type Small64 = Grid<4, 4, u64>;
type Small128 = Grid<4, 4, u128>;

const MAX_PLIES: u32 = 6;
const TABLE_BITS: u32 = 21;
const UNLIMITED: Duration = Duration::from_secs(3600);
const TOLERANCE: f64 = 1e-9;
const Z_SQUARED: f64 = 3.841_458_820_694_124;
const NEWCOMBE_TOLERANCE: f64 = 5.0e-5;
const TEN: u32 = 10;

macro_rules! on_both_widths {
    ($name:ident, $check:ident) => {
        #[test]
        fn $name() -> Result<(), Box<dyn Error>> {
            $check::<Small64>()?;
            $check::<Small128>()
        }
    };
}

fn near(value: Option<f64>, expected: f64) -> bool {
    value.is_some_and(|value| (value - expected).abs() < TOLERANCE)
}

#[test]
fn baseline_best_fraction() -> Result<(), Box<dyn Error>> {
    let mut tally = Tally::default();
    let four_legal = [
        Some(Outcome::Win),
        Some(Outcome::Loss),
        Some(Outcome::Loss),
        Some(Outcome::Draw),
    ];
    let three_legal = [
        Some(Outcome::Draw),
        Some(Outcome::Draw),
        Some(Outcome::Loss),
        None,
    ];
    tally.record(&four_legal, 0).ok_or("no label")?;
    tally.record(&three_legal, 2).ok_or("no label")?;
    assert_eq!(tally.positions(), 2);
    assert!(near(tally.accuracy(), 0.5), "{:?}", tally.accuracy());
    assert!(
        near(tally.baseline(), 11.0 / 24.0),
        "{:?}",
        tally.baseline()
    );
    Ok(())
}

#[test]
fn illegal_choice_is_incorrect() -> Result<(), Box<dyn Error>> {
    let mut tally = Tally::default();
    let three_legal = [
        Some(Outcome::Draw),
        Some(Outcome::Draw),
        Some(Outcome::Loss),
        None,
    ];
    tally.record(&three_legal, 3).ok_or("no label")?;
    assert!(near(tally.accuracy(), 0.0), "{:?}", tally.accuracy());
    Ok(())
}

#[test]
fn wilson_closed_forms() -> Result<(), Box<dyn Error>> {
    let columns = [Some(Outcome::Win), Some(Outcome::Loss)];
    let mut none_correct = Tally::default();
    let mut all_correct = Tally::default();
    for _ in 0..TEN {
        none_correct.record(&columns, 1).ok_or("no label")?;
        all_correct.record(&columns, 0).ok_or("no label")?;
    }
    let zero = none_correct.interval().ok_or("no interval")?;
    let total = f64::from(TEN);
    assert!(zero.low.abs() < TOLERANCE, "{}", zero.low);
    assert!(
        (zero.high - Z_SQUARED / (total + Z_SQUARED)).abs() < TOLERANCE,
        "{}",
        zero.high
    );
    let full = all_correct.interval().ok_or("no interval")?;
    assert!(
        (full.low - total / (total + Z_SQUARED)).abs() < TOLERANCE,
        "{}",
        full.low
    );
    assert!((full.high - 1.0).abs() < TOLERANCE, "{}", full.high);
    Ok(())
}

// Newcombe 1998, Wilson score method: 81 of 263 gives [0.2553, 0.3662].
#[test]
fn wilson_matches_newcombe() -> Result<(), Box<dyn Error>> {
    const CORRECT: u32 = 81;
    const TOTAL: u32 = 263;
    let columns = [Some(Outcome::Win), Some(Outcome::Loss)];
    let mut tally = Tally::default();
    for position in 0..TOTAL {
        let column = u32::from(position >= CORRECT);
        tally.record(&columns, column).ok_or("no label")?;
    }
    let interval = tally.interval().ok_or("no interval")?;
    assert!(
        (interval.low - 0.2553).abs() < NEWCOMBE_TOLERANCE,
        "{}",
        interval.low
    );
    assert!(
        (interval.high - 0.3662).abs() < NEWCOMBE_TOLERANCE,
        "{}",
        interval.high
    );
    Ok(())
}

fn gather<G: Geometry>(board: Board<G>, plies: u32, out: &mut Vec<Board<G>>) {
    out.push(board);
    if plies == 0 {
        return;
    }
    for col in board.playable().filter(|&col| !board.wins(col)) {
        let mut child = board;
        child.play(col);
        gather(child, plies - 1, out);
    }
}

fn perfect_exact<G: Geometry + 'static>() -> Result<(), Box<dyn Error>> {
    let mut positions = Vec::new();
    gather(Board::<G>::default(), MAX_PLIES, &mut positions);
    let mut memo = HashMap::new();
    let mut perfect = Perfect::<G>::new(TABLE_BITS);
    let mut tally = Tally::default();
    for board in positions {
        let columns: Vec<Option<Outcome>> = (0..G::COLUMNS)
            .map(|col| board.can_play(col).then(|| column(board, col, &mut memo)))
            .collect();
        let choice = perfect.choose(board, Instant::now() + UNLIMITED);
        tally
            .record(&columns, choice)
            .ok_or("position without legal column")?;
    }
    assert!(tally.positions() > 0);
    assert!(near(tally.accuracy(), 1.0), "{:?}", tally.accuracy());
    Ok(())
}

on_both_widths!(perfect_exact_4x4, perfect_exact);
