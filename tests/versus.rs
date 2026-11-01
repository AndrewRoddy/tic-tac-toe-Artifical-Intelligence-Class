use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::Duration;

use c4::board::{Board, Geometry, Grid};
use c4::dataset::Labelled;
use c4::outcome::Outcome;
use c4::tournament::{Deadline, Entrant, Record, Seat, Settings, run};
use c4::versus::{Summary, Verdict, fixtures};

#[path = "common/oracle.rs"]
pub mod oracle;

type Four64 = Grid<4, 4, u64>;
type Four128 = Grid<4, 4, u128>;

const UNLIMITED: Duration = Duration::from_secs(3600);
const TABLE_BITS: u32 = 18;
const MAX_STONES: u32 = 5;
const THREADS: usize = 2;

fn stones<G: Geometry>(board: Board<G>) -> u32 {
    G::COLUMNS * G::ROWS - board.empty_cells()
}

fn collect<G: Geometry>(board: Board<G>, out: &mut Vec<Board<G>>) {
    if stones(board) == MAX_STONES {
        return;
    }
    for col in board.playable() {
        if board.wins(col) {
            continue;
        }
        let mut child = board;
        child.play(col);
        out.push(child);
        collect(child, out);
    }
}

fn labelled_starts<G: Geometry>() -> Vec<Labelled<G>> {
    let root = Board::<G>::default();
    let mut starts = vec![root];
    collect(root, &mut starts);
    let mut memo = HashMap::new();
    starts
        .into_iter()
        .map(|board| Labelled {
            board,
            result: oracle::solve(board, &mut memo),
        })
        .collect()
}

fn play_all<G: Geometry + 'static>(agent: Entrant) -> (usize, Summary) {
    let openings = labelled_starts::<G>();
    let games = fixtures(
        &openings,
        Seat {
            entrant: agent,
            per_move: UNLIMITED,
        },
        UNLIMITED,
    );
    let mut summary = Summary::default();
    run(
        &games,
        Settings {
            perfect_table_bits: TABLE_BITS,
        },
        THREADS,
        |record| {
            summary.add(&record, openings[record.opening].result);
            ControlFlow::Continue(())
        },
    );
    (openings.len(), summary)
}

fn assert_all_played(openings: usize, summary: &Summary) {
    let total = summary.win.games + summary.draw.games + summary.loss.games;
    assert_eq!(
        usize::try_from(total + summary.expired + summary.solver_errors).ok(),
        openings.checked_mul(2)
    );
}

fn assert_never_beats_oracle<G: Geometry + 'static>() {
    let (openings, summary) = play_all::<G>(Entrant::SafeRandom);
    assert_all_played(openings, &summary);
    assert_eq!(summary.solver_errors, 0);
    assert_eq!(summary.expired, 0);
}

fn assert_perfect_keeps_oracle<G: Geometry + 'static>() {
    let (openings, summary) = play_all::<G>(Entrant::Perfect);
    assert_all_played(openings, &summary);
    assert_eq!(summary.solver_errors, 0);
    assert_eq!(summary.expired, 0);
    for row in [summary.win, summary.draw, summary.loss] {
        assert_eq!(row.kept, row.games);
    }
}

#[test]
fn random_never_beats_u64() {
    assert_never_beats_oracle::<Four64>();
}

#[test]
fn random_never_beats_u128() {
    assert_never_beats_oracle::<Four128>();
}

#[test]
fn perfect_keeps_value_u64() {
    assert_perfect_keeps_oracle::<Four64>();
}

#[test]
fn perfect_keeps_value_u128() {
    assert_perfect_keeps_oracle::<Four128>();
}

const fn game(game: u64, outcome: Outcome, deadlines: (Deadline, Deadline)) -> Record {
    Record {
        game,
        opening: 0,
        first: Entrant::Perfect,
        second: Entrant::SafeRandom,
        outcome,
        moves: Vec::new(),
        first_deadline: deadlines.0,
        second_deadline: deadlines.1,
    }
}

const MET: (Deadline, Deadline) = (Deadline::Met, Deadline::Met);

#[test]
fn verdict_by_seat_and_value() {
    let mut summary = Summary::default();
    let opening = Outcome::Win;
    let agent_first = |outcome| game(0, outcome, MET);
    let agent_second = |outcome| game(1, outcome, MET);
    assert_eq!(
        summary.add(&agent_first(Outcome::Win), opening),
        Verdict::Scored {
            value: Outcome::Win,
            kept: true
        }
    );
    assert_eq!(
        summary.add(&agent_first(Outcome::Draw), opening),
        Verdict::Scored {
            value: Outcome::Win,
            kept: false
        }
    );
    assert_eq!(
        summary.add(&agent_second(Outcome::Win), opening),
        Verdict::Scored {
            value: Outcome::Loss,
            kept: true
        }
    );
    assert_eq!(
        summary.add(&agent_second(Outcome::Loss), opening),
        Verdict::SolverError
    );
}

#[test]
fn missed_deadline_expires() {
    let mut summary = Summary::default();
    let verdict = summary.add(
        &game(0, Outcome::Loss, (Deadline::Met, Deadline::Missed)),
        Outcome::Win,
    );
    assert_eq!(verdict, Verdict::SolverExpired);
    let second = summary.add(
        &game(1, Outcome::Loss, (Deadline::Missed, Deadline::Met)),
        Outcome::Win,
    );
    assert_eq!(second, Verdict::SolverExpired);
    assert_eq!(summary.expired, 2);
    assert_eq!(
        summary.win.games + summary.draw.games + summary.loss.games,
        0
    );
}
