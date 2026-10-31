use std::collections::HashSet;
use std::time::Instant;

use c4::agent::{Agent, SafeRandom};
use c4::board::{B87, B98, Board, Geometry, Grid};

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::{Partial, grid_wins};

const SEEDS: u64 = 4;
const EMPTY_SEEDS: u64 = 256;
const GAMES: u64 = 50;

#[derive(Clone)]
struct Position<G: Geometry> {
    board: Board<G>,
    grid: Partial,
}

fn start<G: Geometry>() -> Position<G> {
    Position {
        board: Board::default(),
        grid: Partial::default(),
    }
}

fn mover<G: Geometry>(position: &Position<G>) -> usize {
    usize::from(!position.board.first_to_move())
}

fn advance<G: Geometry>(position: &Position<G>, col: u32) -> Position<G> {
    let mut next = position.clone();
    next.grid.place(mover(position), col);
    next.board.play(col);
    next
}

fn check<G: Geometry>(position: &Position<G>) {
    let playable: Vec<u32> = position.board.playable().collect();
    if playable.is_empty() {
        return;
    }
    let side = mover(position);
    let own = &position.grid.stones[side];
    let other = &position.grid.stones[1 - side];
    let wins: Vec<u32> = playable
        .iter()
        .copied()
        .filter(|&col| grid_wins(own, position.grid.cell(col)))
        .collect();
    let blocks: Vec<u32> = playable
        .iter()
        .copied()
        .filter(|&col| grid_wins(other, position.grid.cell(col)))
        .collect();
    let Some(expected) = [wins, blocks, playable]
        .into_iter()
        .find(|set| !set.is_empty())
    else {
        return;
    };
    for seed in 0..SEEDS {
        let choice = SafeRandom::new(seed).choose(position.board, Instant::now());
        assert!(
            expected.contains(&choice),
            "seed {seed}: chose {choice}, expected one of {expected:?}"
        );
    }
}

fn visit<G: Geometry>(position: &Position<G>, seen: &mut HashSet<G::Bits>) {
    if !seen.insert(position.board.key()) {
        return;
    }
    check(position);
    for col in position.board.playable() {
        if !position.board.wins(col) {
            visit(&advance(position, col), seen);
        }
    }
}

fn exhaustive<G: Geometry>() {
    visit(&start::<G>(), &mut HashSet::new());
}

fn turn<G: Geometry>(agent: &mut SafeRandom, position: &Position<G>) -> Option<Position<G>> {
    position.board.playable().next()?;
    check(position);
    let col = agent.choose(position.board, Instant::now());
    (!position.board.wins(col)).then(|| advance(position, col))
}

fn random_game<G: Geometry>(seed: u64) {
    let mut agent = SafeRandom::new(seed);
    let mut position = start::<G>();
    while let Some(next) = turn(&mut agent, &position) {
        position = next;
    }
}

fn every_column_reachable<G: Geometry>() {
    let board = Board::<G>::default();
    let mut seen = HashSet::new();
    for seed in 0..EMPTY_SEEDS {
        seen.insert(SafeRandom::new(seed).choose(board, Instant::now()));
    }
    assert!((0..G::COLUMNS).all(|col| seen.contains(&col)));
}

#[test]
fn oracle_4x4_every_position() {
    exhaustive::<Grid<4, 4, u64>>();
    exhaustive::<Grid<4, 4, u128>>();
}

#[test]
fn oracle_random_games() {
    for seed in 0..GAMES {
        random_game::<B87>(seed);
        random_game::<B98>(seed);
    }
}

#[test]
fn empty_board_all_columns() {
    every_column_reachable::<Grid<4, 4, u64>>();
    every_column_reachable::<Grid<4, 4, u128>>();
    every_column_reachable::<B87>();
    every_column_reachable::<B98>();
}
