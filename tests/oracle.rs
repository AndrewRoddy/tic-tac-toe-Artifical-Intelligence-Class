use std::{
    collections::{HashMap, HashSet},
    error::Error,
    time::Instant,
};

use c4::agent::{Agent, Perfect};
use c4::board::{B76, Board, Geometry, Grid};
use c4::dataset::parse_pons;
use c4::outcome::Outcome;
use c4::solver::Solver;

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::{column, solve};

// Pascal Pons, end-game test set Test_L3_R1: http://blog.gamesolver.org/data/Test_L3_R1
const PONS: &str = include_str!("data/Test_L3_R1");
const PONS_POSITIONS: usize = 1000;
const TABLE_BITS: u32 = 21;
const TINY_TABLE_BITS: u32 = 4;

fn pons_matches_oracle<G: Geometry>() -> Result<(), Box<dyn Error>> {
    assert_eq!(PONS.lines().count(), PONS_POSITIONS);
    let mut memo = HashMap::new();
    let mut solver = Solver::<G>::new(TABLE_BITS);
    for line in PONS.lines() {
        let sample = parse_pons::<G>(line).ok_or(line)?;
        assert_eq!(solve(sample.board, &mut memo), sample.result, "{line}");
        assert_eq!(
            solver.solve(sample.board, None),
            Some(sample.result),
            "{line}"
        );
    }
    Ok(())
}

#[test]
fn pons_positions_match_oracle() -> Result<(), Box<dyn Error>> {
    pons_matches_oracle::<B76>()?;
    pons_matches_oracle::<Grid<7, 6, u128>>()
}

fn walk<G: Geometry>(
    board: Board<G>,
    seen: &mut HashSet<G::Bits>,
    memo: &mut HashMap<G::Bits, Outcome>,
    solver: &mut Solver<G>,
) {
    if seen.insert(board.key()) {
        let label = solver.label(board, None);
        assert_eq!(solver.solve(board, None), Some(solve(board, memo)));
        assert_eq!(
            label.as_ref().map(|found| found.result),
            Some(solve(board, memo))
        );
        let truth: Vec<_> = (0..G::COLUMNS)
            .map(|col| board.can_play(col).then(|| column(board, col, memo)))
            .collect();
        assert_eq!(label.map(|found| found.columns), Some(truth));
        for col in board.playable().filter(|&col| !board.wins(col)) {
            let mut child = board;
            child.play(col);
            walk(child, seen, memo, solver);
        }
    }
}

#[test]
fn even_width_matches_oracle() {
    walk(
        Board::<Grid<4, 4, u64>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TABLE_BITS),
    );
    walk(
        Board::<Grid<4, 4, u128>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TABLE_BITS),
    );
}

#[test]
fn tiny_table_matches_oracle() {
    walk(
        Board::<Grid<4, 4, u64>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TINY_TABLE_BITS),
    );
    walk(
        Board::<Grid<4, 4, u128>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TINY_TABLE_BITS),
    );
}

fn expired_walk<G: Geometry>(
    board: Board<G>,
    seen: &mut HashSet<G::Bits>,
    memo: &mut HashMap<G::Bits, Outcome>,
    solver: &mut Solver<G>,
) {
    if seen.insert(board.key()) {
        let aborted = solver.solve(board, Some(Instant::now()));
        assert!(aborted.is_none_or(|found| found == solve(board, memo)));
        assert_eq!(solver.solve(board, None), Some(solve(board, memo)));
        for col in board.playable().filter(|&col| !board.wins(col)) {
            let mut child = board;
            child.play(col);
            expired_walk(child, seen, memo, solver);
        }
    }
}

#[test]
fn expired_solves_store_nothing() {
    expired_walk(
        Board::<Grid<4, 4, u64>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TABLE_BITS),
    );
    expired_walk(
        Board::<Grid<4, 4, u128>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
        &mut Solver::new(TABLE_BITS),
    );
}

fn expired_root<G: Geometry>() {
    let expired = Some(Instant::now());
    assert_eq!(
        Solver::<G>::new(TABLE_BITS).solve(Board::default(), expired),
        None
    );
}

#[test]
fn expired_root_returns_none() {
    expired_root::<Grid<4, 4, u64>>();
    expired_root::<Grid<4, 4, u128>>();
}

fn expired_policy_walk<G: Geometry>(
    board: Board<G>,
    seen: &mut HashSet<G::Bits>,
    memo: &mut HashMap<G::Bits, Outcome>,
) {
    if seen.insert(board.key()) {
        let truth: Vec<Option<Outcome>> = (0..G::COLUMNS)
            .map(|col| board.can_play(col).then(|| column(board, col, memo)))
            .collect();
        let has_loss = truth.iter().flatten().any(|&found| found == Outcome::Loss);
        let has_safe = truth.iter().flatten().any(|&found| found != Outcome::Loss);
        let proven = |col: u32| {
            let mut child = board;
            child.play(col);
            child.can_win()
        };
        let every_loss_proven = (0..G::COLUMNS)
            .zip(truth.iter().copied())
            .all(|(col, found)| found != Some(Outcome::Loss) || proven(col));
        if has_loss && has_safe && every_loss_proven {
            let mut perfect = Perfect::<G>::new(TINY_TABLE_BITS);
            let col = perfect.choose(board, Instant::now());
            let chosen = (0..G::COLUMNS)
                .zip(truth.iter().copied())
                .find(|&(each, _)| each == col)
                .and_then(|(_, found)| found);
            assert!(chosen.is_some_and(|found| found != Outcome::Loss));
        }
        for col in board.playable().filter(|&col| !board.wins(col)) {
            let mut child = board;
            child.play(col);
            expired_policy_walk(child, seen, memo);
        }
    }
}

#[test]
fn expired_policy_avoids_loss() {
    expired_policy_walk(
        Board::<Grid<4, 4, u64>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
    );
    expired_policy_walk(
        Board::<Grid<4, 4, u128>>::default(),
        &mut HashSet::new(),
        &mut HashMap::new(),
    );
}
