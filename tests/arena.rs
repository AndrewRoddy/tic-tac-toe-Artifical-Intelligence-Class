use std::time::{Duration, Instant};

use c4::agent::{Agent, Perfect};
use c4::arena::{Game, Pace, play};
use c4::board::{Board, Geometry, Grid};
use c4::outcome::Outcome;
use c4::solver::Solver;

const TABLE_BITS: u32 = 16;
const PER_MOVE: Duration = Duration::from_secs(1);
const PACE: Pace = Pace {
    first: PER_MOVE,
    second: PER_MOVE,
};
const START_DEPTH: u32 = 4;

struct Column(u32);

impl<G: Geometry> Agent<G> for Column {
    fn choose(&mut self, _board: Board<G>, _deadline: Instant) -> u32 {
        self.0
    }
}

fn starts<G: Geometry>(board: Board<G>, depth: u32, found: &mut Vec<Board<G>>) {
    found.push(board);
    if depth == 0 {
        return;
    }
    for col in board.playable().filter(|&col| !board.wins(col)) {
        let mut child = board;
        child.play(col);
        starts(child, depth - 1, found);
    }
}

fn replays<G: Geometry>(start: Board<G>, game: &Game) {
    assert!(!game.moves.is_empty());
    let mut board = start;
    let mut mover_view = game.outcome;
    for (ply, &col) in game.moves.iter().enumerate() {
        let ending = if col >= G::COLUMNS || !board.can_play(col) {
            Some(Outcome::Loss)
        } else if board.wins(col) {
            Some(Outcome::Win)
        } else {
            board.play(col);
            board.playable().next().is_none().then_some(Outcome::Draw)
        };
        assert_eq!(ending.is_some(), ply + 1 == game.moves.len());
        if let Some(outcome) = ending {
            assert_eq!(mover_view, outcome);
            return;
        }
        mover_view = mover_view.flip();
    }
}

fn self_play<G: Geometry>(depth: u32) {
    let mut solver = Solver::<G>::new(TABLE_BITS);
    let mut first = Perfect::<G>::new(TABLE_BITS);
    let mut second = Perfect::<G>::new(TABLE_BITS);
    let mut found = Vec::new();
    starts(Board::<G>::default(), depth, &mut found);
    for start in found {
        let game = play(start, &mut first, &mut second, PACE);
        assert_eq!(Some(game.outcome), solver.solve(start, None));
        replays(start, &game);
    }
}

#[test]
fn perfect_play_matches_solver() {
    self_play::<Grid<4, 4, u64>>(START_DEPTH);
    self_play::<Grid<4, 4, u128>>(START_DEPTH);
}

fn forfeits<G: Geometry>() {
    let start = Board::<G>::default();
    let mut perfect = Perfect::<G>::new(TABLE_BITS);
    let off_board = G::COLUMNS;
    assert_eq!(
        play(start, &mut Column(off_board), &mut perfect, PACE).outcome,
        Outcome::Loss
    );
    assert_eq!(
        play(start, &mut perfect, &mut Column(off_board), PACE).outcome,
        Outcome::Win
    );
    assert_eq!(
        play(start, &mut Column(0), &mut Column(0), PACE).outcome,
        Outcome::Loss
    );
}

#[test]
fn illegal_column_forfeits() {
    forfeits::<Grid<4, 4, u64>>();
    forfeits::<Grid<4, 4, u128>>();
}

const UNEQUAL: Pace = Pace {
    first: Duration::from_secs(1),
    second: Duration::from_secs(3),
};

#[derive(Default)]
struct Recorder {
    deadlines: Vec<Instant>,
}

impl<G: Geometry> Agent<G> for Recorder {
    fn choose(&mut self, board: Board<G>, deadline: Instant) -> u32 {
        self.deadlines.push(deadline);
        board.playable().next().unwrap_or(0)
    }
}

#[test]
fn each_seat_gets_its_own_pace() {
    let mut first = Recorder::default();
    let mut second = Recorder::default();
    let before = Instant::now();
    play(
        Board::<Grid<4, 4, u64>>::default(),
        &mut first,
        &mut second,
        UNEQUAL,
    );
    let after = Instant::now();
    assert!(!first.deadlines.is_empty());
    assert!(!second.deadlines.is_empty());
    for deadline in &first.deadlines {
        assert!(before + UNEQUAL.first <= *deadline && *deadline <= after + UNEQUAL.first);
    }
    for deadline in &second.deadlines {
        assert!(before + UNEQUAL.second <= *deadline && *deadline <= after + UNEQUAL.second);
    }
}
