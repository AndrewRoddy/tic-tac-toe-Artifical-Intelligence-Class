use std::error::Error;
use std::time::{Duration, Instant};

use c4::board::{B76, Board, Geometry, Grid};
use c4::mcts::{Budget, Uct};
use c4::solver::Solver;

#[path = "common/pons.rs"]
pub mod pons;

type Small64 = Grid<4, 4, u64>;
type Small128 = Grid<4, 4, u128>;

const SEED: u64 = 11;
const SIMULATIONS: u32 = 2000;
const MAX_PLIES: u32 = 6;
const SOLVER_BITS: u32 = 21;
const HELD_OUT_STEP: usize = 10;
const HELD_OUT_OFFSET: usize = 5;
const UNLIMITED: Duration = Duration::from_secs(3600);

macro_rules! on_both_widths {
    ($name:ident, $check:ident) => {
        #[test]
        fn $name() -> Result<(), Box<dyn Error>> {
            $check::<Small64>()?;
            $check::<Small128>()
        }
    };
}

fn budget() -> Budget {
    Budget {
        deadline: Instant::now() + UNLIMITED,
        simulations: Some(SIMULATIONS),
    }
}

fn gather_positions<G: Geometry>(board: Board<G>, plies: u32, out: &mut Vec<Board<G>>) {
    out.push(board);
    if plies == 0 {
        return;
    }
    for col in board.playable() {
        if board.wins(col) {
            continue;
        }
        let mut next = board;
        next.play(col);
        gather_positions(next, plies - 1, out);
    }
}

fn immediate_wins<G: Geometry>(board: Board<G>) -> Vec<u32> {
    board.playable().filter(|&col| board.wins(col)).collect()
}

fn safe_columns<G: Geometry>(board: Board<G>) -> Vec<u32> {
    board
        .playable()
        .filter(|&col| {
            let mut next = board;
            next.play(col);
            !next.playable().any(|reply| next.wins(reply))
        })
        .collect()
}

fn takes_only_safe_column<G: Geometry + 'static>() -> Result<(), Box<dyn Error>> {
    let empty = Board::<G>::from_moves("").ok_or("empty board")?;
    let mut positions = Vec::new();
    gather_positions(empty, MAX_PLIES, &mut positions);
    let mut uct = Uct::<G>::new(SEED);
    let mut checked = 0_u32;
    for board in positions {
        if board.playable().count() < 2 || !immediate_wins(board).is_empty() {
            continue;
        }
        let safe = safe_columns(board);
        let &[only] = safe.as_slice() else {
            continue;
        };
        checked += 1;
        assert_eq!(uct.search(board, budget()), Some(only));
    }
    assert!(checked > 0);
    Ok(())
}

on_both_widths!(takes_only_safe_column_4x4, takes_only_safe_column);

fn takes_immediate_win<G: Geometry + 'static>() -> Result<(), Box<dyn Error>> {
    let empty = Board::<G>::from_moves("").ok_or("empty board")?;
    let mut positions = Vec::new();
    gather_positions(empty, MAX_PLIES, &mut positions);
    let mut uct = Uct::<G>::new(SEED);
    let mut checked = 0_u32;
    for board in positions {
        let wins = immediate_wins(board);
        if wins.is_empty() {
            continue;
        }
        checked += 1;
        assert!(
            uct.search(board, budget())
                .is_some_and(|col| wins.contains(&col))
        );
    }
    assert!(checked > 0);
    Ok(())
}

on_both_widths!(takes_immediate_win_4x4, takes_immediate_win);

#[test]
fn held_out_pons_beats_random() -> Result<(), Box<dyn Error>> {
    let mut solver = Solver::<B76>::new(SOLVER_BITS);
    let cases = pons::labelled(&mut solver, |index| {
        index % HELD_OUT_STEP == HELD_OUT_OFFSET
    })?;
    assert!(!cases.is_empty());
    let mut uct = Uct::<B76>::new(SEED);
    let mut hits = 0_u32;
    let mut random = 0.0_f64;
    for (sample, label) in &cases {
        let best = label
            .columns
            .iter()
            .enumerate()
            .filter(|(_, outcome)| **outcome == Some(label.result))
            .map(|(col, _)| u32::try_from(col))
            .collect::<Result<Vec<u32>, _>>()?;
        random += f64::from(u32::try_from(best.len())?)
            / f64::from(u32::try_from(sample.board.playable().count())?);
        if uct
            .search(sample.board, budget())
            .is_some_and(|col| best.contains(&col))
        {
            hits += 1;
        }
    }
    assert!(
        f64::from(hits) > random,
        "mcts {hits} random {random:.2} of {}",
        cases.len()
    );
    Ok(())
}
