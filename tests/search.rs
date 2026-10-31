use std::{
    collections::HashMap,
    error::Error,
    time::{Duration, Instant},
};

use c4::board::{B76, Board, Geometry, Grid};
use c4::dataset::Labelled;
use c4::outcome::Outcome;
use c4::search::{Weights, best_column};
use c4::solver::{Label, Solver};

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::column;

#[path = "common/pons.rs"]
pub mod pons;

type Small64 = Grid<4, 4, u64>;
type Small128 = Grid<4, 4, u128>;

const PONS_DEPTH: u32 = 4;
const SOLVER_BITS: u32 = 21;
const SMALL_STONES: u32 = 4;
const SMALL_DEPTH: u32 = 16;
const SMALL_POSITIONS: u32 = 341;
const FIXTURE_DEPTH: u32 = 2;
const FIXTURES: [&str; 6] = ["212", "242", "2424", "3242", "4232", "121212"];
const UNLIMITED: Duration = Duration::from_secs(3600);

fn assert_best<G: Geometry>(board: Board<G>, depth: u32, memo: &mut HashMap<G::Bits, Outcome>) {
    let best = board
        .playable()
        .map(|col| column(board, col, memo))
        .max()
        .unwrap_or(Outcome::Draw);
    let chosen = best_column(board, depth, Instant::now() + UNLIMITED, Weights::DEFAULT);
    assert_eq!(chosen.map(|col| column(board, col, memo)), Some(best));
}

fn walk<G: Geometry>(board: Board<G>, stones: u32, memo: &mut HashMap<G::Bits, Outcome>) -> u32 {
    assert_best(board, SMALL_DEPTH, memo);
    let mut count = 1;
    if stones < SMALL_STONES {
        for col in board.playable().filter(|&col| !board.wins(col)) {
            let mut next = board;
            next.play(col);
            count += walk(next, stones + 1, memo);
        }
    }
    count
}

#[test]
fn full_depth_is_exact_u64() {
    let mut memo = HashMap::new();
    assert_eq!(
        walk(Board::<Small64>::default(), 0, &mut memo),
        SMALL_POSITIONS
    );
}

#[test]
fn full_depth_is_exact_u128() {
    let mut memo = HashMap::new();
    assert_eq!(
        walk(Board::<Small128>::default(), 0, &mut memo),
        SMALL_POSITIONS
    );
}

#[test]
fn shallow_fixtures_are_exact() -> Result<(), &'static str> {
    let mut memo = HashMap::new();
    for moves in FIXTURES {
        let board = Board::<Small64>::from_moves(moves).ok_or("illegal fixture")?;
        assert_best(board, FIXTURE_DEPTH, &mut memo);
    }
    Ok(())
}

fn hits(cases: &[(Labelled<B76>, Label)], weights: Weights) -> usize {
    cases
        .iter()
        .filter(|(sample, label)| {
            let Some(best) = label.columns.iter().flatten().max() else {
                return false;
            };
            let chosen = best_column(
                sample.board,
                PONS_DEPTH,
                Instant::now() + UNLIMITED,
                weights,
            );
            let picked = chosen
                .and_then(|col| usize::try_from(col).ok())
                .and_then(|index| label.columns.get(index))
                .copied()
                .flatten();
            picked == Some(*best)
        })
        .count()
}

#[test]
fn pons_threat_eval_beats_zero() -> Result<(), Box<dyn Error>> {
    let mut solver = Solver::<B76>::new(SOLVER_BITS);
    let cases = pons::labelled(&mut solver, |_| true)?;
    let weighted = hits(&cases, Weights::DEFAULT);
    let zero = hits(&cases, Weights::NONE);
    assert!(
        weighted > zero,
        "weighted {weighted} zero {zero} of {}",
        cases.len()
    );
    Ok(())
}
